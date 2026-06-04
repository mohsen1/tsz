//! Regression coverage for object-intersection property merging and the
//! synthesized write type.
//!
//! When two object members of an intersection share a property, the merged
//! property's *read* type is the (raw, unreduced) intersection of the member
//! types, e.g. `{ a: { b; c } } & { a: { b } }` yields `a: { b; c } & { b }`.
//! The merged *write* type must stay equal to that read type for writable
//! properties; otherwise the property is mistaken for a split accessor and a
//! spurious contravariant write check rejects perfectly valid assignments such
//! as assigning the intersection to `{ a: { b } }`.
//!
//! See <https://github.com/tsz-org/tsz/issues/11323>.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_strict, diagnostic_count};

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_strict(source)
}

fn ts2322_count(diagnostics: &[Diagnostic]) -> usize {
    diagnostic_count(diagnostics, 2322)
}

#[test]
fn required_shared_property_intersection_assignable_to_base_shape() {
    // `{ a: { b; c } } & { a: { b } }` merges `a` to `{ b; c } & { b }`.
    // That intersection has `b`, so it is assignable to `{ a: { b } }`.
    let source = r#"
type Merged = { a: { b: string; c: number } } & { a: { b: string } };
declare let merged: Merged;
const narrow: { a: { b: string } } = merged;
const wide: { a: { b: string; c: number } } = merged;
"#;
    let diags = diagnostics(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "merged shared property must be assignable to either conjunct shape, got: {diags:?}"
    );
}

#[test]
fn optional_shared_property_intersection_assignable() {
    // The issue's focus: optional shared properties in utility-style merges.
    let source = r#"
type Merged = { a?: { b: string; c: number } } & { a?: { b: string } };
declare let merged: Merged;
const narrow: { a?: { b: string } } = merged;
"#;
    let diags = diagnostics(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "optional merged shared property must remain assignable, got: {diags:?}"
    );
}

#[test]
fn inline_intersection_matches_aliased_intersection() {
    // Inline and aliased intersections must behave identically (structural
    // identity); both used to incorrectly reject assignment to `{ a: { b } }`.
    let source = r#"
declare let merged: { a: { b: string; c: number } } & { a: { b: string } };
const narrow: { a: { b: string } } = merged;
"#;
    let diags = diagnostics(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "inline object intersection must be assignable to its narrower conjunct, got: {diags:?}"
    );
}

#[test]
fn deep_partial_chain_assignable_to_manual_optional_shape() {
    // DeepPartial-style recursive mapped composition produces optional
    // properties that compose through intersections without spurious errors.
    let source = r#"
type DeepPartial<T> = { [P in keyof T]?: DeepPartial<T[P]> };
interface Config { nested: { value: string; extra: number } }
declare let dp: DeepPartial<Config> & { nested?: { value?: string } };
const manual: { nested?: { value?: string } } = dp;
"#;
    let diags = diagnostics(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "DeepPartial intersection chain must stay assignable, got: {diags:?}"
    );
}

#[test]
fn type_predicate_over_optional_object_property() {
    // The literal reproduction from the issue: a `y is A` predicate where the
    // predicate type only differs by optional vs `| undefined` modelling.
    let source = r#"
type A = { a?: { b: string } };
type B = { a?: { b: string } | undefined };
function f(x: A, y: B): y is A { return x.a?.b === y.a?.b; }
"#;
    let diags = diagnostics(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "optional vs union-with-undefined predicate must not report assignability errors, got: {diags:?}"
    );
}

#[test]
fn binder_name_invariance() {
    // Anti-hardcoding: behavior must not depend on the chosen identifiers.
    let source = r#"
type Combined = { field: { keep: string; drop: number } } & { field: { keep: string } };
declare let value: Combined;
const projected: { field: { keep: string } } = value;
"#;
    let diags = diagnostics(source);
    assert_eq!(
        ts2322_count(&diags),
        0,
        "renamed binders must behave identically, got: {diags:?}"
    );
}

#[test]
fn genuinely_conflicting_intersection_property_still_rejected() {
    // Negative case: the merged property `a: { b } & { c }` has neither `d`,
    // so assignment to `{ a: { d } }` must still fail with TS2322.
    let source = r#"
type Bad = { a: { b: string } } & { a: { c: number } };
declare let bad: Bad;
const target: { a: { d: boolean } } = bad;
"#;
    let diags = diagnostics(source);
    assert!(
        ts2322_count(&diags) >= 1,
        "a genuinely missing property must still be rejected, got: {diags:?}"
    );
}

#[test]
fn split_accessor_read_covariance_still_enforced() {
    // Negative case: a real split accessor still enforces covariant reads, so a
    // `string` getter is not assignable to a `"lit"` target property.
    let source = r#"
interface Source { get x(): string; set x(v: string); }
declare let s: Source;
const t: { x: "lit" } = s;
"#;
    let diags = diagnostics(source);
    assert!(
        ts2322_count(&diags) >= 1,
        "split accessor read covariance must still be enforced, got: {diags:?}"
    );
}

#[test]
fn divergent_accessor_intersection_preserves_setter_type() {
    // Genuine divergent accessors merged via intersection must keep their
    // combined setter type so the contravariant write check still applies.
    // `A & B` setter for `x` is `(string | undefined) & (string | null)` =
    // `string`, so writing `undefined` must be rejected while `"ok"` is fine.
    // This guards against over-collapsing write==read (regressed
    // `divergentAccessorsTypes8` when the merge forced write to equal read).
    let source = r#"
interface A { get x(): string; set x(v: string | undefined); }
interface B { get x(): string; set x(v: string | null); }
type C = A & B;
declare let c: C;
const ok: string = c.x;
c.x = undefined;
c.x = "ok";
"#;
    let diags = diagnostics(source);
    let ts2322 = ts2322_count(&diags);
    assert_eq!(
        ts2322, 1,
        "only the `c.x = undefined` write must be rejected, got: {diags:?}"
    );
}
