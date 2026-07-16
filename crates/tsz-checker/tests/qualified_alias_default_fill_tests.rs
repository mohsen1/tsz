//! Repro + adjacent matrix for M9: a namespace-qualified generic type alias with
//! defaulted type parameters, referenced BARE (`ns.Alias` where
//! `type Alias<A = D, B = A> = ...`), must substitute the declared defaults —
//! exactly as an unqualified bare reference does. Before the fix the qualified /
//! entity-name resolution path skipped the default fill that the simple-name path
//! (`resolve_simple_type_reference`) applies, so the alias body reached the
//! relation with FREE type parameters (`Body<A, B>`), producing a false
//! TS2345/TS2322 on every assignment to the bare qualified reference (runtypes
//! row). The fill is gated on EVERY parameter having a default, mirroring the
//! simple path (whose fill only runs after the `required_count > 0` arity
//! early-return), so partially-defaulted / non-defaulted qualified references
//! still surface their arity diagnostics.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "repro.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

fn count_code(diags: &[u32], expected: u32) -> usize {
    diags.iter().filter(|&&c| c == expected).count()
}

/// Core witness: bare `Kit.Lean` (class+namespace merge, alias
/// `Lean<A = any, B = A>`) must fill the defaults so the parameter type becomes
/// `{ tag: string; pair: [any, any] }`. A `Kit<number, string>` instance is then
/// assignable to it. Before the fix the target kept free `A`/`B` and rejected the
/// argument with a false TS2345.
#[test]
fn qualified_bare_defaulted_alias_fills_defaults_no_false_error() {
    let diags = codes(
        r#"
class Kit<A = any, B = A> { tag!: string; pair!: [A, B]; }
namespace Kit { export type Lean<A = any, B = A> = { tag: string; pair: [A, B] }; }
declare function take(g: Kit.Lean): void;
declare const g: Kit<number, string>;
take(g);
"#,
    );
    assert_eq!(
        count_code(&diags, 2345),
        0,
        "bare qualified defaulted alias must fill defaults (no false TS2345); got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2322),
        0,
        "no false TS2322 either; got {diags:#?}"
    );
}

/// Anti-hardcoding: rename every binder. The rule is structural, not name-driven.
#[test]
fn qualified_bare_defaulted_alias_is_binder_name_independent() {
    let diags = codes(
        r#"
class Gizmo<P = any, Q = P> { label!: string; slot!: [P, Q]; }
namespace Gizmo { export type Compact<P = any, Q = P> = { label: string; slot: [P, Q] }; }
declare function accept(w: Gizmo.Compact): void;
declare const w: Gizmo<number, string>;
accept(w);
"#,
    );
    assert_eq!(
        count_code(&diags, 2345),
        0,
        "renamed-binder form must also fill defaults; got {diags:#?}"
    );
}

/// Negative control: the top-level (unqualified) alias already filled its
/// defaults before the fix and must stay clean afterwards.
#[test]
fn top_level_bare_defaulted_alias_still_clean() {
    let diags = codes(
        r#"
type Lean<A = any, B = A> = { tag: string; pair: [A, B] };
declare function take(g: Lean): void;
declare const g: { tag: string; pair: [number, string] };
take(g);
"#,
    );
    assert_eq!(
        count_code(&diags, 2345),
        0,
        "top-level bare defaulted alias must remain clean; got {diags:#?}"
    );
}

/// Dependent default: `B = A` where `A = any`. Filling must resolve `B` through
/// `A`'s default, giving `pair: [any, any]`, so a concrete tuple is assignable.
#[test]
fn qualified_bare_alias_resolves_dependent_default() {
    let diags = codes(
        r#"
namespace Box { export type Pair<A = any, B = A> = { first: A; second: B }; }
declare function take(x: Box.Pair): void;
declare const y: { first: number; second: string };
take(y);
"#,
    );
    assert_eq!(
        count_code(&diags, 2345),
        0,
        "dependent default B = A must resolve so a concrete tuple is assignable; got {diags:#?}"
    );
}

/// Mixed arity: `Half<A, B = A>` has a REQUIRED `A` (only `B` defaults), so a
/// bare `Box.Half` reference must NOT be silently filled — it is an arity error.
/// This is the discriminating case between the `all-defaults` gate (correct) and
/// an `any-default` gate (which would drop the error). tsc reports the range form
/// TS2707 ("between 1 and 2"); the qualified path now mirrors the simple path and
/// does the same instead of the exact-count TS2314.
#[test]
fn qualified_bare_mixed_arity_alias_reports_range_arity_error() {
    let diags = codes(
        r#"
namespace Box { export type Half<A, B = A> = { a: A; b: B }; }
declare function take(x: Box.Half): void;
"#,
    );
    assert_eq!(
        count_code(&diags, 2707),
        1,
        "mixed-arity bare qualified alias must report the range arity error TS2707; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2314),
        0,
        "mixed-arity uses the range form, not the exact-count TS2314; got {diags:#?}"
    );
}

/// Non-defaulted qualified generic: unchanged TS2314 (missing-args family).
#[test]
fn qualified_bare_nondefaulted_alias_reports_ts2314() {
    let diags = codes(
        r#"
namespace Box { export type Strict<A> = { a: A }; }
declare function take(x: Box.Strict): void;
"#,
    );
    assert_eq!(
        count_code(&diags, 2314),
        1,
        "non-defaulted qualified generic must keep its TS2314; got {diags:#?}"
    );
}

/// Partially-applied qualified reference (`Box.Lean<string>`): the explicit `A`
/// plus the trailing default `B = A` resolve, so it is clean.
#[test]
fn partially_applied_qualified_alias_fills_trailing_default() {
    let diags = codes(
        r#"
namespace Box { export type Lean<A = any, B = A> = { a: A; b: B }; }
declare function take(x: Box.Lean<string>): void;
declare const y: { a: string; b: string };
take(y);
"#,
    );
    assert_eq!(
        count_code(&diags, 2345),
        0,
        "partially-applied qualified alias must fill the trailing default; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2314) + count_code(&diags, 2707),
        0,
        "partially-applied qualified alias must not report an arity error; got {diags:#?}"
    );
}

/// Deeper namespace qualification (`Outer.Inner.Lean`) bare: still fills.
#[test]
fn deep_namespace_qualified_bare_alias_fills_defaults() {
    let diags = codes(
        r#"
namespace Outer { export namespace Inner { export type Lean<A = any, B = A> = { a: A; b: B }; } }
declare function take(x: Outer.Inner.Lean): void;
declare const y: { a: number; b: string };
take(y);
"#,
    );
    assert_eq!(
        count_code(&diags, 2345),
        0,
        "deep-namespace bare qualified alias must fill defaults; got {diags:#?}"
    );
}

/// Import-alias qualification (`import K = Box; K.Lean`) bare: still fills.
#[test]
fn import_alias_qualified_bare_alias_fills_defaults() {
    let diags = codes(
        r#"
namespace Box { export type Lean<A = any, B = A> = { a: A; b: B }; }
import K = Box;
declare function take(x: K.Lean): void;
declare const y: { a: number; b: string };
take(y);
"#,
    );
    assert_eq!(
        count_code(&diags, 2345),
        0,
        "import-alias qualified bare alias must fill defaults; got {diags:#?}"
    );
}
