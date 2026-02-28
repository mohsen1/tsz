//! Tests for TS2725 emission
//! "Class name cannot be 'Object' when targeting ES5 and above with module X."

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn has_ts2725(source: &str, module: ModuleKind, file_is_esm: Option<bool>) -> bool {
    let options = CheckerOptions {
        module,
        ..CheckerOptions::default()
    };

    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.file_is_esm = file_is_esm;
    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().any(|d| d.code == 2725)
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
