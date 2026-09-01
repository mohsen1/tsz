//! A tuple that spreads the result of its own recursive conditional
//! application (`[H, ...Split<R>]` where `Split<S> = S extends ... ? [H,
//! ...Split<R>] : [S]`) must display flattened, matching tsc.
//!
//! Structural rule: `tsc` always flattens a rest element whose type is a
//! concrete tuple, at construction and after instantiation. tsz already does
//! this at intern time (`splice_concrete_tuple_spreads`), but a
//! self-referential spread's rest element is still an unresolved generic
//! `Application` at that point — the nested `Split<R>` hasn't evaluated yet —
//! so the intern-time splice cannot fire (documented as its own caveat). By
//! the time the whole recursion bottoms out, the outer `Tuple` is already
//! interned with that unresolved rest element, and nothing re-visits it. The
//! printer (`TypeFormatter::format_tuple`) now re-checks the same splice rule
//! at display time, resolving a rest element's type through the same
//! `Application` display-reduction chase the formatter already uses when
//! rendering that node standalone. The underlying type is unaffected — this is
//! a display-only fix (`#16732`); assignability already treated the nested and
//! flattened forms identically.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

/// The issue's own witness: a two-segment split renders as a flat 2-tuple,
/// not `["a", ...["b"]]`.
#[test]
fn two_level_recursive_conditional_spread_displays_flattened() {
    let messages = ts2322_messages(
        r#"
type Split<S extends string> = S extends `${infer H}.${infer R}` ? [H, ...Split<R>] : [S];
type P = Split<"a.b">;
const bad: ["z"] = null as unknown as P;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains(r#"["a", "b"]"#)),
        "two-level recursive split should display as a flat tuple, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("...[")),
        "no nested-spread residue should remain in the display, got: {messages:?}"
    );
}

/// Deeper recursion nests further without the fix (`["a", ...["b", ...["c"]]]`)
/// — every level must flatten, not just the outermost one.
#[test]
fn three_level_recursive_conditional_spread_displays_flattened() {
    let messages = ts2322_messages(
        r#"
type Split<S extends string> = S extends `${infer H}.${infer R}` ? [H, ...Split<R>] : [S];
type P = Split<"a.b.c">;
const bad: ["z"] = null as unknown as P;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains(r#"["a", "b", "c"]"#)),
        "three-level recursive split should display as a flat tuple, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("...[")),
        "no nested-spread residue should remain in the display, got: {messages:?}"
    );
}

/// tsc relabels the spliced elements from whichever branch actually
/// contributed them (`head` from the recursive arm, `tail` from the base
/// case) — not a generic `rest` label carried from the recursive rest
/// element. The splice must reuse the resolved inner tuple's own element
/// metadata, not just its types.
#[test]
fn recursive_conditional_spread_preserves_resolved_element_labels() {
    let messages = ts2322_messages(
        r#"
type SplitNamed<S extends string> = S extends `${infer H}.${infer R}` ? [head: H, ...rest: SplitNamed<R>] : [tail: S];
type PN = SplitNamed<"a.b">;
const bad: ["z"] = null as unknown as PN;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"[head: "a", tail: "b"]"#)),
        "labels should come from whichever branch resolved each element, got: {messages:?}"
    );
}

/// Renamed alias/binder control: the flattening is structural, not keyed on
/// the alias or parameter names `Split`/`S`/`H`/`R`.
#[test]
fn renamed_recursive_conditional_spread_still_flattens() {
    let messages = ts2322_messages(
        r#"
type Segments<Path extends string> = Path extends `${infer First}.${infer Rest}` ? [First, ...Segments<Rest>] : [Path];
type Q = Segments<"a.b">;
const bad: ["z"] = null as unknown as Q;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains(r#"["a", "b"]"#)),
        "a renamed recursive conditional spread should still flatten, got: {messages:?}"
    );
}

/// Negative control: a hand-written nested spread (no recursion involved)
/// already flattened before this fix and must keep doing so.
#[test]
fn hand_written_nested_spread_still_flattens() {
    let messages = ts2322_messages(
        r#"
type T = ["a", ...["b", ...["c"]]];
const bad: ["z"] = null as unknown as T;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains(r#"["a", "b", "c"]"#)),
        "hand-written nested spread should stay flattened, got: {messages:?}"
    );
}

/// Negative control: a rest element typed as a plain (non-tuple) generic
/// array must not be treated as a concrete tuple and spliced away — `S[]` has
/// no fixed element count to inline, so it keeps its `...` rest display.
#[test]
fn array_typed_rest_element_is_not_spliced() {
    let messages = ts2322_messages(
        r#"
function f(x: [string, ...number[]]): ["z"] {
  return x;
}
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("[string, ...number[]]")),
        "an array-typed rest element must keep its variadic display, got: {messages:?}"
    );
}
