use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::check_source_code_messages as diagnostics;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

const SYMBOL_INDEX_MISMATCH: &str = r#"
declare const sym: symbol;
let y: { [key: symbol]: string };
const z = { [sym]: 1 };
y = z;
"#;

const EXPECTED: &str =
    "Type '{ [sym]: number; }' is not assignable to type '{ [key: symbol]: string; }'.";

const RENAMED_SYMBOL_INDEX_MISMATCH: &str = r#"
declare const token: symbol;
let destination: { [slot: symbol]: string };
const source = { [token]: 1 };
destination = source;
"#;

const RENAMED_EXPECTED: &str =
    "Type '{ [token]: number; }' is not assignable to type '{ [slot: symbol]: string; }'.";

fn diagnostics_without_test_pragmas(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(source_file);

    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|diag| (diag.code, diag.message_text))
        .collect()
}

fn assert_native_symbol_index_mismatch(diags: &[(u32, String)], expected: &str) {
    assert_eq!(
        diags,
        &[(2322, expected.to_string())],
        "expected exactly the native symbol-index incompatibility diagnostic"
    );
}

#[test]
fn symbol_index_value_mismatch_reports_natively() {
    assert_native_symbol_index_mismatch(&diagnostics(SYMBOL_INDEX_MISMATCH), EXPECTED);
}

#[test]
fn symbol_index_value_mismatch_does_not_depend_on_test_pragmas() {
    assert_native_symbol_index_mismatch(
        &diagnostics_without_test_pragmas(SYMBOL_INDEX_MISMATCH),
        EXPECTED,
    );
}

#[test]
fn symbol_index_value_mismatch_preserves_renamed_computed_key() {
    assert_native_symbol_index_mismatch(
        &diagnostics(RENAMED_SYMBOL_INDEX_MISMATCH),
        RENAMED_EXPECTED,
    );
}

#[test]
fn compatible_symbol_index_value_remains_assignable() {
    let source = r#"
declare const token: symbol;
let destination: { [slot: symbol]: number };
const value = { [token]: 1 };
destination = value;
"#;
    let diags = diagnostics(source);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
}

#[test]
fn contextual_unique_symbol_value_error_outranks_missing_property() {
    let source = r#"
declare const token: unique symbol;
const value: { [slot: symbol]: string; required: number } = { [token]: 1 };
"#;
    assert_eq!(
        diagnostics(source),
        vec![(
            2418,
            "Type of computed property's value is 'number', which is not assignable to type 'string'."
                .to_string(),
        )]
    );
}

#[test]
fn contextual_unique_symbol_value_error_keeps_sibling_property_error() {
    let source = r#"
declare const token: unique symbol;
const value: { [slot: symbol]: string; required: number } = {
    [token]: 1,
    required: "bad",
};
"#;
    assert_eq!(
        diagnostics(source),
        vec![
            (
                2418,
                "Type of computed property's value is 'number', which is not assignable to type 'string'."
                    .to_string(),
            ),
            (
                2322,
                "Type 'string' is not assignable to type 'number'.".to_string(),
            ),
        ]
    );
}
