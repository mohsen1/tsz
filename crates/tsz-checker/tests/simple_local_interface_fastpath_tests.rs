use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_checker::context::{CheckerOptions, LibContext};
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_code_messages, check_source_with_libs_code_messages,
    load_compiled_lib_files,
};
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn primitive_type_reference_properties_keep_intrinsic_types() {
    let diagnostics = check_source_code_messages(
        r#"
interface I {
    n: number;
    s: string;
    b: boolean;
    tag: "ok";
}

const ok: I = { n: 1, s: "x", b: true, tag: "ok" };
const badNumber: I = { n: "x", s: "x", b: true, tag: "ok" };
const badTag: I = { n: 1, s: "x", b: true, tag: "no" };
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        2,
        "expected TS2322s for primitive and literal property mismatches, got {diagnostics:?}",
    );
    assert!(
        ts2322.iter().any(
            |(_, message)| message.contains("Type 'string' is not assignable to type 'number'")
        ),
        "expected primitive number target in TS2322, got {ts2322:?}",
    );
    assert!(
        ts2322.iter().any(
            |(_, message)| message.contains("Type '\"no\"' is not assignable to type '\"ok\"'")
        ),
        "expected string-literal target in TS2322, got {ts2322:?}",
    );
}

#[test]
fn composite_and_array_properties_keep_lowered_types() {
    let diagnostics = check_source_code_messages(
        r#"
interface I {
    choice: number | "x";
    list: number[];
    pair: [number, "ok"];
}

const ok: I = { choice: "x", list: [1], pair: [1, "ok"] };
const badChoice: I = { choice: true, list: [1], pair: [1, "ok"] };
const badList: I = { choice: 1, list: ["x"], pair: [1, "ok"] };
const badPair: I = { choice: 1, list: [1], pair: ["x", "ok"] };
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        3,
        "expected TS2322s for union, array, and tuple property mismatches, got {diagnostics:?}",
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, message)| message.contains("Type 'true' is not assignable to type")),
        "expected union target in TS2322, got {ts2322:?}",
    );
    assert!(
        ts2322
            .iter()
            .filter(
                |(_, message)| message.contains("Type 'string' is not assignable to type 'number'")
            )
            .count()
            >= 2,
        "expected array and tuple number targets in TS2322, got {ts2322:?}",
    );
}

#[test]
fn missing_interface_lib_rows_keep_lib_shapes() {
    let lib_files = load_compiled_lib_files(&[
        "lib.es5.d.ts",
        "lib.es2015.iterable.d.ts",
        "lib.es2020.symbol.wellknown.d.ts",
        "lib.es2022.regexp.d.ts",
    ]);
    assert!(
        !lib_files.is_empty(),
        "compiled lib fixtures should be available"
    );

    let diagnostics = check_source_with_libs_code_messages(
        r#"
const descriptor: PropertyDescriptor = { configurable: "yes" };
const descriptorMap: PropertyDescriptorMap = { field: { enumerable: "yes" } };
declare const indices: RegExpIndicesArray;
const firstIndex: [number, number] = indices[0];
"#,
        "test.ts",
        CheckerOptions::default(),
        &lib_files,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| *code == 2322
            && message.contains("Type 'string' is not assignable to type 'boolean'")),
        "expected PropertyDescriptor boolean property mismatch, got {diagnostics:?}",
    );
    assert!(
        diagnostics.iter().any(|(code, message)| *code == 2322
            && message.contains("Type 'unknown' is not assignable to type '[number, number]'")),
        "expected RegExpIndicesArray indexed value mismatch, got {diagnostics:?}",
    );
}

fn assert_imported_user_interface_keeps_user_shape(interface_name: &str) {
    let lib_files = load_compiled_lib_files(&["lib.es5.d.ts"]);
    assert!(
        !lib_files.is_empty(),
        "compiled lib fixtures should be available"
    );

    let defs = format!("\nexport interface {interface_name} {{\n    custom: string;\n}}\n");
    let main = format!(
        "\nimport type {{ {interface_name} }} from \"./defs\";\n\nconst bad: {interface_name} = {{}};\n"
    );

    let diagnostics = check_multi_file_with_libs(
        &[("defs.ts", defs.as_str()), ("main.ts", main.as_str())],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
        &lib_files,
    );

    let messages: Vec<_> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.message_text.as_str()))
        .collect();
    assert!(
        messages
            .iter()
            .any(|(code, message)| *code == 2741 && message.contains("custom")),
        "imported user interface named {interface_name} must keep its own shape, got {messages:?}",
    );
}

#[test]
fn missing_interface_lib_row_names_do_not_capture_imported_user_interfaces() {
    assert_imported_user_interface_keeps_user_shape("PropertyDescriptor");
}

#[test]
fn imported_user_interface_lib_name_is_resolved_by_provenance_not_spelling() {
    // The residue suppress rule is keyed on binder-recorded lib provenance, so an
    // adjacent lib name (`RegExpExecArray`) must behave identically to the original
    // `PropertyDescriptor` case.
    assert_imported_user_interface_keeps_user_shape("RegExpExecArray");
}

/// Direct unit coverage of the suppress predicate at the changed boundary.
///
/// The PR removes the hardcoded `PropertyDescriptor | PropertyDescriptorMap |
/// RegExpIndicesArray` name allowlist from the `should_suppress_missing_interface_decl_reject`
/// computation. The new rule reads only binder-recorded lib provenance. This
/// test exercises the predicate (`simple_object_missing_interface_decl_residue_is_lib_provenance_case`)
/// across three witnesses chosen to cross the old allowlist boundary:
///   * `PropertyDescriptor` — inside the old allowlist; must remain a lib case.
///   * `RegExpExecArray`    — outside the old allowlist; must also be a lib case.
///   * `UserInterface`      — user-defined symbol; must never be a lib case.
///
/// On `origin/main` the inline predicate additionally `matches!`'d against the
/// allowlist, so a non-allowlisted cloned-lib name (`RegExpExecArray`) returned
/// `false`. The test would therefore fail on `origin/main` at the second
/// assertion below.
#[test]
fn suppress_predicate_classifies_lib_symbols_by_provenance_not_name() {
    let lib_files = load_compiled_lib_files(&["lib.es5.d.ts"]);
    assert!(
        !lib_files.is_empty(),
        "compiled lib fixtures should be available"
    );

    let source = "\ninterface UserInterface { custom: string; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let binder_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| tsz_binder::state::LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&binder_lib_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let user_sym_id = binder
        .file_locals
        .get("UserInterface")
        .expect("user interface should be bound");

    let find_cloned_lib_symbol = |name: &str| {
        binder
            .file_locals
            .get(name)
            .filter(|sym_id| binder.lib_symbol_ids.contains(sym_id))
            .unwrap_or_else(|| panic!("lib symbol {name} should be merged as cloned-lib"))
    };
    let lib_descriptor_sym = find_cloned_lib_symbol("PropertyDescriptor");
    let lib_exec_array_sym = find_cloned_lib_symbol("RegExpExecArray");

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(checker_lib_contexts);

    assert!(
        checker
            .ctx
            .simple_object_missing_interface_decl_residue_is_lib_provenance_case(
                lib_descriptor_sym,
                false,
            ),
        "PropertyDescriptor is a cloned-lib symbol and must be a lib-provenance case",
    );
    assert!(
        checker
            .ctx
            .simple_object_missing_interface_decl_residue_is_lib_provenance_case(
                lib_exec_array_sym,
                false,
            ),
        "RegExpExecArray (outside the old allowlist) must also be a lib-provenance case — \
         this assertion is what would fail on origin/main",
    );
    assert!(
        !checker
            .ctx
            .simple_object_missing_interface_decl_residue_is_lib_provenance_case(
                user_sym_id,
                false,
            ),
        "user-defined UserInterface must never be classified as a lib-provenance case",
    );
    assert!(
        !checker
            .ctx
            .simple_object_missing_interface_decl_residue_is_lib_provenance_case(
                lib_descriptor_sym,
                true,
            ),
        "lib symbol with a local interface decl must fall through to the merge path, \
         not the lib-only residue case",
    );
}
