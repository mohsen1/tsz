use super::*;
use std::path::Path;

use crate::config::JsxEmit;

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
