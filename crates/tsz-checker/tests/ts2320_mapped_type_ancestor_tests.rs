//! TS2320 for a base interface that inherits a conflicting member through a
//! non-interface ancestor (a mapped type or plain type-alias application in
//! its own `extends` clause), e.g. `interface Base extends Partial<T> {}`.
//!
//! `check_interface_extension_compatibility`'s cross-base member walk
//! (`crates/tsz-checker/src/classes/class_checker_compat.rs`) only enqueued
//! an ancestor onto its traversal worklist when the ancestor resolved to an
//! actual interface declaration. A base whose own heritage clause points at
//! a mapped type or type alias (not an interface) never reached that
//! worklist, so its members were silently dropped from the cross-base
//! comparison and a real conflict went unreported. tsc's `getBaseTypes` has
//! no such declaration-kind restriction: a base's fully-resolved property
//! set is part of an interface's inherited surface regardless of how it
//! arrived there.
//!
//! Fixing this surfaced a second, narrower gap: the property type read for
//! a *directly declared* base member (`get_type_of_interface_member_simple`)
//! unconditionally unions an optional property with `undefined`, while a
//! property folded in from a non-interface ancestor carries its raw
//! declared type. Under `exactOptionalPropertyTypes`, tsc's `addOptionality`
//! does *not* widen an optional property for this identity comparison, so
//! comparing an always-widened type against a raw one produced spurious
//! mismatches. Both sides are now normalized to the same convention before
//! comparison.
//!
//! Oracle-verified against pinned `typescript@7.0.2`.

use crate::test_utils::check_source_codes;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn codes_exact_optional(source: &str, exact: bool) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            exact_optional_property_types: exact,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn ts2320_mapped_ancestor_conflicts_under_exact_optional() {
    // `Base1` inherits `port` from `Partial<Base1Src>` (a mapped-type
    // ancestor of its own `extends` clause), giving `port` the raw
    // declared type `number`. `Base2` declares `port` explicitly as
    // `number | undefined`. Under `exactOptionalPropertyTypes`, these are
    // not identical.
    let source = r#"
type LocalPartial<T> = { [P in keyof T]?: T[P] };
interface Base1Src { port: number; }
interface Base1 extends LocalPartial<Base1Src> {}
interface Base2 { port?: number | undefined; }
interface Combined extends Base1, Base2 {
    other?: string;
}
"#;
    assert_eq!(
        codes_exact_optional(source, true),
        vec![2320],
        "expected TS2320 when exactOptionalPropertyTypes narrows Base1's mapped-ancestor `port` to `number`"
    );
}

#[test]
fn ts2320_mapped_ancestor_no_conflict_without_exact_optional() {
    // Same shape as above but without `exactOptionalPropertyTypes`: tsc's
    // `addOptionality` widens both sides to `number | undefined`, so they
    // are identical and no diagnostic fires.
    let source = r#"
type LocalPartial<T> = { [P in keyof T]?: T[P] };
interface Base1Src { port: number; }
interface Base1 extends LocalPartial<Base1Src> {}
interface Base2 { port?: number | undefined; }
interface Combined extends Base1, Base2 {
    other?: string;
}
"#;
    assert_eq!(
        codes_exact_optional(source, false),
        Vec::<u32>::new(),
        "expected no diagnostics when both sides widen to `number | undefined`"
    );
}

#[test]
fn ts2320_mapped_ancestor_type_mismatch_regardless_of_optionality() {
    // A genuine type mismatch (`number` vs `string`) through a mapped-type
    // ancestor must still fire TS2320 in both modes — the fix must not
    // suppress a real conflict.
    let source_true = r#"
type LocalPartial<T> = { [P in keyof T]?: T[P] };
interface Base1Src { port: number; }
interface Base1 extends LocalPartial<Base1Src> {}
interface Base2 { port: string; }
interface Combined extends Base1, Base2 {
    other?: string;
}
"#;
    assert_eq!(codes_exact_optional(source_true, true), vec![2320]);
    assert_eq!(codes_exact_optional(source_true, false), vec![2320]);
}

#[test]
fn ts2320_mapped_ancestor_matching_optional_types_no_conflict() {
    // Both sides declare `port` as plainly optional with no explicit
    // `undefined` (`port?: number` on both). tsc treats these as identical
    // in both modes: raw `number` under exactOptionalPropertyTypes, widened
    // `number | undefined` otherwise.
    let source = r#"
type LocalPartial<T> = { [P in keyof T]?: T[P] };
interface Base1Src { port: number; }
interface Base1 extends LocalPartial<Base1Src> {}
interface Base2 { port?: number; }
interface Combined extends Base1, Base2 {
    other?: string;
}
"#;
    assert_eq!(codes_exact_optional(source, true), Vec::<u32>::new());
    assert_eq!(codes_exact_optional(source, false), Vec::<u32>::new());
}

#[test]
fn ts2320_mapped_ancestor_redeclared_by_derived_no_ts2320() {
    // When the derived interface redeclares the property itself, tsc
    // resolves the conflict as a TS2430 override check against the
    // redeclared member, not as a TS2320 cross-base "cannot simultaneously
    // extend" — that redeclaration path is out of scope for the
    // mapped-ancestor cross-base fold (see the code comment at its `derived_members`
    // guard), so this must not additionally report TS2320.
    let source = r#"
type LocalPartial<T> = { [P in keyof T]?: T[P] };
interface Base1Src { port: number; }
interface Base1 extends LocalPartial<Base1Src> {}
interface Base2 { port?: number | undefined; }
interface Combined extends Base1, Base2 {
    port?: number | undefined;
}
"#;
    assert!(
        !codes_exact_optional(source, true).contains(&2320),
        "redeclared member must not additionally raise TS2320"
    );
}

#[test]
fn ts2320_plain_type_alias_ancestor_conflict() {
    // The non-interface ancestor need not be a mapped type — a plain type
    // alias to an object type in a base interface's own `extends` clause
    // hits the same gap.
    let source = r#"
type Base1Shape = { count: number; };
interface Base1 extends Base1Shape {}
interface Base2 { count: string; }
interface Combined extends Base1, Base2 {
    other?: string;
}
"#;
    assert!(check_source_codes(source).contains(&2320));
}

#[test]
fn ts2320_mapped_ancestor_conflict_renamed_binders() {
    // Same shape as the primary case with every identifier renamed, to
    // guard against a name-keyed rather than structural check.
    let source = r#"
type Widen<U> = { [K in keyof U]?: U[K] };
interface OptionsShapeZ { timeout: number; }
namespace outer {
    export interface RequestOptionsZ extends Widen<OptionsShapeZ> {}
}
interface RetryOptionsZ { timeout?: number | undefined; }
interface MergedOptionsZ extends outer.RequestOptionsZ, RetryOptionsZ {
    retries?: number;
}
"#;
    assert_eq!(codes_exact_optional(source, true), vec![2320]);
    assert_eq!(codes_exact_optional(source, false), Vec::<u32>::new());
}
