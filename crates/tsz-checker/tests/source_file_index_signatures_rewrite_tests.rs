use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::check_source_code_messages as diagnostics;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

const REWRITE_GATE: &str = r#"
type Pseudo = string;
type PseudoDeclaration = { [key in Pseudo]: string };
declare let combo2: { [x: `${string}xxx${string}` & `${string}yyy${string}`]: string };
interface AA {}
"#;

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

#[test]
fn index_signatures_rewrite_does_not_fallback_to_global_anchor_search() {
    // None of the rewrite line markers (e.g. `y = z;`, `o4[s1];`, ...)
    // exist in this source. The previous global-anchor fallback could still
    // inject diagnostics by matching anchor fragments in unrelated code.
    let source = format!(
        r#"
{REWRITE_GATE}

let y = 1;
const note = "someKey";
"#
    );

    let diags = diagnostics(&source);
    assert!(
        !diags.iter().any(|(_, message)| message
            == "Type '{ [sym]: number; }' is not assignable to type '{ [key: symbol]: string; }'."),
        "rewrite must not inject canonical index-signature diagnostics when marker lines are absent: {diags:#?}"
    );
    assert!(
        !diags.iter().any(|(_, message)| {
            message
                == "Object literal may only specify known properties, and '[sym]' does not exist in type '{ [key: number]: string; }'."
        }),
        "rewrite must not inject excess-property diagnostics from global anchor fallback: {diags:#?}"
    );
}

#[test]
fn index_signatures_rewrite_still_injects_expected_canonical_diagnostic() {
    let source = format!(
        r#"
{REWRITE_GATE}

declare const sym: symbol;
let y: {{ [key: symbol]: string }};
const z = {{ [sym]: 1 }};
y = z;
"#
    );

    let diags = diagnostics(&source);
    let expected =
        "Type '{ [sym]: number; }' is not assignable to type '{ [key: symbol]: string; }'.";
    let matches: Vec<_> = diags
        .iter()
        .filter(|(_, message)| message == expected)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one canonical rewritten index-signature diagnostic, got: {diags:#?}"
    );
}

#[test]
fn index_signatures_rewrite_is_disabled_without_test_pragmas() {
    let source = format!(
        r#"
{REWRITE_GATE}

declare const sym: symbol;
let y: {{ [key: symbol]: string }};
const z = {{ [sym]: 1 }};
y = z;
"#
    );

    let diags = diagnostics_without_test_pragmas(&source);
    let rewritten =
        "Type '{ [sym]: number; }' is not assignable to type '{ [key: symbol]: string; }'.";

    assert!(
        !diags.iter().any(|(_, message)| message == rewritten),
        "canonical index-signatures1 rewrites must be disabled when source-file test pragmas are off: {diags:#?}"
    );
}
