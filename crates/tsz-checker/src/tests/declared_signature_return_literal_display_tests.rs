//! Tests that a **declared** (non-fresh) signature source preserves its literal
//! return and parameter types verbatim in TS2322 assignability diagnostics,
//! while a **fresh** function expression's inferred return literal still widens.
//!
//! Structural rule: `tsc`'s `getWidenedType` widens only *fresh* literals, so a
//! literal written in a declared signature position — `{ m(): 1 }`,
//! `(x: 2) => void`, `() => 1` — is rendered verbatim, whereas a fresh function
//! expression's inferred return (`(x) => 1`) is widened to its base
//! (`(x) => number`). tsz lost per-literal freshness for inferred arrow returns,
//! so the discriminator is the *source provenance*: only the declared-identifier
//! source path preserves the signature literal; fresh function-expression
//! sources keep widening.
//!
//! Before the fix, the assignability display normalizer widened *every*
//! function/method/constructor return literal, so a declared `{ m(): 1 }` source
//! rendered as `{ m(): number; }`. The traversal that recognizes a non-fresh
//! source's canonical literal members also did not descend into signature
//! parameter/return positions, so a declared `{ m(x: 1): void }` param widened
//! too.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

fn assert_source_displays(source: &str, expected_source_type: &str) {
    let messages = ts2322_messages(source);
    assert!(
        messages.iter().any(|m| m.contains(expected_source_type)),
        "expected source display `{expected_source_type}`, got: {messages:?}"
    );
}

#[test]
fn declared_method_return_literal_is_preserved() {
    assert_source_displays(
        r#"
declare const v: { m(): 1 };
const s: string = v;
"#,
        "{ m(): 1; }",
    );
}

#[test]
fn declared_method_string_return_literal_is_preserved() {
    assert_source_displays(
        r#"
declare const v: { m(): "x" };
const s: string = v;
"#,
        "{ m(): \"x\"; }",
    );
}

#[test]
fn declared_method_parameter_literal_is_preserved() {
    // The non-fresh-literal-member traversal must descend into signature
    // parameter positions, not just return positions.
    assert_source_displays(
        r#"
declare const v: { m(x: 1): void };
const s: string = v;
"#,
        "{ m(x: 1): void; }",
    );
}

#[test]
fn declared_method_param_and_return_literals_are_preserved() {
    assert_source_displays(
        r#"
declare const v: { m(x: 1): 2 };
const s: string = v;
"#,
        "{ m(x: 1): 2; }",
    );
}

#[test]
fn nested_declared_method_return_literal_is_preserved() {
    assert_source_displays(
        r#"
declare const v: { o: { m(): 2 } };
const s: string = v;
"#,
        "{ o: { m(): 2; }; }",
    );
}

#[test]
fn declared_top_level_function_return_literal_is_preserved() {
    assert_source_displays(
        r#"
declare const v: () => 1;
const s: string = v;
"#,
        "() => 1",
    );
}

#[test]
fn declared_top_level_function_param_literal_is_preserved() {
    // A literal type nested in a top-level FUNCTION_TYPE's parameter list
    // (not just its return position) must also decline the canonical
    // structural formatter's widening.
    assert_source_displays(
        r#"
declare const v: (x: 1) => void;
const s: string = v;
"#,
        "(x: 1) => void",
    );
}

#[test]
fn declared_top_level_constructor_return_literal_is_preserved() {
    assert_source_displays(
        r#"
declare const v: new () => 1;
const s: string = v;
"#,
        "new () => 1",
    );
}

#[test]
fn declared_top_level_function_negative_numeric_literal_is_preserved() {
    assert_source_displays(
        r#"
declare const v: () => -1;
const s: string = v;
"#,
        "() => -1",
    );
}

#[test]
fn declared_top_level_function_bigint_literal_is_preserved() {
    assert_source_displays(
        r#"
declare const v: () => 1n;
const s: string = v;
"#,
        "() => 1n",
    );
}

#[test]
fn declared_top_level_function_boolean_literal_is_preserved() {
    assert_source_displays(
        r#"
declare const v: () => true;
const s: string = v;
"#,
        "() => true",
    );
}

#[test]
fn declared_top_level_tuple_literal_element_is_preserved() {
    assert_source_displays(
        r#"
declare const v: [1, string];
const s: string = v;
"#,
        "[1, string]",
    );
}

#[test]
fn badly_spaced_top_level_function_return_literal_is_normalized() {
    // Regression (#17128): the raw-source-text fallback that preserves a
    // declared literal must not also leak the user's exact (possibly
    // non-canonical) spacing into the diagnostic.
    assert_source_displays(
        r#"
declare const v: ()=>1;
const s: string = v;
"#,
        "() => 1",
    );
}

#[test]
fn badly_spaced_top_level_function_param_literal_is_normalized() {
    assert_source_displays(
        r#"
declare const v: (x:1)=>void;
const s: string = v;
"#,
        "(x: 1) => void",
    );
}

#[test]
fn badly_spaced_top_level_constructor_return_literal_is_normalized() {
    // The construct-signature sibling shares the same raw-text fallback and
    // the same asymmetry, just untested with non-canonical spacing before.
    assert_source_displays(
        r#"
declare const v: new()=>1;
const s: string = v;
"#,
        "new () => 1",
    );
}

#[test]
fn badly_spaced_nested_function_type_param_with_literal_is_normalized() {
    // A higher-order parameter type that is itself a literal-bearing
    // FUNCTION_TYPE must also normalize, not just the outer signature.
    assert_source_displays(
        r#"
declare const v: (f:(x:1)=>void)=>void;
const s: string = v;
"#,
        "(f: (x: 1) => void) => void",
    );
}

#[test]
fn badly_spaced_rest_param_with_literal_return_is_normalized() {
    assert_source_displays(
        r#"
declare const v: (...args:number[])=>1;
const s: string = v;
"#,
        "(...args: number[]) => 1",
    );
}

#[test]
fn declared_top_level_function_no_literal_still_canonicalizes_spacing() {
    // Control: a signature with no literal member still routes through the
    // canonical structural formatter and gets tsc's spacing, unaffected by
    // this fix.
    assert_source_displays(
        r#"
declare const v: (x:number,y:string)=>void;
const s: string = v;
"#,
        "(x: number, y: string) => void",
    );
}

#[test]
fn declared_top_level_function_badly_spaced_return_literal_canonicalizes_spacing() {
    // #17128: a written signature literal must stay verbatim (`1`, not
    // `number`) *and* have its author whitespace canonicalized (`()=>1` ->
    // `() => 1`). #17124 fixed the literal by echoing the raw source text,
    // which re-leaked the spacing on badly-spaced input; the canonical
    // formatter under `PreserveSignatureReturnLiteralsScope` reconciles both.
    assert_source_displays(
        r#"
declare const v: ()=>1;
const s: string = v;
"#,
        "() => 1",
    );
}

#[test]
fn declared_top_level_function_badly_spaced_param_literal_canonicalizes_spacing() {
    // The `(x:1)=>void` row that regressed silently under #17124 (no test
    // covered a no-space param-position literal).
    assert_source_displays(
        r#"
declare const v: (x:1)=>void;
const s: string = v;
"#,
        "(x: 1) => void",
    );
}

#[test]
fn declared_top_level_constructor_badly_spaced_return_literal_canonicalizes_spacing() {
    // The construct-signature (`TypeData::Callable`) form of the same rule:
    // `new()=>1` canonicalizes to `new () => 1` while keeping the return
    // literal.
    assert_source_displays(
        r#"
declare const v: new()=>1;
const s: string = v;
"#,
        "new () => 1",
    );
}

#[test]
fn declared_top_level_function_badly_spaced_no_literal_canonicalizes_spacing() {
    // Control: the no-literal signature still canonicalizes its spacing under
    // the inline-signature source path (it never widens a literal, so the
    // preserve scope is a no-op for it).
    assert_source_displays(
        r#"
declare const v: (x:number)=>void;
const s: string = v;
"#,
        "(x: number) => void",
    );
}

#[test]
fn declared_method_literal_preserved_when_source_is_in_target_role_position() {
    // The mismatching source here is `x` (`{ m(): 2 }`); it must keep `2`.
    assert_source_displays(
        r#"
declare const x: { m(): 2 };
const v: { m(): 1 } = x;
"#,
        "{ m(): 2; }",
    );
}

#[test]
fn renamed_binders_preserve_declared_method_return_literal() {
    // Anti-hardcoding: the rule is structural, not keyed on identifier names.
    assert_source_displays(
        r#"
declare const widget: { compute(): 7 };
const out: string = widget;
"#,
        "{ compute(): 7; }",
    );
}

#[test]
fn fresh_arrow_expression_return_literal_still_widens() {
    // A fresh function expression's inferred return literal is widened to its
    // base, matching tsc (`(x) => 1` → `(x: number) => number`).
    let messages = ts2322_messages(
        r#"
interface T { (x: number): string }
declare let t: T;
t = (x: number) => 1;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("(x: number) => number")),
        "fresh arrow return should widen to `number`, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("(x: number) => 1")),
        "fresh arrow return must not be preserved as a literal, got: {messages:?}"
    );
}

#[test]
fn fresh_function_expression_return_literal_still_widens() {
    let messages = ts2322_messages(
        r#"
interface T { (x: number): string }
declare let t: T;
t = function (x: number) { return 1; };
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("(x: number) => number")),
        "fresh function-expression return should widen, got: {messages:?}"
    );
}

#[test]
fn fresh_object_literal_method_return_still_widens() {
    // A fresh object literal's inferred method return is widened at inference,
    // so it must keep displaying the widened base.
    let messages = ts2322_messages(
        r#"
const v = { m() { return 1; } };
const s: string = v;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("{ m(): number; }")),
        "fresh object-literal method return should widen, got: {messages:?}"
    );
}

#[test]
fn declared_plain_literal_property_still_preserved_control() {
    // Pre-existing behavior: a declared non-method literal property is kept.
    assert_source_displays(
        r#"
declare const v: { a: 1 };
const s: string = v;
"#,
        "{ a: 1; }",
    );
}

#[test]
fn generic_template_literal_signature_source_with_coincidental_alias_expands() {
    // #17119 (templateLiteralTypes7.ts shape): `v`'s inline generic signature
    // annotation has no `aliasSymbol`, even though it is structurally
    // identical to `G1`. tsc renders the expanded signature for the source;
    // it must never substitute the coincidentally-shaped alias name.
    let messages = ts2322_messages(
        r#"
type NMap = { 1: string; 2: number; 3: boolean };
type G1 = <T extends 1 | 2 | 3>(x: `${T}`) => NMap[T];
type G2 = (x: string) => void;
declare const v: <T extends 1 | 2 | 3>(x: `${T}`) => NMap[T];
const bad: G2 = v;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("<T extends 1 | 2 | 3>(x: `${T}`) => NMap[T]")),
        "generic template-literal signature source must expand, not show the coincidental \
         alias `G1`, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Type 'G1'")),
        "must not substitute the coincidental alias `G1`, got: {messages:?}"
    );
}

#[test]
fn declared_unit_literal_source_still_widens_control() {
    // A declared *scalar* unit-literal source widens to its base against a
    // non-literal target (unchanged by this fix; literals have no signature).
    let messages = ts2322_messages(
        r#"
declare const v: 1;
const s: string = v;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("Type 'number'")),
        "declared unit literal should widen to base, got: {messages:?}"
    );
}
