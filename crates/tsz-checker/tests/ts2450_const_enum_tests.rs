use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    get_diagnostics_with_options(source, CheckerOptions::default())
}

fn get_diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

#[test]
fn test_regular_enum_used_before_declaration_emits_ts2450() {
    let source = r"
function foo() {
    return E.A;
    enum E { A }
}
";
    let diags = get_diagnostics(source);
    assert!(
        count_code(&diags, 2450) >= 1,
        "Expected TS2450 for regular enum used before declaration, got: {diags:?}"
    );
}

#[test]
fn test_const_enum_used_before_declaration_no_ts2450() {
    let source = r"
function foo() {
    return E.A;
    const enum E { A }
}
";
    let diags = get_diagnostics(source);
    assert_eq!(
        count_code(&diags, 2450),
        0,
        "Should NOT emit TS2450 for const enum used before declaration, got: {diags:?}"
    );
}

#[test]
fn test_const_enum_top_level_forward_reference_no_ts2450() {
    let source = r"
const config = {
    a: AfterObject.A,
};
const enum AfterObject {
    A = 2,
}
";
    let diags = get_diagnostics(source);
    assert_eq!(
        count_code(&diags, 2450),
        0,
        "Should NOT emit TS2450 for const enum forward reference at top level, got: {diags:?}"
    );
}

#[test]
fn test_mixed_enum_and_const_enum_only_regular_emits_ts2450() {
    // Regular enum should emit TS2450, const enum should not
    let source = r"
function foo1() {
    return E.A;
    enum E { A }
}
function foo2() {
    return E.A;
    const enum E { A }
}
";
    let diags = get_diagnostics(source);
    let ts2450_count = count_code(&diags, 2450);
    assert_eq!(
        ts2450_count, 1,
        "Expected exactly 1 TS2450 (for regular enum only), got {ts2450_count}: {diags:?}"
    );
}

#[test]
fn test_const_enum_type_annotation_forward_reference_no_ts2450() {
    // Using const enum in type position before declaration should also be fine
    let source = r"
const v: ConstColor = ConstColor.Green;
const enum ConstColor { Red, Green, Blue }
";
    let diags = get_diagnostics(source);
    assert_eq!(
        count_code(&diags, 2450),
        0,
        "Should NOT emit TS2450 for const enum in type annotation, got: {diags:?}"
    );
}

#[test]
fn test_regular_enum_type_annotation_forward_reference_emits_ts2450() {
    // Regular enum used in value position before declaration should emit TS2450
    let source = r"
const v: Color = Color.Green;
enum Color { Red, Green, Blue }
";
    let diags = get_diagnostics(source);
    assert!(
        count_code(&diags, 2450) >= 1,
        "Expected TS2450 for regular enum used before declaration in value position, got: {diags:?}"
    );
}

#[test]
fn test_const_enum_in_nested_scope_no_ts2450() {
    // Const enum used before declaration in a nested block scope should not emit TS2450
    let source = r"
{
    const x = E.A;
    const enum E { A = 1, B = 2 }
}
";
    let diags = get_diagnostics(source);
    assert_eq!(
        count_code(&diags, 2450),
        0,
        "Should NOT emit TS2450 for const enum in nested block scope, got: {diags:?}"
    );
}

// Note: with isolatedModules: true, const enums DO get TS2450 because they
// create runtime bindings. This is tested via the conformance suite
// (blockScopedEnumVariablesUseBeforeDef_isolatedModules.ts) rather than as
// a unit test, because the unit test environment lacks global lib types
// which causes the checker to bail out before reaching TDZ checks.
