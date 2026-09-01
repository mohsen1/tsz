//! Package Json Data tests for `module_resolver`.
//!
//! Tests for the `PackageJson` serde wrapper plus `PackageType` and
//! `ImportingModuleKind` defaults / enum semantics.

use super::super::*;
use crate::module_resolver_helpers::*;

#[test]
fn test_package_type_enum() {
    assert_eq!(PackageType::default(), PackageType::CommonJs);
    assert_ne!(PackageType::Module, PackageType::CommonJs);
}

#[test]
fn test_importing_module_kind_enum() {
    assert_eq!(
        ImportingModuleKind::default(),
        ImportingModuleKind::CommonJs
    );
    assert_ne!(ImportingModuleKind::Esm, ImportingModuleKind::CommonJs);
}

#[test]
fn test_package_json_deserialize_basic() {
    let json = r#"{"name": "test-package", "type": "module", "main": "./index.js"}"#;

    let package_json: PackageJson = serde_json::from_str(json).unwrap();
    assert_eq!(package_json.name, Some("test-package".to_string()));
    assert_eq!(package_json.package_type, Some("module".to_string()));
    assert_eq!(package_json.main, Some("./index.js".to_string()));
}

#[test]
fn test_package_json_deserialize_exports() {
    let json = r#"{"name": "pkg", "exports": {"." : "./dist/index.js"}}"#;

    let package_json: PackageJson = serde_json::from_str(json).unwrap();
    assert!(package_json.exports.is_some());
}

#[test]
fn test_package_json_deserialize_types_versions() {
    // Build JSON programmatically to avoid raw string issues
    let json = serde_json::json!({
        "name": "typed-package",
        "typesVersions": {
            "*": {
                "*": ["./types/index.d.ts"]
            }
        }
    });

    let package_json: PackageJson = serde_json::from_value(json).unwrap();
    assert_eq!(package_json.name, Some("typed-package".to_string()));
    assert!(package_json.types_versions.is_some());
}

#[test]
fn test_package_json_deserialize_invalid_types_field_is_ignored() {
    let json = r#"{
        "name": "csv-parse",
        "main": "./lib",
        "types": ["./lib/index.d.ts", "./lib/sync.d.ts"]
    }"#;

    let package_json: PackageJson = serde_json::from_str(json).unwrap();
    assert_eq!(package_json.name, Some("csv-parse".to_string()));
    assert_eq!(package_json.main, Some("./lib".to_string()));
    assert_eq!(package_json.types, None);
}

#[test]
fn test_package_type_default_is_commonjs() {
    assert_eq!(PackageType::default(), PackageType::CommonJs);
}

#[test]
fn test_importing_module_kind_default_is_commonjs() {
    assert_eq!(
        ImportingModuleKind::default(),
        ImportingModuleKind::CommonJs
    );
}
