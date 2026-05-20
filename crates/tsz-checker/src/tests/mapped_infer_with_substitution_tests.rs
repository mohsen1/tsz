//! Regression tests for mapped+infer evaluation when the inferred type is
//! subsequently substituted into another generic application.
//!
//! Pattern (from `compiler/conformance/jsx/tsxLibraryManagedAttributes.tsx`):
//!
//! ```ts
//! interface PropTypeChecker<U, TRequired = false> {
//!     [checkedType]: TRequired extends true ? U : U | null | undefined;
//! }
//! type InferredPropTypes<P> = {
//!     [K in keyof P]: P[K] extends PropTypeChecker<infer T, infer U>
//!         ? PropTypeChecker<T, U>[typeof checkedType]
//!         : {}
//! };
//! ```
//!
//! For `P = { bar: PropTypeChecker<X, true> }`, tsc evaluates the per-key
//! conditional to the `true` branch, producing `PropTypeChecker<X, true>[checkedType]`
//! and finally `X`. tsz currently falls through to `{}`, indicating the
//! mapped per-key infer pattern fails to match the substituted application.

use crate::test_utils::check_source_diagnostics;

fn first_2322(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2322 = diags.iter().find(|d| d.code == 2322).unwrap_or_else(|| {
        panic!(
            "Expected TS2322, got: {:?}",
            diags
                .iter()
                .map(|d| (d.code, d.message_text.clone()))
                .collect::<Vec<_>>()
        )
    });
    ts2322.message_text.clone()
}

/// Direct alias `type N = (typeof node)[typeof checkedType]` — no mapped
/// type, just the conditional substitution. tsc preserves `ReactNode`.
#[test]
fn mapped_infer_substituted_alias_preserved_via_indexed_conditional() {
    let msg = first_2322(
        r#"
type ReactNode = string | number | object;
declare const checkedType: unique symbol;
interface PropChecker<U, R = false> {
    [checkedType]: R extends true ? U : U | null | undefined;
}
declare const node: PropChecker<ReactNode, true>;
type N = (typeof node)[typeof checkedType];
declare let x: N;
x = null;
"#,
    );
    assert!(
        msg.contains("'ReactNode'") || msg.contains("'N'"),
        "Direct conditional substitution should preserve ReactNode (or wrapper N). Got: {msg}"
    );
}

/// The full mapped+infer pattern from the failing tsxLibraryManagedAttributes
/// test. tsz currently falls through to `{}`; tsc evaluates correctly.
#[test]
fn mapped_per_key_infer_with_substitution_resolves_true_branch() {
    let msg = first_2322(
        r#"
type ReactNode = string | number | object;
declare const checkedType: unique symbol;
interface PropTypeChecker<U, TRequired = false> {
    [checkedType]: TRequired extends true ? U : U | null | undefined;
}
type InferredPropTypes<P> = {
    [K in keyof P]: P[K] extends PropTypeChecker<infer T, infer U>
        ? PropTypeChecker<T, U>[typeof checkedType]
        : {}
};

declare const propTypes: { bar: PropTypeChecker<ReactNode, true> };
type Props = InferredPropTypes<typeof propTypes>;
declare let bar: Props["bar"];
bar = null;
"#,
    );
    assert!(
        msg.contains("'ReactNode'"),
        "Mapped per-key infer should resolve to 'ReactNode' (true-branch via TRequired=true). Got: {msg}"
    );
    assert!(
        !msg.contains("type '{}'"),
        "Mapped per-key infer must NOT fall through to '{{}}' branch. Got: {msg}"
    );
}

/// Anti-hardcoding cover: same pattern with renamed identifiers.
/// If the fix relies on a hardcoded user-chosen name (`P`, `T`, `U`,
/// `K`, `TRequired`), this test breaks.
#[test]
fn mapped_per_key_infer_with_substitution_resolves_true_branch_renamed() {
    let msg = first_2322(
        r#"
type Renderable = string | number | object;
declare const tag: unique symbol;
interface Checker<V, R = false> {
    [tag]: R extends true ? V : V | null | undefined;
}
type Inferred<S> = {
    [Q in keyof S]: S[Q] extends Checker<infer X, infer Y>
        ? Checker<X, Y>[typeof tag]
        : never
};

declare const checks: { item: Checker<Renderable, true> };
type Result = Inferred<typeof checks>;
declare let item: Result["item"];
item = null;
"#,
    );
    assert!(
        msg.contains("'Renderable'"),
        "Renamed variant: must resolve to 'Renderable'. Got: {msg}"
    );
    assert!(
        !msg.contains("type 'never'"),
        "Renamed variant: must NOT fall through to 'never' branch. Got: {msg}"
    );
}
