//! Tests for TS2322 source-display preservation against `undefined` / `null` targets.
//!
//! tsc preserves the source's literal surface in TS2322 diagnostics whose target
//! is `undefined` or `null` — the user wrote a concrete value (`1`, `""`, `true`)
//! and the diagnostic should echo that value back rather than its widened
//! primitive base. tsz mirrors this for boolean keywords, string literals,
//! template literals, and signed numeric / bigint literals.
//!
//! The same preservation extends to a *nullable union type-alias application*
//! (`type Maybe<T> = T | undefined`): its alias symbol is restored over the
//! relation's reduced target (tsc `reportErrorResults`), so the reported target
//! is the whole singleton-capable union and the literal source survives — while a
//! structurally identical *inline* `string | undefined` (no alias) reports against
//! its reduced non-nullish member and widens. Both directions are pinned below.
//!
//! Conformance test: `invalidUndefinedValues.ts`.

fn compile_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn ts2322(diags: &[(u32, String)]) -> Vec<&str> {
    diags
        .iter()
        .filter_map(|(code, msg)| (*code == 2322).then_some(msg.as_str()))
        .collect()
}

#[test]
fn ts2322_preserves_number_literal_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
var x: typeof undefined;
x = 1;
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '1'") && m.contains("'undefined'")),
        "expected literal '1' preserved against 'undefined', got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_string_literal_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
var x: typeof undefined;
x = '';
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '\"\"'") && m.contains("'undefined'")),
        "expected literal '\"\"' preserved against 'undefined', got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_true_against_undefined_target() {
    let diags = compile_diagnostics(
        r#"
var x: typeof undefined;
x = true;
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type 'true'") && m.contains("'undefined'")),
        "expected preserved 'true' against 'undefined', got: {msgs:?}"
    );
}

// A generic type-alias application whose body is a nullable union
// (`type Maybe<T> = T | undefined`) carries an alias symbol, which tsc's
// `reportErrorResults` restores over the relation's reduced target — so the
// reported target is the *whole* `Maybe<string>` union, which is singleton-
// capable through its `undefined` member, and the literal source `5` is
// preserved rather than generalized to `number`. This is the #15368 f-source
// residual.
#[test]
fn ts2322_preserves_number_literal_against_nullable_union_alias() {
    let diags = compile_diagnostics(
        r#"
type Maybe<T> = T | undefined;
const m: Maybe<string> = 5;
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '5'") && m.contains("Maybe<string>")),
        "expected literal '5' preserved against nullable union alias 'Maybe<string>', got: {msgs:?}"
    );
}

// Same rule with a `| null` member and an object argument (the issue's own
// witness shape), and a renamed binder to prove the decision is structural.
#[test]
fn ts2322_preserves_number_literal_against_null_union_alias_renamed() {
    let diags = compile_diagnostics(
        r#"
type OrNull<Val> = Val | null;
const c: OrNull<{ u: string }> = 5;
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '5'") && m.contains("OrNull<{ u: string; }>")),
        "expected literal '5' preserved against null union alias 'OrNull<...>', got: {msgs:?}"
    );
}

// Negative control #1: a structurally identical *inline* nullable union carries
// no alias symbol, so tsc reports against the reduced non-nullish member and
// generalizes the literal to its base. The fix must not preserve here.
#[test]
fn ts2322_widens_number_literal_against_inline_nullable_union() {
    let diags = compile_diagnostics(
        r#"
const x: string | undefined = 5;
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("Type 'number'"))
            && !msgs.iter().any(|m| m.contains("Type '5'")),
        "expected literal '5' widened to 'number' against inline 'string | undefined', got: {msgs:?}"
    );
}

// Negative control #2: a plain, non-singleton-capable target still generalizes
// the literal source to its primitive base, exactly as tsc does.
#[test]
fn ts2322_widens_number_literal_against_plain_string_target() {
    let diags = compile_diagnostics(
        r#"
var x: string;
x = 5;
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type 'number'") && m.contains("'string'")),
        "expected literal '5' widened to 'number' against plain 'string', got: {msgs:?}"
    );
}

#[test]
fn ts2322_preserves_string_literal_against_string_literal_target() {
    let diags = compile_diagnostics(
        r#"
let x: "a" = "b";
"#,
    );
    let msgs = ts2322(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Type '\"b\"'") && m.contains("'\"a\"'")),
        "expected literal '\"b\"' kept against literal '\"a\"', got: {msgs:?}"
    );
}
