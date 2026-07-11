//! Contextual return-type inference through a type-alias application whose body
//! is a union.
//!
//! When a generic call (`x.then(cb)`) is contextually typed by a declared return
//! type that is a type-alias application (`Parse<Output>` where
//! `Parse<T> = Sync<T> | Async<T>` and `Async<T> = Thenable<Sync<T>>`), tsc
//! relates the signature return against the *reduced apparent type* of the
//! contextual type. The union arm that reuses the call's generic base
//! (`Thenable<Sync<T>>`) must seed the tracked return type parameter
//! (`then`'s `R1`); otherwise it collapses to `never` and a non-thenable
//! callback body is spuriously rejected against `PromiseLike<never>`
//! (canary false-positive on the zod / neverthrow rows).
//!
//! The harness loads no standard library, so each case declares a local
//! `Thenable` interface carrying a `then` signature shaped like `PromiseLike`.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_code_message_refs};

/// A `then` signature shaped like the lib's `PromiseLike.then`, plus the
/// `Sync`/`Async`/`Parse` alias family. `$name` renames every binder so the
/// assertions cannot depend on identifier text.
macro_rules! thenable_prelude {
    ($th:literal, $sync:literal, $async:literal, $parse:literal, $ok:literal, $bad:literal, $r1:literal, $r2:literal) => {
        concat!(
            "interface ",
            $th,
            "<T> {\n",
            "  then<",
            $r1,
            " = T, ",
            $r2,
            " = never>(\n",
            "    onfulfilled?: ((value: T) => ",
            $r1,
            " | ",
            $th,
            "<",
            $r1,
            ">) | null,\n",
            "    onrejected?: ((reason: any) => ",
            $r2,
            " | ",
            $th,
            "<",
            $r2,
            ">) | null\n",
            "  ): ",
            $th,
            "<",
            $r1,
            " | ",
            $r2,
            ">;\n",
            "}\n",
            "type ",
            $bad,
            " = { valid: false };\n",
            "declare const ",
            $bad,
            ": ",
            $bad,
            ";\n",
            "type ",
            $ok,
            "<T> = { valid: true; value: T };\n",
            "type ",
            $sync,
            "<T> = ",
            $ok,
            "<T> | ",
            $bad,
            ";\n",
            "type ",
            $async,
            "<T> = ",
            $th,
            "<",
            $sync,
            "<T>>;\n",
            "type ",
            $parse,
            "<T> = ",
            $sync,
            "<T> | ",
            $async,
            "<T>;\n",
        )
    };
}

fn assert_no_codes(source: &str, codes: &[u32], context: &str) {
    let diagnostics = check_source_diagnostics(source);
    let offending: Vec<_> = diagnostics
        .iter()
        .filter(|d| codes.contains(&d.code))
        .collect();
    assert!(
        offending.is_empty(),
        "{context}: expected none of {codes:?}, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

/// The dossier witness: `Output = T["_output"]` is a deferred indexed access,
/// so `Parse<Output>` stays an unreduced alias application at the point of
/// contextual inference. The `Async<Output>` arm (an alias wrapping the
/// thenable) must still seed `then`'s `R1`, keeping the callback body's
/// non-thenable return assignable.
#[test]
fn alias_union_return_with_deferred_indexed_access_is_clean() {
    let source = concat!(
        thenable_prelude!(
            "Thenable", "Sync", "Async", "Parse", "OK", "INVALID", "R1", "R2"
        ),
        "class Base<Output> {\n",
        "  _output!: Output;\n",
        "  run(): Thenable<Sync<Output>> { return null as any; }\n",
        "}\n",
        "type AnyBase = Base<any>;\n",
        "class Eff<T extends AnyBase, Output = T[\"_output\"]> extends Base<Output> {\n",
        "  schema!: T;\n",
        "  go(): Parse<Output> {\n",
        "    return this.schema.run().then((val) => INVALID);\n",
        "  }\n",
        "}\n",
    );
    assert_no_codes(
        source,
        &[2741, 2345, 2322],
        "deferred indexed-access alias union return",
    );
}

/// Renamed-binder variant of the witness: identical structure, every alias /
/// type parameter / property renamed. Guards against any identifier-string
/// dependence in the fix.
#[test]
fn alias_union_return_renamed_binders_is_clean() {
    let source = concat!(
        thenable_prelude!(
            "Deferred", "Good", "Later", "Outcome", "Hit", "Miss", "First", "Second"
        ),
        "class Node<Payload> {\n",
        "  _payload!: Payload;\n",
        "  eval(): Deferred<Good<Payload>> { return null as any; }\n",
        "}\n",
        "type AnyNode = Node<any>;\n",
        "class Wrap<N extends AnyNode, Payload = N[\"_payload\"]> extends Node<Payload> {\n",
        "  inner!: N;\n",
        "  go(): Outcome<Payload> {\n",
        "    return this.inner.eval().then((v) => Miss);\n",
        "  }\n",
        "}\n",
    );
    assert_no_codes(
        source,
        &[2741, 2345, 2322],
        "renamed-binder alias union return",
    );
}

/// Control: `Output` is a direct class type parameter (not a deferred indexed
/// access). Already clean on `main`; must stay clean.
#[test]
fn alias_union_return_direct_output_param_is_clean() {
    let source = concat!(
        thenable_prelude!(
            "Thenable", "Sync", "Async", "Parse", "OK", "INVALID", "R1", "R2"
        ),
        "class Base<Output> {\n",
        "  run(): Thenable<Sync<Output>> { return null as any; }\n",
        "}\n",
        "class Eff<Output> extends Base<Output> {\n",
        "  schema!: Base<Output>;\n",
        "  go(): Parse<Output> {\n",
        "    return this.schema.run().then((val) => INVALID);\n",
        "  }\n",
        "}\n",
    );
    assert_no_codes(source, &[2741, 2345, 2322], "direct output-param control");
}

/// Control: the return type is written as an inline union rather than an alias
/// application, so no alias expansion is required. Must stay clean.
#[test]
fn inline_union_return_is_clean() {
    let source = concat!(
        thenable_prelude!(
            "Thenable", "Sync", "Async", "Parse", "OK", "INVALID", "R1", "R2"
        ),
        "class Base<Output> {\n",
        "  _output!: Output;\n",
        "  run(): Thenable<Sync<Output>> { return null as any; }\n",
        "}\n",
        "type AnyBase = Base<any>;\n",
        "class Eff<T extends AnyBase, Output = T[\"_output\"]> {\n",
        "  schema!: T;\n",
        "  go(): Sync<Output> | Thenable<Sync<Output>> {\n",
        "    return this.schema.run().then((val) => INVALID);\n",
        "  }\n",
        "}\n",
    );
    assert_no_codes(source, &[2741, 2345, 2322], "inline union return control");
}

/// Control: the return type is a single (non-union) thenable application, so
/// there is no union to decompose. Must stay clean.
#[test]
fn non_union_thenable_return_is_clean() {
    let source = concat!(
        thenable_prelude!(
            "Thenable", "Sync", "Async", "Parse", "OK", "INVALID", "R1", "R2"
        ),
        "class Base<Output> {\n",
        "  _output!: Output;\n",
        "  run(): Thenable<Sync<Output>> { return null as any; }\n",
        "}\n",
        "type AnyBase = Base<any>;\n",
        "class Eff<T extends AnyBase, Output = T[\"_output\"]> extends Base<Output> {\n",
        "  schema!: T;\n",
        "  go(): Thenable<Sync<Output>> {\n",
        "    return this.schema.run().then((val) => INVALID);\n",
        "  }\n",
        "}\n",
    );
    assert_no_codes(
        source,
        &[2741, 2345, 2322],
        "non-union thenable return control",
    );
}

/// Convergence guard: the contextual return alias is self-referential
/// (`Rec<T> = Sync<T> | Thenable<Rec<T>>`). Arm expansion must terminate (it is
/// bounded) and type-checking must complete without hanging.
#[test]
fn self_referential_alias_union_return_terminates() {
    let source = concat!(
        "interface Thenable<T> {\n",
        "  then<R1 = T, R2 = never>(\n",
        "    onfulfilled?: ((value: T) => R1 | Thenable<R1>) | null\n",
        "  ): Thenable<R1 | R2>;\n",
        "}\n",
        "type INVALID = { valid: false };\n",
        "declare const INVALID: INVALID;\n",
        "type OK<T> = { valid: true; value: T };\n",
        "type Sync<T> = OK<T> | INVALID;\n",
        "type Rec<T> = Sync<T> | Thenable<Rec<T>>;\n",
        "class Base<Output> {\n",
        "  _output!: Output;\n",
        "  run(): Thenable<Sync<Output>> { return null as any; }\n",
        "}\n",
        "type AnyBase = Base<any>;\n",
        "class Eff<T extends AnyBase, Output = T[\"_output\"]> extends Base<Output> {\n",
        "  schema!: T;\n",
        "  go(): Rec<Output> {\n",
        "    return this.schema.run().then((val) => INVALID);\n",
        "  }\n",
        "}\n",
    );
    // Only asserting termination + no panic here; the self-referential arm is a
    // convergence stress case, not a parity witness.
    let _ = check_source_diagnostics(source);
}
