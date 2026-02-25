use super::*;
use std::path::Path;

use crate::config::JsxEmit;
use tempfile::tempdir;

// ─── js_extension_for tests ──────────────────────────────────────────────────

#[test]
fn test_js_extension_for_ts_input() {
    assert_eq!(js_extension_for(Path::new("file.ts"), None), Some("js"));
}

#[test]
fn test_js_extension_for_tsx_react() {
    assert_eq!(
        js_extension_for(Path::new("file.tsx"), Some(JsxEmit::React)),
        Some("js")
    );
}

#[test]
fn test_js_extension_for_tsx_preserve() {
    assert_eq!(
        js_extension_for(Path::new("file.tsx"), Some(JsxEmit::Preserve)),
        Some("jsx")
    );
}

#[test]
fn test_js_extension_for_mts_input() {
    assert_eq!(js_extension_for(Path::new("file.mts"), None), Some("mjs"));
}

#[test]
fn test_js_extension_for_cts_input() {
    assert_eq!(js_extension_for(Path::new("file.cts"), None), Some("cjs"));
}

#[test]
fn test_js_extension_for_js_input() {
    // JS input files should produce JS output (same extension).
    // This matches tsc behavior where allowJs files are emitted.
    assert_eq!(js_extension_for(Path::new("file.js"), None), Some("js"));
}

#[test]
fn test_js_extension_for_jsx_input() {
    assert_eq!(js_extension_for(Path::new("file.jsx"), None), Some("jsx"));
}

#[test]
fn test_js_extension_for_mjs_input() {
    assert_eq!(js_extension_for(Path::new("file.mjs"), None), Some("mjs"));
}

#[test]
fn test_js_extension_for_cjs_input() {
    assert_eq!(js_extension_for(Path::new("file.cjs"), None), Some("cjs"));
}

#[test]
fn test_js_extension_for_unknown_ext() {
    assert_eq!(js_extension_for(Path::new("file.txt"), None), None);
    assert_eq!(js_extension_for(Path::new("file.rs"), None), None);
}

#[test]
fn test_normalize_type_roots_keeps_existing_absolute_root() {
    let temp = tempdir().unwrap();
    let types_dir = temp.path().join("types");
    std::fs::create_dir_all(&types_dir).unwrap();
    let absolute = canonicalize_or_owned(&types_dir);

    let normalized = normalize_type_roots(temp.path(), Some(vec![absolute.clone()])).unwrap();

    assert_eq!(normalized, vec![absolute]);
}

#[test]
fn test_normalize_type_roots_skips_missing_absolute_root() {
    let temp = tempdir().unwrap();
    let base_dir = canonicalize_or_owned(temp.path());
    // Even though <base_dir>/types/ exists, an absolute "/types" that doesn't
    // exist on disk should NOT fall back to the base_dir-relative path.
    // tsc treats absolute typeRoots as-is; if they don't exist, they're skipped.
    let _types_dir = base_dir.join("types");
    std::fs::create_dir_all(&_types_dir).unwrap();

    let normalized =
        normalize_type_roots(&base_dir, Some(vec![Path::new("/types").to_path_buf()])).unwrap();

    assert!(
        normalized.is_empty(),
        "absolute /types should be skipped when it doesn't exist on disk"
    );
}
