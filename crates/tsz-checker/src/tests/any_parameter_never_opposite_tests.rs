//! `never` opposite an `any` parameter under `strictFunctionTypes`.
//!
//! Structural rule: when one signature's parameter is exactly `any` and the
//! other's is exactly `never`, tsc does not let `any` silence the pair. Its
//! `isSimpleTypeRelatedTo` rejects `any -> never` before any `any` allowance
//! applies, so the contravariant parameter check (`target <: source`) rejects
//! `(u: never) => R` against `(u: any) => R2`, while the mirrored pair stays
//! compatible through `never <: any`. tsz decides this in the solver, in
//! `are_parameters_compatible_impl`
//! (`relations/subtype/rules/functions/mod.rs`): the permissive `any`
//! shortcut now skips the `never`-opposite pair and lets the ordinary
//! variance-directed check answer — the same check that already gets
//! `ReadonlyArray<any>` vs `ReadonlyArray<never>` right.
//!
//! Method parameters stay bivariant, so a `never`-parameter method still
//! satisfies an `any`-parameter method through the reverse direction.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_function_types: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    check_source(source, "main.ts", strict_opts())
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn never_parameter_source_is_rejected_against_any_parameter_target() {
    let source = "declare function accept(fn: (u: any) => any): void;\n\
                  declare const produce: (u: never) => number;\n\
                  accept(produce);\n";
    assert_eq!(
        codes(source),
        vec![2345],
        "`(u: never) => number` must not be assignable to `(u: any) => any`: \
         the contravariant parameter check is `any <: never`, which tsc rejects"
    );
}

#[test]
fn any_parameter_source_is_accepted_against_never_parameter_target() {
    let source = "declare function accept(fn: (u: never) => any): void;\n\
                  declare const produce: (u: any) => number;\n\
                  accept(produce);\n";
    assert!(
        codes(source).is_empty(),
        "the mirrored pair is compatible through `never <: any`; \
         only the `any`-in-target-position direction rejects"
    );
}

#[test]
fn renamed_binders_do_not_change_the_verdict() {
    let source = "declare function consume(handler: (payload: any) => void): void;\n\
                  declare const emitter: (bottom: never) => void;\n\
                  consume(emitter);\n";
    assert_eq!(
        codes(source),
        vec![2345],
        "the rule is structural: renaming the parameter binders must not change it"
    );
}

#[test]
fn alias_wrapped_signature_is_rejected_the_same_way() {
    let source = "type Handler<A, U> = (u: U) => A;\n\
                  declare function accept(fn: Handler<any, any>): void;\n\
                  declare const produce: Handler<number, never>;\n\
                  accept(produce);\n";
    assert_eq!(
        codes(source),
        vec![2345],
        "wrapping both signatures in the same generic alias must not hide the pair"
    );
}

#[test]
fn callback_position_parameter_pair_stays_bivariant() {
    let source = "declare function accept(fn: (inner: (u: any) => any) => void): void;\n\
                  declare const produce: (inner: (u: never) => number) => void;\n\
                  accept(produce);\n";
    assert!(
        codes(source).is_empty(),
        "`strictFunctionTypes` does not reach a parameter that is itself a callback \
         parameter, so tsc relates this pair bivariantly and accepts it"
    );
}

#[test]
fn method_parameters_stay_bivariant() {
    let source = "declare function accept(o: { run(u: any): any }): void;\n\
                  declare const produce: { run(u: never): number };\n\
                  accept(produce);\n";
    assert!(
        codes(source).is_empty(),
        "method parameters are bivariant even under strictFunctionTypes, so the \
         reverse direction (`never <: any`) accepts this pair"
    );
}

#[test]
fn any_opposite_a_non_never_parameter_is_still_silenced() {
    let source = "declare function accept(fn: (u: any) => any): void;\n\
                  declare const produce: (u: string) => number;\n\
                  accept(produce);\n";
    assert!(
        codes(source).is_empty(),
        "the narrowing applies to `never` only: `any` must keep silencing every \
         other parameter mismatch it silenced before"
    );
}

#[test]
fn never_in_a_covariant_type_argument_is_unchanged() {
    let source = "interface Box<T> { readonly value: T }\n\
                  declare function accept(b: Box<never>): void;\n\
                  declare const b: Box<any>;\n\
                  accept(b);\n";
    assert_eq!(
        codes(source),
        vec![2345],
        "the pre-existing covariant `any` -> `never` rejection must not regress"
    );
}
