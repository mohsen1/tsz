//! Regression coverage for issue #8703.
//!
//! `push_type_parameters` previously called `fresh_type_param` every time
//! a declaration's signature was processed. Two processings of the same
//! `function f<T>(...)` therefore minted two structurally-equivalent but
//! distinct `TypeParameter` `TypeId`s, which propagated up through the
//! interner tables for every type that closed over `T`. The relation
//! engine's identity-based fast paths (`source == target`,
//! `source == union_member`, variance-against-union-target, canonical-id
//! equality) all missed because the two sides hashed to different
//! entries even when they represented the same instantiation. Recursive
//! generic aliases such as `Recur<T>` then exhausted the iteration
//! budget and produced a spurious `TS2859 "Excessive complexity
//! comparing types"` on assignments like
//! `const y: Recur<T> | undefined = x` where `x: Recur<T>`.
//!
//! The fix caches a canonical `TypeId` per type-parameter declaration's
//! `DefId` in `DefinitionStore::type_param_for_def` so that the second
//! and later processings reuse the first allocation when the
//! `TypeParamInfo` content matches. Cross-declaration distinctness is
//! preserved by keying on the declaration `DefId`, not on the
//! `TypeParamInfo` content.
//!
//! Test matrix here follows §26 of the agent spec: the structural rule
//! is the focus, so coverage varies the user-chosen alias name, the
//! type-parameter name, the union arity around the recursive
//! reference, and the negative direction (two different declarations
//! must stay distinct).

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

fn assert_no_excessive_complexity(source: &str, label: &str) {
    let diags = diagnostics(source);
    let ts2859 = diags.iter().filter(|d| d.code == 2859).collect::<Vec<_>>();
    assert!(
        ts2859.is_empty(),
        "[{label}] no TS2859 should fire; got: {ts2859:?}",
    );
    let ts2589 = diags.iter().filter(|d| d.code == 2589).collect::<Vec<_>>();
    assert!(
        ts2589.is_empty(),
        "[{label}] no TS2589 should fire; got: {ts2589:?}",
    );
}

/// The reduced repro from issue #8703. Source and target reference the
/// same generic alias instantiated with the function's own type
/// parameter; assignment to a union containing the same instantiation
/// must succeed without iteration-budget exhaustion.
#[test]
fn recur_assignable_to_self_or_undefined() {
    let source = r#"
type Recur<T> = (
    T extends (unknown[]) ? {} : { [K in keyof T]?: Recur<T[K]> }
) | ['marker', ...Recur<T>[]];

function f<T>(x: Recur<T>): void {
    const y: Recur<T> | undefined = x;
}
"#;
    assert_no_excessive_complexity(source, "self-or-undefined");
}

/// Alias name independence: the structural rule must hold when the
/// recursive alias is renamed away from `Recur`.
#[test]
fn renamed_alias_same_rule() {
    let source = r#"
type Tree<U> = (
    U extends (unknown[]) ? {} : { [K in keyof U]?: Tree<U[K]> }
) | ['leaf', ...Tree<U>[]];

function g<U>(x: Tree<U>): void {
    const y: Tree<U> | undefined = x;
}
"#;
    assert_no_excessive_complexity(source, "renamed-alias");
}

/// Type-parameter name independence: the cached `DefId -> TypeId`
/// table must not collapse the rule onto a specific name spelling.
#[test]
fn renamed_type_parameter_same_rule() {
    let source = r#"
type R<P> = ({ k?: P }) | ['m', ...R<P>[]];

function h<P>(x: R<P>): void {
    const y: R<P> | undefined = x;
}
"#;
    assert_no_excessive_complexity(source, "renamed-type-parameter");
}

/// Union arity around the recursive reference does not change the
/// rule. The recursive instantiation must match a single member of an
/// arbitrarily-sized union without expansion.
#[test]
fn larger_union_with_recursive_member() {
    let source = r#"
type R<T> = ({ k?: T }) | ['m', ...R<T>[]];

function h<T>(x: R<T>): void {
    const a: R<T> | null = x;
    const b: R<T> | string = x;
    const c: R<T> | undefined | null = x;
}
"#;
    assert_no_excessive_complexity(source, "larger-union");
}

/// `Recur<T>[]` recurses through an `Array` element type. The fix
/// must also keep this assignable through a union containing the same
/// array shape.
#[test]
fn recur_array_through_union_member() {
    let source = r#"
type R<T> = ({ k?: T }) | ['m', ...R<T>[]];

function h<T>(xs: R<T>[]): void {
    const ys: R<T>[] | undefined = xs;
}
"#;
    assert_no_excessive_complexity(source, "recur-array-through-union");
}

/// Negative case: two distinct generic declarations that happen to
/// share the type-parameter name `T` must NOT collapse onto a single
/// `TypeId`. The mismatch below must still be reported as `TS2322`.
#[test]
fn distinct_declarations_stay_distinct() {
    let source = r#"
function p<T>(): T { return null as any; }
function q<T>(): T { return null as any; }
const x = p<string>();
const y = q<number>();
const z: string = y;
"#;
    let diags = diagnostics(source);
    let ts2322 = diags.iter().filter(|d| d.code == 2322).collect::<Vec<_>>();
    assert_eq!(
        ts2322.len(),
        1,
        "distinct declarations must remain distinct; got: {diags:?}",
    );
}

/// Self-referential recursive mapped alias used at a non-trivial
/// instantiation: `Transform<T>` from `recursiveMappedTypes.ts`. The
/// member-access through a property-with-union element type closes
/// over the same alias instantiation more than once during type
/// computation; the fix must not introduce TS2859 here. The full
/// upstream fixture is covered by the conformance suite — this test
/// stays narrow so unrelated diagnostic changes do not flip it.
#[test]
fn nested_recursive_transform_alias_no_excessive_complexity() {
    let source = r#"
type Transform<T> = { [K in keyof T]: Transform<T[K]> };

interface User { avatar: string; }
interface Guest { displayName: string; }
interface Product { users: (User | Guest)[]; }

declare var product: Transform<Product>;
product.users;
"#;
    assert_no_excessive_complexity(source, "transform-product");
}
