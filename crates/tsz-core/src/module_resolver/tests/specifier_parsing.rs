//! Specifier Parsing tests for `module_resolver`.
//!
//! Tests for `parse_package_specifier` — splitting bare imports into
//! package name and subpath, including scoped (`@scope/name`) packages.

use crate::module_resolver_helpers::*;

#[test]
fn test_parse_package_specifier_simple() {
    let (name, subpath) = parse_package_specifier("lodash");
    assert_eq!(name, "lodash");
    assert_eq!(subpath, None);
}

#[test]
fn test_parse_package_specifier_with_subpath() {
    let (name, subpath) = parse_package_specifier("lodash/fp");
    assert_eq!(name, "lodash");
    assert_eq!(subpath, Some("fp".to_string()));
}

#[test]
fn test_parse_package_specifier_scoped() {
    let (name, subpath) = parse_package_specifier("@babel/core");
    assert_eq!(name, "@babel/core");
    assert_eq!(subpath, None);
}

#[test]
fn test_parse_package_specifier_scoped_with_subpath() {
    let (name, subpath) = parse_package_specifier("@babel/core/transform");
    assert_eq!(name, "@babel/core");
    assert_eq!(subpath, Some("transform".to_string()));
}
