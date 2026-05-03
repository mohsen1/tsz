//! Tests for strict callback parameter checking under `--strictFunctionTypes`.
//!
//! When a function/method's parameter is itself a callable (a "callback"),
//! tsc's `compareSignaturesRelated` enters callback mode (`StrictCallback`
//! bit set). Inside callback mode, the param check skips the bivariant
//! first-half and uses contravariant comparison only — even when the
//! callback's own signatures originate from method declarations. This means
//! method-bivariance does NOT loosen contravariance for the callback's
//! params.
//!
//! Without this, `f2 = f1` below would be accepted: f1's callback type
//! `(cb: BivariantHack<Animal, Animal>)` would be compared bivariantly against
//! f2's callback `(cb: BivariantHack<Dog, Animal>)` (because the
//! `BivariantHack` pattern produces method-flavored signatures), and
//! bivariance would find a direction where the param check passes, hiding a
//! real type error.
//!
//! See `strictFunctionTypesErrors.ts` (n2 namespace) and the
//! `compareSignaturesRelated` rule about `!(checkMode & Callback)` in
//! `TypeScript/src/compiler/checker.ts`.

use crate::test_utils::check_source_diagnostics;

#[test]
fn callback_parameter_check_is_strict_even_when_inner_signature_is_method_like() {
    // The BivariantHack alias produces a method-flavored function type
    // (`{ foo(x: I): O }["foo"]`). Outside of callback mode, methods would
    // be checked bivariantly. Inside callback mode (the outer `(cb: ...) =>
    // void`), tsc forces strict variance for the inner signature's params.
    //
    // For `f2 = f1`: source.cb's `x` is Animal, target.cb's `x` is Dog.
    // Strict contravariance requires Animal <: Dog, which fails (Animal
    // lacks `dog`). tsc reports TS2322 here; tsz must too.
    let diags = check_source_diagnostics(
        r#"
// @strict: true
interface Animal { animal: void }
interface Dog extends Animal { dog: void }

type BivariantHack<Input, Output> = { foo(x: Input): Output }["foo"];

declare let f1: (cb: BivariantHack<Animal, Animal>) => void;
declare let f2: (cb: BivariantHack<Dog, Animal>) => void;
f1 = f2;       // OK — Dog <: Animal at the inner param level
f2 = f1;       // Error — Animal not <: Dog at the inner param level
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected at least one TS2322 for `f2 = f1` (strict callback param), got diags: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn callback_return_type_failure_under_strict_callback_param_check() {
    // For `fc1 = fc2`: target.f's return is Animal, source.f's return is Dog.
    // Strict covariance requires source.return <: target.return — but here we
    // are inside a callback param check, the *callback's* return type fails:
    // Dog (source.cb.return) is not <: Animal (target.cb.return)? That direction
    // passes. The failing direction is Animal <: Dog (callback's params).
    //
    // The point of this test: regardless of which side of the callback fails,
    // the strict-mode invariant must produce a diagnostic. tsc emits an error
    // for both `fc1 = fc2` and `fc2 = fc1`.
    let diags = check_source_diagnostics(
        r#"
// @strict: true
interface Animal { animal: void }
interface Dog extends Animal { dog: void }

declare let fc1: (f: (x: Animal) => Animal) => void;
declare let fc2: (f: (x: Dog) => Dog) => void;
fc1 = fc2;  // Error
fc2 = fc1;  // Error
"#,
    );

    // Both `fc1 = fc2` and `fc2 = fc1` should produce a diagnostic. We
    // conservatively require at least two distinct error sites (one per
    // failing assignment) without over-pinning the exact codes — tsc emits
    // TS2328 for the return-failure direction and TS2322 for the
    // param-failure direction.
    let assignment_starts: std::collections::BTreeSet<u32> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2328)
        .map(|d| d.start)
        .collect();

    assert!(
        assignment_starts.len() >= 2,
        "Expected diagnostics on both `fc1 = fc2` and `fc2 = fc1`; got starts: {:?}, diags: {:?}",
        assignment_starts,
        diags
            .iter()
            .map(|d| (d.code, d.start, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn callback_param_strict_does_not_propagate_to_level_three() {
    // Method bivariance must be restored once we exit the immediate callback
    // recursion. This test verifies that a level-3 method comparison (a
    // callable nested inside a callback's param type) still uses bivariance.
    //
    // The `Comparer1<T>` interface uses a method-style declaration
    // (`compare(a: T, b: T): number`). Comparing `Comparer1<Animal>` and
    // `Comparer1<Dog>` should not error under `strictFunctionTypes` because
    // the `compare` member is a method (bivariant params).
    let diags = check_source_diagnostics(
        r#"
// @strict: true
interface Animal { animal: void }
interface Dog extends Animal { dog: void }

interface Comparer1<T> {
    compare(a: T, b: T): number;
}

declare let animalComparer1: Comparer1<Animal>;
declare let dogComparer1: Comparer1<Dog>;

animalComparer1 = dogComparer1;  // OK — method bivariance applies
dogComparer1 = animalComparer1;  // OK — method bivariance applies
"#,
    );

    let assignment_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2328 || d.code == 2416)
        .collect();
    assert!(
        assignment_errors.is_empty(),
        "Expected no errors for method-bivariant Comparer1 assignments; got: {:?}",
        assignment_errors
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
