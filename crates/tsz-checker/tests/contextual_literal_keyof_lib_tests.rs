//! Regression tests for contextual literal narrowing through `keyof Lazy(LibType)`
//! and `IndexAccess(Lazy(LibType), key)`.
//!
//! When a string literal is assigned to an indexed-access or keyof target whose
//! object/operand is a `Lazy(DefId)` reference to a namespace interface (such as
//! `Intl.NumberFormatOptions` from the lib, or a user-declared namespace),
//! `evaluate_type_with_env` may not be able to resolve the Lazy because the def
//! hasn't been registered in the type environment yet. Previously this caused
//! fresh literals like `'currency'` to be widened to `string`, producing false
//! TS2322 errors. The fix forces a stronger Lazy resolution before retrying the
//! keyof evaluation, plus an `IndexAccess` fallback that looks up property types
//! through the contextual property API.
//!
//! Repro for the original arrayToLocaleStringES2015 / ES2020 conformance cases.
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn check(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

const NS_PRELUDE: &str = r#"
declare namespace Lib {
    interface StyleRegistry {
        decimal: never;
        percent: never;
        currency: never;
    }
    type Style = keyof StyleRegistry;
    interface Options {
        style?: Style | undefined;
        currency?: string | undefined;
    }
}
"#;

/// `const x: T = 'currency'` where `type T = Lib.Options['style']` must keep
/// the fresh literal narrow rather than widening to `string`. tsc accepts this
/// assignment (the literal matches `keyof StyleRegistry | undefined`).
#[test]
fn keeps_literal_narrow_via_alias_to_namespace_indexed_access() {
    let mut source = String::from(NS_PRELUDE);
    source.push_str(
        r#"
type S = Lib.Options["style"];
const x: S = "currency";
"#,
    );
    let diagnostics = check(&source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "literal 'currency' must satisfy contextual `keyof StyleRegistry | undefined` via Lazy alias; got {ts2322:?}",
    );
}

/// Direct indexed access on a namespace interface (`Lib.Options['style']`)
/// must also preserve fresh literals. This is the bare form before any alias
/// indirection.
#[test]
fn keeps_literal_narrow_via_direct_namespace_indexed_access() {
    let mut source = String::from(NS_PRELUDE);
    source.push_str(
        r#"
const x: Lib.Options["style"] = "currency";
"#,
    );
    let diagnostics = check(&source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "literal 'currency' must satisfy contextual `Lib.Options['style']`; got {ts2322:?}",
    );
}

/// Intersection of a namespace interface with `{}` must surface the
/// inner-property contextual type so an object literal property keeps its
/// fresh literal type. tsc accepts this; tsz used to widen to `string`.
#[test]
fn intersection_with_namespace_keeps_property_literal_narrow() {
    let mut source = String::from(NS_PRELUDE);
    source.push_str(
        r#"
const x: Lib.Options & {} = { style: "currency" };
"#,
    );
    let diagnostics = check(&source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "object literal `{{ style: 'currency' }}` must satisfy intersection target; got {ts2322:?}",
    );
}

/// Aliasing the intersection (`type T = Lib.X & {}`) must also narrow.
#[test]
fn alias_of_intersection_with_namespace_keeps_property_literal_narrow() {
    let mut source = String::from(NS_PRELUDE);
    source.push_str(
        r#"
type T = Lib.Options & {};
const x: T = { style: "currency" };
"#,
    );
    let diagnostics = check(&source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "object literal must narrow via aliased intersection target; got {ts2322:?}",
    );
}
