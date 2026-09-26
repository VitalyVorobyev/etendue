//! Motion primitives: rest-to-rest trapezoidal profiles and natural cubic
//! splines with exact derivative bounds.

/// A rest-to-rest trapezoidal (or triangular) profile moving a path
/// parameter from 0 to 1, optionally slowed down by a uniform time scale.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Trapezoid {
    /// Acceleration magnitude of the unscaled profile.
    a: f64,
    /// Cruise (peak) speed of the unscaled profile.
    v: f64,
    /// Acceleration phase length of the unscaled profile.
    t_acc: f64,
    /// Duration of the unscaled profile.
    natural: f64,
    /// Duration after scaling (`≥ natural`).
    duration: f64,
}

impl Trapezoid {
    /// The fastest profile covering unit distance with `|ṡ| ≤ v_max` and
    /// `|s̈| ≤ a_max` (both `> 0`).
    pub(crate) fn fastest(v_max: f64, a_max: f64) -> Self {
        let (t_acc, v, natural) = if v_max * v_max / a_max >= 1.0 {
            // Triangular: accelerate to the midpoint, decelerate back.
            let t_acc = (1.0 / a_max).sqrt();
            (t_acc, a_max * t_acc, 2.0 * t_acc)
        } else {
            let t_acc = v_max / a_max;
            (t_acc, v_max, 1.0 / v_max + t_acc)
        };
        Self {
            a: a_max,
            v,
            t_acc,
            natural,
            duration: natural,
        }
    }

    /// Natural (unscaled) duration.
    pub(crate) fn natural_duration(&self) -> f64 {
        self.natural
    }

    /// The same path, uniformly slowed down to `duration ≥ natural`. Speeds
    /// scale by `natural / duration` and accelerations by its square, so
    /// limits that held before still hold.
    pub(crate) fn stretched(mut self, duration: f64) -> Self {
        debug_assert!(duration >= self.natural);
        self.duration = duration;
        self
    }

    /// `(s, ṡ, s̈)` at time `t` (clamped to `[0, duration]`).
    pub(crate) fn eval(&self, t: f64) -> (f64, f64, f64) {
        let r = self.natural / self.duration;
        let tau = (t * r).clamp(0.0, self.natural);
        let (s, sd, sdd) = if tau < self.t_acc {
            (0.5 * self.a * tau * tau, self.a * tau, self.a)
        } else if tau < self.natural - self.t_acc {
            (
                0.5 * self.a * self.t_acc * self.t_acc + self.v * (tau - self.t_acc),
                self.v,
                0.0,
            )
        } else if tau < self.natural {
            let rem = self.natural - tau;
            (1.0 - 0.5 * self.a * rem * rem, self.a * rem, -self.a)
        } else {
            (1.0, 0.0, 0.0)
        };
        (s, sd * r, sdd * r * r)
    }
}

/// A natural cubic spline through `values` at uniform knots
/// `s_k = k / (n − 1)` on `[0, 1]`.
#[derive(Clone, Debug)]
pub(crate) struct Spline {
    h: f64,
    y: Vec<f64>,
    /// Second derivatives at the knots (`0` at both ends).
    m: Vec<f64>,
}

impl Spline {
    /// Interpolate `values` (at least two).
    pub(crate) fn natural(values: Vec<f64>) -> Self {
        let n = values.len();
        assert!(n >= 2, "a spline needs at least two knots");
        let h = 1.0 / (n - 1) as f64;
        let mut m = vec![0.0; n];
        if n > 2 {
            // Thomas algorithm for M_{k-1} + 4 M_k + M_{k+1} = 6/h² Δ²y_k,
            // k = 1..n-2, with M_0 = M_{n-1} = 0.
            let rhs: Vec<f64> = (1..n - 1)
                .map(|k| 6.0 / (h * h) * (values[k + 1] - 2.0 * values[k] + values[k - 1]))
                .collect();
            let len = rhs.len();
            let mut c = vec![0.0; len];
            let mut d = vec![0.0; len];
            c[0] = 1.0 / 4.0;
            d[0] = rhs[0] / 4.0;
            for i in 1..len {
                let denom = 4.0 - c[i - 1];
                c[i] = 1.0 / denom;
                d[i] = (rhs[i] - d[i - 1]) / denom;
            }
            m[len] = d[len - 1];
            for i in (0..len - 1).rev() {
                m[i + 1] = d[i] - c[i] * m[i + 2];
            }
        }
        Self { h, y: values, m }
    }

    fn segment(&self, s: f64) -> (usize, f64) {
        let last = self.y.len() - 2;
        let k = ((s / self.h).floor().max(0.0) as usize).min(last);
        (k, s - k as f64 * self.h)
    }

    /// `(y, y', y'')` at `s ∈ [0, 1]`.
    pub(crate) fn eval(&self, s: f64) -> (f64, f64, f64) {
        let (k, u) = self.segment(s.clamp(0.0, 1.0));
        let h = self.h;
        let w = h - u;
        let (m0, m1, y0, y1) = (self.m[k], self.m[k + 1], self.y[k], self.y[k + 1]);
        let a0 = y0 / h - m0 * h / 6.0;
        let a1 = y1 / h - m1 * h / 6.0;
        // Exactly on a knot, return the knot value itself: the cubic formula
        // reproduces it only to an ULP, and a motion must start exactly where
        // the previous step (or capture) left the robot.
        let y = if u == 0.0 {
            y0
        } else {
            m0 * w * w * w / (6.0 * h) + m1 * u * u * u / (6.0 * h) + a0 * w + a1 * u
        };
        let yd = -m0 * w * w / (2.0 * h) + m1 * u * u / (2.0 * h) - a0 + a1;
        let ydd = (m0 * w + m1 * u) / h;
        (y, yd, ydd)
    }

    /// Exact `max |y'|` and `max |y''|` over `[0, 1]`.
    pub(crate) fn derivative_bounds(&self) -> (f64, f64) {
        let h = self.h;
        let max2 = self.m.iter().fold(0.0_f64, |acc, v| acc.max(v.abs()));
        let mut max1 = 0.0_f64;
        for k in 0..self.y.len() - 1 {
            let s0 = k as f64 * h;
            let (_, d0, _) = self.eval(s0);
            let (_, d1, _) = self.eval(s0 + h);
            max1 = max1.max(d0.abs()).max(d1.abs());
            // y'' is linear on the segment; y' has an extremum where it
            // crosses zero.
            let (m0, m1) = (self.m[k], self.m[k + 1]);
            if m0 * m1 < 0.0 {
                let u = h * m0 / (m0 - m1);
                let (_, d, _) = self.eval(s0 + u);
                max1 = max1.max(d.abs());
            }
        }
        (max1, max2)
    }

    /// Exact `(min y, max y)` over `[0, 1]`.
    pub(crate) fn range(&self) -> (f64, f64) {
        let h = self.h;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for k in 0..self.y.len() - 1 {
            let s0 = k as f64 * h;
            let mut consider = |s: f64| {
                let (y, _, _) = self.eval(s);
                lo = lo.min(y);
                hi = hi.max(y);
            };
            consider(s0);
            consider(s0 + h);
            // y'(u) = A u² + B u + C on the segment, u ∈ (0, h).
            let (m0, m1) = (self.m[k], self.m[k + 1]);
            let c = (self.y[k + 1] - self.y[k]) / h - (m1 - m0) * h / 6.0;
            let (qa, qb, qc) = ((m1 - m0) / (2.0 * h), m0, c - m0 * h / 2.0);
            let mut roots = [f64::NAN; 2];
            if qa.abs() < 1e-300 {
                if qb != 0.0 {
                    roots[0] = -qc / qb;
                }
            } else {
                let disc = qb * qb - 4.0 * qa * qc;
                if disc >= 0.0 {
                    let sq = disc.sqrt();
                    let q = -0.5 * (qb + qb.signum() * sq);
                    roots[0] = q / qa;
                    if q != 0.0 {
                        roots[1] = qc / q;
                    }
                }
            }
            for u in roots {
                if u > 0.0 && u < h {
                    consider(s0 + u);
                }
            }
        }
        (lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trapezoid_reaches_one_at_rest_within_limits() {
        for (v, a) in [(1.0, 1.0), (0.5, 4.0), (3.0, 0.7), (10.0, 100.0)] {
            let p = Trapezoid::fastest(v, a);
            let (s, sd, _) = p.eval(p.natural_duration());
            assert!((s - 1.0).abs() < 1e-12 && sd.abs() < 1e-12);
            let n = 10_000;
            for i in 0..=n {
                let (_, sd, sdd) = p.eval(p.natural_duration() * i as f64 / n as f64);
                assert!(sd <= v * (1.0 + 1e-12) && sdd.abs() <= a * (1.0 + 1e-12));
            }
            let slow = p.stretched(2.0 * p.natural_duration());
            let (s, sd, _) = slow.eval(2.0 * p.natural_duration());
            assert!((s - 1.0).abs() < 1e-12 && sd.abs() < 1e-12);
        }
    }

    #[test]
    fn spline_interpolates_and_bounds_are_exact() {
        let ys: Vec<f64> = (0..=20).map(|k| (k as f64 * 0.3).sin()).collect();
        let sp = Spline::natural(ys.clone());
        for (k, y) in ys.iter().enumerate() {
            let (v, _, _) = sp.eval(k as f64 / 20.0);
            assert!((v - y).abs() < 1e-12);
        }
        // Bit-exact at the start knot (where motions begin).
        assert_eq!(sp.eval(0.0).0, ys[0]);
        let (d1, d2) = sp.derivative_bounds();
        let (lo, hi) = sp.range();
        let n = 200_000;
        for i in 0..=n {
            let (y, yd, ydd) = sp.eval(i as f64 / n as f64);
            assert!(yd.abs() <= d1 * (1.0 + 1e-9) + 1e-12);
            assert!(ydd.abs() <= d2 * (1.0 + 1e-9) + 1e-12);
            assert!(y >= lo - 1e-12 && y <= hi + 1e-12);
        }
    }

    #[test]
    fn two_knot_spline_is_linear() {
        let sp = Spline::natural(vec![1.0, 3.0]);
        let (y, yd, ydd) = sp.eval(0.25);
        assert!((y - 1.5).abs() < 1e-15 && (yd - 2.0).abs() < 1e-15 && ydd == 0.0);
    }
}
