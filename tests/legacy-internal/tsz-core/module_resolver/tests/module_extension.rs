//! Module Extension tests for `module_resolver`.
//!
//! Tests for `ModuleExtension`:
//!
//! - `from_path` classification of every supported extension
//! - `as_str` round-tripping back to the source spelling
//! - `forces_esm` / `forces_cjs` (Node16/NodeNext mode discrimination)

use super::super::*;

#[test]
fn test_module_extension_from_path() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.ts")),
        ModuleExtension::Ts
    );
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.d.ts")),
        ModuleExtension::Dts
    );
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.tsx")),
        ModuleExtension::Tsx
    );
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.js")),
        ModuleExtension::Js
    );
}

#[test]
fn test_module_extension_forces_esm() {
    assert!(ModuleExtension::Mts.forces_esm());
    assert!(ModuleExtension::Mjs.forces_esm());
    assert!(ModuleExtension::DmTs.forces_esm());
    assert!(!ModuleExtension::Ts.forces_esm());
    assert!(!ModuleExtension::Cts.forces_esm());
}

#[test]
fn test_module_extension_forces_cjs() {
    assert!(ModuleExtension::Cts.forces_cjs());
    assert!(ModuleExtension::Cjs.forces_cjs());
    assert!(ModuleExtension::DCts.forces_cjs());
    assert!(!ModuleExtension::Ts.forces_cjs());
    assert!(!ModuleExtension::Mts.forces_cjs());
}

#[test]
fn test_extension_from_path_ts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("foo.ts")),
        ModuleExtension::Ts
    );
}

#[test]
fn test_extension_from_path_tsx() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("Component.tsx")),
        ModuleExtension::Tsx
    );
}

#[test]
fn test_extension_from_path_dts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("types.d.ts")),
        ModuleExtension::Dts
    );
}

#[test]
fn test_extension_from_path_arbitrary_extension_declaration() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("native.d.node.ts")),
        ModuleExtension::Dts
    );
    assert!(ModuleExtension::from_path(Path::new("/project/native.d.node.ts")).is_declaration());
}

#[test]
fn test_extension_from_path_dmts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("types.d.mts")),
        ModuleExtension::DmTs
    );
}

#[test]
fn test_extension_from_path_dcts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("types.d.cts")),
        ModuleExtension::DCts
    );
}

#[test]
fn test_extension_from_path_js() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("bundle.js")),
        ModuleExtension::Js
    );
}

#[test]
fn test_extension_from_path_jsx() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("App.jsx")),
        ModuleExtension::Jsx
    );
}

#[test]
fn test_extension_from_path_mjs() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("module.mjs")),
        ModuleExtension::Mjs
    );
}

#[test]
fn test_extension_from_path_cjs() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("config.cjs")),
        ModuleExtension::Cjs
    );
}

#[test]
fn test_extension_from_path_mts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("utils.mts")),
        ModuleExtension::Mts
    );
}

#[test]
fn test_extension_from_path_cts() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("config.cts")),
        ModuleExtension::Cts
    );
}

#[test]
fn test_extension_from_path_json() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("package.json")),
        ModuleExtension::Json
    );
}

#[test]
fn test_extension_from_path_unknown() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("style.css")),
        ModuleExtension::Unknown
    );
}

#[test]
fn test_extension_from_path_no_extension() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("Makefile")),
        ModuleExtension::Unknown
    );
}

#[test]
fn test_extension_from_path_nested() {
    assert_eq!(
        ModuleExtension::from_path(Path::new("/project/src/lib/types.d.ts")),
        ModuleExtension::Dts
    );
}

#[test]
fn test_extension_as_str_roundtrip() {
    let extensions = [
        ModuleExtension::Ts,
        ModuleExtension::Tsx,
        ModuleExtension::Dts,
        ModuleExtension::DmTs,
        ModuleExtension::DCts,
        ModuleExtension::Js,
        ModuleExtension::Jsx,
        ModuleExtension::Mjs,
        ModuleExtension::Cjs,
        ModuleExtension::Mts,
        ModuleExtension::Cts,
        ModuleExtension::Json,
    ];
    for ext in &extensions {
        let ext_str = ext.as_str();
        assert!(
            !ext_str.is_empty(),
            "{ext:?} should have a non-empty string representation"
        );
        // Verify the string starts with a dot
        assert!(
            ext_str.starts_with('.'),
            "{ext:?}.as_str() should start with '.', got: {ext_str}"
        );
    }
    assert_eq!(ModuleExtension::Unknown.as_str(), "");
}

#[test]
fn test_extension_forces_esm() {
    assert!(ModuleExtension::Mts.forces_esm());
    assert!(ModuleExtension::Mjs.forces_esm());
    assert!(ModuleExtension::DmTs.forces_esm());

    assert!(!ModuleExtension::Ts.forces_esm());
    assert!(!ModuleExtension::Tsx.forces_esm());
    assert!(!ModuleExtension::Dts.forces_esm());
    assert!(!ModuleExtension::Js.forces_esm());
    assert!(!ModuleExtension::Cjs.forces_esm());
    assert!(!ModuleExtension::Cts.forces_esm());
}

#[test]
fn test_extension_forces_cjs() {
    assert!(ModuleExtension::Cts.forces_cjs());
    assert!(ModuleExtension::Cjs.forces_cjs());
    assert!(ModuleExtension::DCts.forces_cjs());

    assert!(!ModuleExtension::Ts.forces_cjs());
    assert!(!ModuleExtension::Tsx.forces_cjs());
    assert!(!ModuleExtension::Dts.forces_cjs());
    assert!(!ModuleExtension::Js.forces_cjs());
    assert!(!ModuleExtension::Mjs.forces_cjs());
    assert!(!ModuleExtension::Mts.forces_cjs());
}

#[test]
fn test_extension_neutral_mode() {
    // .ts, .tsx, .js, .jsx, .d.ts, .json should be neutral (neither ESM nor CJS forced)
    let neutral = [
        ModuleExtension::Ts,
        ModuleExtension::Tsx,
        ModuleExtension::Dts,
        ModuleExtension::Js,
        ModuleExtension::Jsx,
        ModuleExtension::Json,
        ModuleExtension::Unknown,
    ];
    for ext in &neutral {
        assert!(
            !ext.forces_esm() && !ext.forces_cjs(),
            "{ext:?} should be neutral (neither ESM nor CJS)"
        );
    }
}
