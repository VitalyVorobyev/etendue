//! The Blender backend's EXR conventions, pinned against a real render
//! (`tests/data/blender_probe.exr`, from `tests/data/blender_probe.py`): the
//! Combined pass is found, and rows come top first — the red square Blender
//! drew left of and above the optical axis fills the top of the left half.
#![cfg(feature = "images")]

use std::path::Path;

use etendue_synth::images::read_exr_combined;

#[test]
fn blender_multilayer_exr_reads_top_row_first() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/blender_probe.exr");
    let img = read_exr_combined(&path).unwrap();
    assert_eq!((img.width, img.height), (64, 48));
    let red = |x: u32, y: u32| {
        let k = 3 * (y * img.width + x) as usize;
        img.rgb[k] > 0.5 && img.rgb[k + 1] < 0.1 && img.rgb[k + 2] < 0.1
    };
    let count = |xs: std::ops::Range<u32>, ys: std::ops::Range<u32>| {
        ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| red(x, y))
            .count()
    };
    // The square spans y ∈ [−0.05, 0.95] m at 2 m: from just below the axis to
    // beyond the top edge, all of it left of the axis.
    let top_left = count(0..32, 0..12);
    assert!(top_left > 150, "top-left red pixels: {top_left}");
    assert_eq!(count(32..64, 0..48), 0, "right half");
    assert_eq!(count(0..64, 36..48), 0, "bottom quarter");
}
