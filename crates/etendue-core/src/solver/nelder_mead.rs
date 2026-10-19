//! A deterministic Nelder–Mead simplex minimiser.
//!
//! No randomness and no dependencies, so it builds for wasm (ADR 0008 §7). The coefficients are
//! the standard ones (reflection 1, expansion 2, contraction ½, shrink ½), and the search stops
//! when the standard deviation of the costs at the simplex's vertices falls below a tolerance,
//! or after a number of iterations.
//!
//! A non-finite cost is treated as `+∞`, so a cost function can reject a point by returning
//! `f64::INFINITY` (or NaN) and the simplex retreats from it.

/// Options of [`minimise`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// Stop when the standard deviation of the vertex costs is at most this.
    pub cost_tolerance: f64,
    /// Stop after this many iterations.
    pub max_iterations: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            cost_tolerance: 1e-6,
            max_iterations: 500,
        }
    }
}

/// The outcome of [`minimise`].
#[derive(Clone, Debug, PartialEq)]
pub struct Minimum {
    /// The best vertex found.
    pub point: Vec<f64>,
    /// Its cost.
    pub cost: f64,
    /// Iterations run.
    pub iterations: u32,
    /// Whether the cost tolerance was met (rather than the iteration cap).
    pub converged: bool,
}

fn finite(cost: f64) -> f64 {
    if cost.is_nan() { f64::INFINITY } else { cost }
}

/// Minimise `cost` from `simplex`: `n + 1` vertices of dimension `n`.
///
/// # Panics
///
/// If `simplex` does not have `n + 1` vertices of one dimension `n ≥ 1`.
pub fn minimise(
    mut cost: impl FnMut(&[f64]) -> f64,
    simplex: Vec<Vec<f64>>,
    options: Options,
) -> Minimum {
    let n = simplex.first().map_or(0, Vec::len);
    assert!(
        n >= 1 && simplex.len() == n + 1,
        "a simplex needs n + 1 vertices of dimension n ≥ 1"
    );
    assert!(
        simplex.iter().all(|v| v.len() == n),
        "simplex vertices differ in dimension"
    );
    let mut vertices: Vec<(Vec<f64>, f64)> = simplex
        .into_iter()
        .map(|v| {
            let c = finite(cost(&v));
            (v, c)
        })
        .collect();
    let combine = |a: &[f64], b: &[f64], t: f64| -> Vec<f64> {
        a.iter().zip(b).map(|(x, y)| x + t * (y - x)).collect()
    };

    let mut iterations = 0;
    let mut converged = false;
    loop {
        // Best first; a stable sort keeps ties in vertex order, so the run is deterministic.
        vertices.sort_by(|a, b| a.1.total_cmp(&b.1));
        let costs: Vec<f64> = vertices.iter().map(|v| v.1).collect();
        if costs.iter().all(|c| c.is_finite()) {
            let mean = costs.iter().sum::<f64>() / costs.len() as f64;
            let sd =
                (costs.iter().map(|c| (c - mean).powi(2)).sum::<f64>() / costs.len() as f64).sqrt();
            if sd <= options.cost_tolerance {
                converged = true;
                break;
            }
        }
        if iterations >= options.max_iterations {
            break;
        }
        iterations += 1;

        let (best, second_worst, worst) = (costs[0], costs[n - 1], costs[n]);
        let centroid: Vec<f64> = (0..n)
            .map(|i| vertices[..n].iter().map(|v| v.0[i]).sum::<f64>() / n as f64)
            .collect();
        let worst_point = vertices[n].0.clone();
        // Reflect the worst vertex through the centroid of the others.
        let reflected = combine(&centroid, &worst_point, -1.0);
        let reflected_cost = finite(cost(&reflected));
        if reflected_cost < best {
            let expanded = combine(&centroid, &worst_point, -2.0);
            let expanded_cost = finite(cost(&expanded));
            vertices[n] = if expanded_cost < reflected_cost {
                (expanded, expanded_cost)
            } else {
                (reflected, reflected_cost)
            };
            continue;
        }
        if reflected_cost < second_worst {
            vertices[n] = (reflected, reflected_cost);
            continue;
        }
        // Contract toward the better of the worst vertex and its reflection.
        let (towards, towards_cost) = if reflected_cost < worst {
            (reflected, reflected_cost)
        } else {
            (worst_point, worst)
        };
        let contracted = combine(&centroid, &towards, 0.5);
        let contracted_cost = finite(cost(&contracted));
        if contracted_cost < towards_cost {
            vertices[n] = (contracted, contracted_cost);
            continue;
        }
        // Shrink everything toward the best vertex.
        let anchor = vertices[0].0.clone();
        for v in &mut vertices[1..] {
            let p = combine(&anchor, &v.0, 0.5);
            let c = finite(cost(&p));
            *v = (p, c);
        }
    }
    vertices.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (point, cost) = vertices.swap_remove(0);
    Minimum {
        point,
        cost,
        iterations,
        converged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_minimum_of_a_rosenbrock_valley() {
        let rosenbrock = |x: &[f64]| (1.0 - x[0]).powi(2) + 100.0 * (x[1] - x[0] * x[0]).powi(2);
        let m = minimise(
            rosenbrock,
            vec![vec![-1.2, 1.0], vec![-1.1, 1.0], vec![-1.2, 1.1]],
            Options {
                cost_tolerance: 1e-14,
                max_iterations: 5000,
            },
        );
        assert!(m.converged);
        assert!(
            (m.point[0] - 1.0).abs() < 1e-4 && (m.point[1] - 1.0).abs() < 1e-4,
            "{:?}",
            m.point
        );
    }

    #[test]
    fn retreats_from_rejected_points_and_is_deterministic() {
        // A bowl at (0.3, -0.2), refusing x > 0.5.
        let bowl = |x: &[f64]| {
            if x[0] > 0.5 {
                f64::NAN
            } else {
                (x[0] - 0.3).powi(2) + (x[1] + 0.2).powi(2)
            }
        };
        let start = vec![vec![0.45, 0.0], vec![0.49, 0.0], vec![0.45, 0.05]];
        let a = minimise(
            bowl,
            start.clone(),
            Options {
                cost_tolerance: 1e-16,
                max_iterations: 2000,
            },
        );
        let b = minimise(
            bowl,
            start,
            Options {
                cost_tolerance: 1e-16,
                max_iterations: 2000,
            },
        );
        assert_eq!(a, b);
        assert!(
            (a.point[0] - 0.3).abs() < 1e-6 && (a.point[1] + 0.2).abs() < 1e-6,
            "{:?}",
            a.point
        );
    }
}
