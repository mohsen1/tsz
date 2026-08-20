//! tsz must not offer "did you mean?" suggestions in cases where tsc stays
//! silent — matching tsc's candidate-meaning and min-argument-count rules.
//!
//! - Namespace (TS2833 vs TS2503): a missing-namespace suggestion candidate must
//!   carry a namespace meaning (enum / value-module / namespace-module). A pure
//!   type (interface/class/type alias) is never offered. Owner:
//!   `error_reporter/name_resolution.rs` `error_cannot_find_namespace_with_suggestion`.
//! - Method-call index hint (TS7052 vs TS7053): the "Did you mean to call
//!   '<obj>.get'?" hint requires the get/set signature to need a leading argument
//!   (`getMinArgumentCount >= 1`). A rest-only / optional-first param does not.
//!   Owner: `error_reporter/properties/diagnostic_methods_tail.rs`
//!   `signature_accepts_index_argument`.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2503: u32 = 2503; // Cannot find namespace 'X'.
const TS2833: u32 = 2833; // Cannot find namespace 'X'. Did you mean 'Y'?
const TS7052: u32 = 7052; // ... no index signature. Did you mean to call '...'?
const TS7053: u32 = 7053; // Element implicitly has an 'any' type (no index sig).

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn type_is_not_offered_as_namespace_suggestion() {
    // `Foobaz.X` near an `interface Foobar` (a pure type) — tsc emits plain
    // TS2503, never suggesting the interface as a namespace.
    let codes = check_strict(
        r#"
interface Foobar { x: number }
type T = Foobaz.X;
"#,
    );
    assert_eq!(count(&codes, TS2503), 1, "plain TS2503 expected: {codes:?}");
    assert_eq!(
        count(&codes, TS2833),
        0,
        "an interface must not be suggested as a namespace: {codes:?}"
    );
}

#[test]
fn real_namespace_is_still_suggested() {
    // Control: a real `namespace` near-match still produces the TS2833 suggestion.
    let codes = check_strict(
        r#"
namespace Foobar { export type X = number; }
type T = Foobaz.X;
"#,
    );
    assert_eq!(
        count(&codes, TS2833),
        1,
        "a real namespace is still suggested: {codes:?}"
    );
    assert_eq!(count(&codes, TS2503), 0, "{codes:?}");
}

#[test]
fn namespace_suggestion_stable_across_repeated_references() {
    // The `Cannot find namespace` suggestion path is memoized per
    // (reference site, NAMESPACE meaning) so a missing namespace re-resolved
    // many times under demand-driven evaluation does not re-run the
    // full-symbol-universe scan (issue #14349). Referencing the same missing
    // namespace from several distinct sites must still yield one TS2833 per
    // site with the correct suggestion and no spurious TS2503 — i.e. the memo
    // never returns a stale/empty scan for a namespace lookup.
    let codes = check_strict(
        r#"
namespace Foobar { export type X = number; }
type A = Foobaz.X;
type B = Foobaz.X;
type C = Foobaz.X;
"#,
    );
    assert_eq!(
        count(&codes, TS2833),
        3,
        "each missing-namespace site still gets its suggestion: {codes:?}"
    );
    assert_eq!(count(&codes, TS2503), 0, "{codes:?}");
}

#[test]
fn value_only_qualified_name_anchor_still_gets_namespace_suggestion() {
    // `m` resolves as a value, not a namespace, but tsc still offers the nearby
    // namespace `M` for the qualified type-name anchor.
    let codes = check_strict(
        r#"
namespace M { export interface Point { x: number; y: number } }
var m = M;
var p: m.Point;
"#,
    );
    assert_eq!(
        count(&codes, TS2833),
        1,
        "wrong-meaning namespace anchors still get suggestions: {codes:?}"
    );
    assert_eq!(count(&codes, TS2503), 0, "{codes:?}");
}

#[test]
fn ts7_namespace_suggestions_continue_after_ten_failures() {
    // TypeScript 7 no longer caps namespace "did you mean?" diagnostics at ten
    // sites. Every distinct near-match remains TS2833.
    let codes = check_strict(
        r#"
namespace Foobar { export type X = number; }
type T0 = Foobaz.X;
type T1 = Foobaz.X;
type T2 = Foobaz.X;
type T3 = Foobaz.X;
type T4 = Foobaz.X;
type T5 = Foobaz.X;
type T6 = Foobaz.X;
type T7 = Foobaz.X;
type T8 = Foobaz.X;
type T9 = Foobaz.X;
type T10 = Foobaz.X;
"#,
    );
    assert_eq!(
        count(&codes, TS2833),
        11,
        "every namespace site should get its suggestion: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2503),
        0,
        "no namespace site should fall back to plain TS2503: {codes:?}"
    );
}

#[test]
fn rest_only_indexer_method_does_not_get_call_hint() {
    // `o[sym]` where `get` takes only a rest param (min-arg-count 0) — tsc emits
    // plain TS7053, not the "Did you mean to call 'o.get'?" TS7052 hint.
    let codes = check_strict(
        r#"
declare const sym: unique symbol;
const o = { get(..._: any): any { return 1; } };
o[sym];
"#,
    );
    assert_eq!(count(&codes, TS7053), 1, "plain TS7053 expected: {codes:?}");
    assert_eq!(
        count(&codes, TS7052),
        0,
        "rest-only get must not trigger the call hint: {codes:?}"
    );
}

#[test]
fn required_arg_indexer_method_still_gets_call_hint() {
    // Control: a `get` with a required leading param still produces TS7052.
    let codes = check_strict(
        r#"
declare const sym: unique symbol;
const o = { get(k: symbol): any { return k; } };
o[sym];
"#,
    );
    assert_eq!(
        count(&codes, TS7052),
        1,
        "required-arg get still gets the call hint: {codes:?}"
    );
    assert_eq!(count(&codes, TS7053), 0, "{codes:?}");
}
