//! Regression tests for TS2313 circular generic constraint detection.

use tsz_checker::test_utils::check_source_diagnostics;

fn ts2313_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2313)
        .count()
}

// --- Direct self-referential alias-application constraints (issue #9778) ---
//
// Structural rule: when a type parameter's constraint is a generic type-alias
// application that, once expanded, reduces (along the base-constraint resolution
// path) back to the parameter itself, the parameter has a circular constraint.
// A type alias is transparent (expands to its body); interfaces/classes and
// aliases whose body is opaque (object/conditional) are not, preserving valid
// F-bounded polymorphism.

#[test]
fn direct_self_referential_alias_constraint_emits_ts2313() {
    // `Self<T>` expands to its body `T`, so `T extends Self<T>` is circular.
    assert_eq!(
        ts2313_count("type Self<T extends Self<T>> = T;"),
        1,
        "Expected TS2313 for direct self-referential alias constraint"
    );
}

#[test]
fn direct_self_referential_alias_constraint_is_name_independent() {
    // Same rule with a different parameter/alias spelling (anti-hardcoding §25).
    assert_eq!(
        ts2313_count("type Loop<K extends Loop<K>> = K;"),
        1,
        "Expected TS2313 regardless of the chosen identifier"
    );
}

#[test]
fn self_referential_alias_constraint_through_intermediate_alias_emits_ts2313() {
    // Generalization: the transparent alias need not be the enclosing one.
    // `Wrap<T>` expands to `T`, so `T extends Wrap<T>` is circular.
    let source = r"
type Wrap<X> = X;
type G<T extends Wrap<T>> = T;
";
    assert_eq!(
        ts2313_count(source),
        1,
        "Expected TS2313 when an intermediate transparent alias reduces to the parameter"
    );
}

#[test]
fn self_referential_alias_constraint_through_union_alias_emits_ts2313() {
    // Generalization: union members are on the base-constraint resolution path.
    // `Un<T>` expands to `T | string`, so `T extends Un<T>` is circular.
    let source = r"
type Un<X> = X | string;
type H<T extends Un<T>> = T;
";
    assert_eq!(
        ts2313_count(source),
        1,
        "Expected TS2313 when a transparent alias reduces to a union containing the parameter"
    );
}

#[test]
fn self_referential_alias_constraint_through_index_access_emits_ts2313() {
    // Generalization: index access is on the base-constraint resolution path.
    // `Id<T>["x"]` still contains a transparent alias application that reduces
    // back to `T`, so the constraint is circular.
    let source = r#"
type Id<X> = X;
type A<T extends Id<T>["x"]> = T;
"#;
    assert_eq!(
        ts2313_count(source),
        1,
        "Expected TS2313 when index access reveals a transparent alias application"
    );
}

#[test]
fn mutual_two_parameter_circular_constraints_still_emit_ts2313() {
    // Regression guard for the pre-existing mutual-cycle detection.
    assert_eq!(
        ts2313_count("type Pair<A extends B, B extends A> = [A, B];"),
        2,
        "Expected TS2313 for both parameters of a mutual circular constraint"
    );
}

#[test]
fn fbounded_constraint_over_interface_does_not_emit_ts2313() {
    // Negative control: an interface instantiation is opaque (nominal), not a
    // transparent alias, so valid F-bounded polymorphism must not be flagged.
    let source = r"
interface C<T> { f(x: T): void }
type Ok<T extends C<T>> = T;
";
    assert_eq!(
        ts2313_count(source),
        0,
        "Expected no TS2313 for F-bounded polymorphism over an interface"
    );
}

#[test]
fn fbounded_constraint_over_object_alias_does_not_emit_ts2313() {
    // Negative control: `Box<T>` expands to `{ value: T }`; object property
    // types are NOT on the base-constraint resolution path, so this is valid.
    let source = r"
type Box<X> = { value: X };
type FB<T extends Box<T>> = T;
";
    assert_eq!(
        ts2313_count(source),
        0,
        "Expected no TS2313 for F-bounded polymorphism over an object-shaped alias"
    );
}

#[test]
fn conditional_alias_constraint_does_not_emit_ts2313() {
    // Negative control: `Foo<S>` expands to a conditional type, which is opaque
    // at the base-constraint resolution level, so `S extends Foo<S>` is valid.
    let source = r"
type Foo<T> = [T] extends [number] ? {} : {};
type Bar<S extends Foo<S>> = S;
";
    assert_eq!(
        ts2313_count(source),
        0,
        "Expected no TS2313 for a constraint whose alias body is a conditional type"
    );
}

#[test]
fn mixin_constructor_alias_constraint_no_false_ts2313() {
    let source = r#"
type Constructor = new (...args: any[]) => {};

declare const Object: Constructor;

const Mixin1 = <C extends Constructor>(Base: C) => class extends Base { private _fooPrivate: {}; }

type FooConstructor = typeof Mixin1 extends (a: Constructor) => infer Cls ? Cls : never;
const Mixin2 = <C extends FooConstructor>(Base: C) => class extends Base {};

class C extends Mixin2(Mixin1(Object)) {}
"#;
    let diags = check_source_diagnostics(source);

    assert!(
        diags.iter().all(|d| d.code != 2313),
        "Expected no TS2313 for mixin constructor alias constraint, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
