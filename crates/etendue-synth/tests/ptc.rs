//! P4-6 (docs/pivot/PLAN.md): the photon-transfer curve of the sensor model reproduces the
//! configured conversion gain within 2 %.
//!
//! EMVA 1288 two-frame method: at each of 12 flat-field levels, two frames A and B; the mean
//! signal is `mean(A + B)/2 − black level` and the temporal variance `var(A − B)/2` (PRNU
//! cancels in the difference). Below 70 % of full well the variance is
//! `K · signal + K²σ_read² + 1/12`, so the least-squares slope estimates K.

use etendue_synth::sensor::SensorModel;

const W: u32 = 256;
const H: u32 = 256;

fn sensor() -> SensorModel {
    SensorModel {
        electrons_per_unit: 10_000.0,
        rgb_weights: [0.2126, 0.7152, 0.0722],
        prnu: 0.01,
        dark_electrons: 5.0,
        read_noise_electrons: 3.0,
        full_well_electrons: 12_000.0,
        gain_dn_per_electron: 0.25,
        black_level_dn: 64.0,
        bits: 12,
        seed: 1288,
    }
}

#[test]
fn p4_6_photon_transfer_recovers_the_gain() {
    let m = sensor();
    let n = (W * H) as usize;
    let mut points = Vec::new();
    for level in 0..12 {
        // Radiance so that the mean signal spans 0 … ~66 % of full well.
        let radiance = f64::from(level) * 0.07;
        let rgb = vec![radiance as f32; 3 * n];
        let a = m.expose(&rgb, W, H, 2 * level as u64).unwrap();
        let b = m.expose(&rgb, W, H, 2 * level as u64 + 1).unwrap();
        let (mut sum, mut diff, mut diff2) = (0.0_f64, 0.0_f64, 0.0_f64);
        for (x, y) in a.dn.iter().zip(&b.dn) {
            let (x, y) = (f64::from(*x), f64::from(*y));
            sum += x + y;
            diff += x - y;
            diff2 += (x - y) * (x - y);
        }
        let mean = sum / (2.0 * n as f64) - m.black_level_dn;
        let var = (diff2 / n as f64 - (diff / n as f64).powi(2)) / 2.0;
        points.push((mean, var));
    }
    let k = points.len() as f64;
    let (sx, sy): (f64, f64) = points
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let (mx, my) = (sx / k, sy / k);
    let slope = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>()
        / points.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>();
    let intercept = my - slope * mx;
    // Read noise and dark-current shot noise, plus quantisation.
    let k2 = m.gain_dn_per_electron.powi(2);
    let expected_intercept = k2 * (m.read_noise_electrons.powi(2) + m.dark_electrons) + 1.0 / 12.0;
    println!(
        "PTC: K̂ = {slope:.5} DN/e (configured {}), intercept {intercept:.3} DN² (expected ≈ {expected_intercept:.3})",
        m.gain_dn_per_electron
    );
    for (mean, var) in &points {
        println!("  mean {mean:9.2} DN   var {var:8.3} DN²");
    }
    let rel = (slope / m.gain_dn_per_electron - 1.0).abs();
    assert!(rel <= 0.02, "K̂ off by {:.2} %", 100.0 * rel);
}
