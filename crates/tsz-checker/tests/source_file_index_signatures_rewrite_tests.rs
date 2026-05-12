use tsz_checker::test_utils::check_source_code_messages as diagnostics;

const REWRITE_GATE: &str = r#"
type Pseudo = string;
type PseudoDeclaration = { [key in Pseudo]: string };
declare let combo2: { [x: `${string}xxx${string}` & `${string}yyy${string}`]: string };
interface AA {}
"#;

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
