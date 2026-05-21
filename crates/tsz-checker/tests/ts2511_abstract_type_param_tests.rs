use tsz_checker::test_utils::check_source_diagnostics;

fn codes(src: &str) -> Vec<u32> {
    check_source_diagnostics(src)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn has_code(src: &str, code: u32) -> bool {
    codes(src).contains(&code)
}

fn no_code(src: &str, code: u32) -> bool {
    !codes(src).contains(&code)
}

// Rule: when a `new` expression target is a type parameter T whose constraint
// resolves to an abstract construct signature (`typeof AbstractClass`), tsc
// emits TS2511. The constraint is the structural predicate — the identifier
// spelling of the type parameter or the abstract class is irrelevant.

/// Reported repro: basic generic factory over abstract class.
#[test]
fn type_param_constrained_to_typeof_abstract_basic() {
    assert!(
        has_code(
            r#"
abstract class A {}
function f<T extends typeof A>(ctor: T) { return new ctor(); }
"#,
            2511
        ),
        "expected TS2511 for T extends typeof AbstractClass"
    );
}

/// Renamed abstract class and factory — confirms the check is not name-specific.
#[test]
fn type_param_constrained_to_typeof_abstract_renamed() {
    assert!(
        has_code(
            r#"
abstract class Animal { abstract speak(): void; }
function factory<U extends typeof Animal>(ctor: U) { return new ctor(); }
"#,
            2511
        ),
        "expected TS2511 for U extends typeof Animal"
    );
}

/// Renamed type parameter letter — `K` instead of `T`.
#[test]
fn type_param_different_letter() {
    assert!(
        has_code(
            r#"
abstract class Shape {}
function create<K extends typeof Shape>(ctor: K) { return new ctor(); }
"#,
            2511
        ),
        "expected TS2511 regardless of type parameter letter"
    );
}

/// Non-abstract base class through the same generic factory — must NOT emit TS2511.
#[test]
fn type_param_constrained_to_typeof_concrete_no_error() {
    assert!(
        no_code(
            r#"
class Concrete {}
function factory<T extends typeof Concrete>(ctor: T) { return new ctor(); }
"#,
            2511
        ),
        "must NOT emit TS2511 for concrete class constraint"
    );
}

/// Constructible constraint (not abstract) — no TS2511, exercises TypeParam(None) path.
#[test]
fn constructible_constraint_no_ts2511() {
    assert!(
        no_code(
            r#"
function f<T extends new() => object>(ctor: T) { return new ctor(); }
"#,
            2511
        ),
        "must NOT emit TS2511 for a constructible (non-abstract) constraint"
    );
}

/// Type alias wrapping `typeof AbstractClass` as constraint — alias indirection.
#[test]
fn type_param_via_type_alias_constraint() {
    assert!(
        has_code(
            r#"
abstract class Base {}
type AbstractCtor = typeof Base;
function make<T extends AbstractCtor>(ctor: T) { return new ctor(); }
"#,
            2511
        ),
        "expected TS2511 when constraint is a type alias for typeof AbstractClass"
    );
}

/// Union constraint where both branches are abstract — still TS2511.
#[test]
fn type_param_union_constraint_both_abstract() {
    assert!(
        has_code(
            r#"
abstract class A {}
abstract class B {}
function f<T extends typeof A | typeof B>(ctor: T) { return new ctor(); }
"#,
            2511
        ),
        "expected TS2511 when constraint is a union of abstract classes"
    );
}

/// Direct abstract class instantiation still errors (regression guard).
#[test]
fn direct_abstract_instantiation_still_errors() {
    assert!(
        has_code(
            r#"
abstract class A {}
let x = new A();
"#,
            2511
        ),
        "regression: direct abstract class instantiation must still emit TS2511"
    );
}

/// Non-abstract subclass instantiation must not error (regression guard).
#[test]
fn concrete_subclass_no_error() {
    assert!(
        no_code(
            r#"
abstract class A {}
class B extends A {}
let b = new B();
"#,
            2511
        ),
        "regression: concrete subclass instantiation must not emit TS2511"
    );
}
