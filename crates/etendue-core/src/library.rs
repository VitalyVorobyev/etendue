//! Library items (ADR 0010) beyond their structure: what the CLI and the studio both need.

use etendue_scene::{LIBRARY_VERSION, LibraryItem};
use sha2::{Digest, Sha256};

/// The SHA-256 (hex) of an item's content: its JSON with `origin` left out and the version set to
/// [`LIBRARY_VERSION`], so a project's copy hashes as its source did, whichever version either
/// file was written in. A copy records its source's hash in `origin.sha256`.
#[must_use]
pub fn content_hash(item: &LibraryItem) -> String {
    let mut canonical = item.clone();
    canonical.origin = None;
    canonical.version = LIBRARY_VERSION;
    let json = serde_json::to_vec(&canonical).unwrap_or_default();
    Sha256::digest(&json)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use etendue_scene::{ItemOrigin, Layer};

    #[test]
    fn a_copy_hashes_as_its_source_and_a_change_does_not() {
        let text = r#"{ "version": 1, "id": "imx174", "name": "Sony IMX174", "sensor": { "pixel_pitch_m": 5.86e-6, "resolution": [1936, 1216] } }"#;
        let source: LibraryItem = serde_json::from_str(text).unwrap();
        let hash = content_hash(&source);
        assert_eq!(hash.len(), 64);
        let mut copy = source.clone();
        copy.version = LIBRARY_VERSION;
        copy.origin = Some(ItemOrigin {
            layer: Layer::Bank,
            reference: "sensors/imx174".into(),
            sha256: hash.clone(),
        });
        assert_eq!(content_hash(&copy), hash);
        copy.sensor.as_mut().unwrap().pixel_pitch_m = 5.0e-6;
        assert_ne!(content_hash(&copy), hash);
    }
}
