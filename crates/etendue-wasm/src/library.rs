//! Library items and procedure rules, for the studio's library and procedure editors.

use etendue_scene::ValidationError;
use wasm_bindgen::prelude::*;

use crate::{Error, Result, js_error, parse};

/// A library item (ADR 0008 §5) parsed and checked: its version first, then its fields, its single
/// part, and, for a camera design with optics, its params against what the optics give.
///
/// # Errors
///
/// [`Error::Invalid`] with every problem found.
pub fn validate_library_item(text: &str) -> Result<etendue_scene::LibraryItem> {
    let version: etendue_scene::Versioned = parse("library item", text)?;
    etendue_scene::LibraryItem::check_version(version.version)
        .map_err(|e| Error::invalid("library item", e))?;
    let item: etendue_scene::LibraryItem = parse("library item", text)?;
    let mut issues = item.validate().err().map(|e| e.issues).unwrap_or_default();
    if let Some(c) = &item.camera
        && let Some(o) = &c.optics
    {
        issues.extend(etendue_core::from_scene::check_derived(
            o, &c.params, "camera",
        ));
    }
    if issues.is_empty() {
        Ok(item)
    } else {
        Err(Error::invalid("library item", ValidationError { issues }))
    }
}

/// A library item, checked, as JSON text. See [`validate_library_item`].
///
/// # Errors
///
/// Throws an `Error` with `kind` (and `issues` for validation errors).
#[wasm_bindgen(js_name = validateLibraryItem)]
pub fn validate_library_item_js(text: &str) -> std::result::Result<String, JsValue> {
    let item = validate_library_item(text).map_err(|e| js_error(&e))?;
    serde_json::to_string(&item).map_err(|e| js_error(&Error::Input(e.to_string())))
}

/// A library item's content hash ([`etendue_core::library::content_hash`], ADR 0010): what a
/// project's copy records of its source, to tell later whether the source changed.
///
/// # Errors
///
/// Throws as [`validate_library_item_js`] for an item that does not check.
#[wasm_bindgen(js_name = libraryItemHash)]
pub fn library_item_hash_js(text: &str) -> std::result::Result<String, JsValue> {
    let item = validate_library_item(text).map_err(|e| js_error(&e))?;
    Ok(etendue_core::library::content_hash(&item))
}

/// The rules of calibration procedures ([`etendue_scene::procedure_rules`]) as JSON text: which
/// options and seeds each workflow takes, so an editor offers only those. The same table checks a
/// procedure, so the editor and the check cannot disagree.
///
/// # Errors
///
/// Never in practice (the rules serialize).
#[wasm_bindgen(js_name = procedureRules)]
pub fn procedure_rules_js() -> std::result::Result<String, JsValue> {
    serde_json::to_string(&etendue_scene::procedure_rules())
        .map_err(|e| js_error(&Error::Input(e.to_string())))
}
