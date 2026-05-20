//! Tests for TS2725 emission
//! "Class name cannot be 'Object' when targeting ES5 and above with module X."

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source_with_file_is_esm;
use tsz_common::common::ModuleKind;

fn has_ts2725(source: &str, module: ModuleKind, file_is_esm: Option<bool>) -> bool {
    let options = CheckerOptions {
        module,
        ..CheckerOptions::default()
    };

    check_source_with_file_is_esm(source, "test.ts", options, file_is_esm)
        .iter()
        .any(|d| d.code == 2725)
}

#[test]
fn ts2725_emitted_for_commonjs_module() {
    assert!(
        has_ts2725("class Object {}", ModuleKind::CommonJS, None),
        "TS2725 should be emitted for CommonJS module"
    );
}

#[test]
fn ts2725_emitted_for_node16_cjs_file() {
    assert!(
        has_ts2725("class Object {}", ModuleKind::Node16, Some(false)),
        "TS2725 should be emitted for Node16 when file is CJS (file_is_esm=false)"
    );
}

#[test]
fn ts2725_emitted_for_nodenext_cjs_file() {
    assert!(
        has_ts2725("class Object {}", ModuleKind::NodeNext, Some(false)),
        "TS2725 should be emitted for NodeNext when file is CJS (file_is_esm=false)"
    );
}

#[test]
fn ts2725_not_emitted_for_node16_esm_file() {
    assert!(
        !has_ts2725("class Object {}", ModuleKind::Node16, Some(true)),
        "TS2725 should NOT be emitted for Node16 when file is ESM (file_is_esm=true)"
    );
}

#[test]
fn ts2725_not_emitted_for_nodenext_esm_file() {
    assert!(
        !has_ts2725("class Object {}", ModuleKind::NodeNext, Some(true)),
        "TS2725 should NOT be emitted for NodeNext when file is ESM (file_is_esm=true)"
    );
}

#[test]
fn ts2725_not_emitted_for_esnext_module() {
    assert!(
        !has_ts2725("class Object {}", ModuleKind::ESNext, None),
        "TS2725 should NOT be emitted for ESNext module"
    );
}

#[test]
fn ts2725_not_emitted_for_declare_class() {
    assert!(
        !has_ts2725("declare class Object {}", ModuleKind::CommonJS, None),
        "TS2725 should NOT be emitted for ambient (declare) class"
    );
}

#[test]
fn ts2725_not_emitted_for_non_object_class() {
    assert!(
        !has_ts2725("class Foo {}", ModuleKind::CommonJS, None),
        "TS2725 should NOT be emitted for classes not named 'Object'"
    );
}
