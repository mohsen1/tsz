//! A *non-generic* type alias whose tuple body is built by flattening a
//! fixed-tuple spread (`type T = [...[a, b], c]`, or `type T = [...Inner, c]`
//! where `Inner` is a fixed tuple) must display as its resolved tuple form in
//! diagnostics, matching tsc.
//!
//! Structural rule: a fixed-tuple spread flattens into a fresh tuple that tsc
//! stamps with no `aliasSymbol`, so the printer renders the structural tuple
//! (`[a, b, c]`) rather than the alias name. A directly-written fixed tuple
//! (`type T = [a, b, c]`) and a genuinely variadic body (`[...number[], c]`)
//! keep their alias name. This is the non-generic analogue of the variadic
//! *application* rule in `variadic_tuple_alias_display_tests` (#10817); because
//! the flattened tuple interns to the same shape as a directly-written fixed
//! tuple, the suppression is keyed per alias def, never by body `TypeId`.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

/// An inline fixed-tuple spread (`[...[number, string], boolean]`) flattens, so
/// the alias name must not appear — the structural tuple is rendered.
#[test]
fn inline_fixed_tuple_spread_displays_structural_tuple() {
    let messages = ts2322_messages(
        r#"
type Glued = [...[number, string], boolean];
const bad: Glued = [1, "a"];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("[number, string, boolean]")),
        "spread-flattened tuple alias should display structurally, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Glued")),
        "the alias name must not leak into the message, got: {messages:?}"
    );
}

/// A spread of a *named* fixed-tuple alias (`[...Inner, boolean]`) also
/// flattens. The spread operand is only resolved during evaluation, so this
/// exercises the evaluated-shape path rather than the syntactic inline one.
#[test]
fn named_fixed_tuple_spread_displays_structural_tuple() {
    let messages = ts2322_messages(
        r#"
type Inner = [number, string];
type Outer = [...Inner, boolean];
const bad: Outer = [1, "a"];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("[number, string, boolean]")),
        "spread of a named fixed tuple should display structurally, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Outer")),
        "the alias name must not leak into the message, got: {messages:?}"
    );
}

/// A leading element followed by a fixed-tuple spread flattens too.
#[test]
fn leading_element_then_fixed_tuple_spread_displays_structural_tuple() {
    let messages = ts2322_messages(
        r#"
type Combined = [boolean, ...[number, string]];
const bad: Combined = [true];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("[boolean, number, string]")),
        "spread-flattened tuple alias should display structurally, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Combined")),
        "the alias name must not leak into the message, got: {messages:?}"
    );
}

/// A directly-written fixed tuple alias keeps its name — the suppression is
/// scoped to spread-flattened bodies only. (Binder name varied from the cases
/// above to prove the rule is structural, not name-driven.)
#[test]
fn directly_written_fixed_tuple_alias_keeps_name() {
    let messages = ts2322_messages(
        r#"
type Triple = [number, string, boolean];
const bad: Triple = [1, "a"];
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("Triple")),
        "directly-written fixed tuple alias should keep its name, got: {messages:?}"
    );
}

/// A genuinely variadic body (`[...number[], boolean]`) stays variadic — no
/// fixed-tuple spread is flattened — so the alias name is kept, matching tsc.
#[test]
fn rest_array_variadic_tuple_alias_keeps_name() {
    let messages = ts2322_messages(
        r#"
type Rested = [...number[], boolean];
const bad: Rested = ["a"];
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("Rested")),
        "rest-array variadic tuple alias should keep its name, got: {messages:?}"
    );
}

/// A variadic spread whose operand is a NAMED array alias (`[...Nums, boolean]`
/// where `Nums = number[]`) drops the alias name: tsc classifies a
/// named-operand spread as `Variadic` (only a syntactic array-type operand is
/// `Rest`), and normalizing the variadic element mints a fresh tuple without
/// the alias symbol, so the structural form is rendered
/// (`variadicTuples1.ts` line 415, TypeScript issue #40235 repro).
#[test]
fn named_array_operand_spread_alias_displays_structural_tuple() {
    let messages = ts2322_messages(
        r#"
type Nums = number[];
type Chain = [...Nums, boolean];
const bad: Chain = [false, false];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("[...number[], boolean]")),
        "a named-array-operand spread alias should display structurally, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Chain")),
        "the alias name must not leak into the message, got: {messages:?}"
    );
}

/// Per-def keying witness: a spread-flattened alias still drops its name even
/// when a directly-written fixed tuple alias of the *same* interned shape
/// coexists in the program. A body-`TypeId`-keyed flag could not do this — the
/// directly-written twin marks the shared body "directly named", which would
/// (wrongly) keep the spread alias's name too.
///
/// (The dual — keeping the directly-written twin's name at its own use site —
/// depends on disambiguating two aliases that share one interned `TypeId`,
/// which is the separate cross-shape identity limitation tracked under the
/// canonical-identity work, not this display rule. So this test asserts only
/// the spread side, which is what the rule owns.)
#[test]
fn spread_alias_drops_name_despite_same_shape_direct_alias() {
    let messages = ts2322_messages(
        r#"
type Woven = [...[number, string], boolean];
type Plain = [number, string, boolean];
const a: Woven = [1, "a"];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("[number, string, boolean]")),
        "the spread-flattened alias should display structurally, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Woven")),
        "the spread-flattened alias name must not leak even when a same-shape \
         directly-written alias coexists, got: {messages:?}"
    );
}
