//! Tests for TS2344: defer constraint check for `K in keyof T` where T is a
//! free type parameter.
//!
//! When the type argument is a bare type parameter K whose constraint is
//! `keyof T`, and T is itself a free type parameter (e.g. `T extends unknown[]`),
//! `K`'s base constraint must be kept as the deferred `keyof T` form. Resolving
//! it eagerly through T's constraint produces a concrete union of array method
//! names which then fails an outer numeric-string constraint check, producing
//! a false TS2344.
//!
//! tsc defers the check to instantiation time. We must too.
//!
//! Conformance test: `numericStringLiteralTypes.ts`.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostic_codes(source: &str) -> Vec<u32> {
    compile_and_get_diagnostic_codes_with_options(source, CheckerOptions::default())
}

fn compile_and_get_diagnostic_codes_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Mapped key K iterating `keyof T` (T a free type parameter constrained
/// to `unknown[]`) used as type argument to a generic constrained to a
/// numeric-string union must NOT emit TS2344. tsc defers this check.
#[test]
fn test_keyof_free_type_param_defers_ts2344() {
    let diagnostics = compile_and_get_diagnostic_codes(
        r#"
type T20<T extends number | `${number}`> = T;
type T21<T extends unknown[]> = { [K in keyof T]: T20<K> };
"#,
    );
    assert!(
        !diagnostics.contains(&2344),
        "expected no TS2344, got: {diagnostics:?}"
    );
}

/// Sanity check: a CONCRETE T (a tuple/array literal type) where keyof
/// resolves to a known set NOT satisfying the constraint should still
/// emit TS2344. The deferral is gated on T being free, not on the
/// constraint shape.
#[test]
fn test_keyof_concrete_array_emits_ts2344_when_constraint_unsatisfied() {
    let diagnostics = compile_and_get_diagnostic_codes(
        r#"
type Wants<T extends "foo"> = T;
type Probe = { [K in keyof string[]]: Wants<K> };
"#,
    );
    // keyof string[] includes "length", "push", etc. — not assignable to "foo"
    assert!(
        diagnostics.contains(&2344),
        "expected TS2344, got: {diagnostics:?}"
    );
}

/// Variant: K used in `T20<K>` where K's constraint is `keyof T` and
/// T is constrained to `Record<string, unknown>` (object-like). The
/// keyof resolution would surface only string literal property names
/// (none), and the constraint asks for the numeric-string literal union.
/// tsc defers; we must also defer.
#[test]
fn test_keyof_free_object_tparam_defers_ts2344() {
    let diagnostics = compile_and_get_diagnostic_codes(
        r#"
type Want<T extends number | `${number}`> = T;
type Probe<T extends Record<string, unknown>> = { [K in keyof T]: Want<K> };
"#,
    );
    assert!(
        !diagnostics.contains(&2344),
        "expected no TS2344, got: {diagnostics:?}"
    );
}

/// A mapped type that preserves a callable numeric index signature remains
/// callable when indexed by the same extracted numeric key space. This mirrors
/// the `coAndContraVariantInferences3.ts` pattern:
/// `Parameters<{ [P in Extract<keyof T, number>]: T[P] }[Extract<keyof T, number>]>`.
#[test]
fn test_mapped_numeric_key_index_preserves_callable_constraint_for_parameters() {
    let diagnostics = compile_and_get_diagnostic_codes(
        r#"
type Parameters<T extends (...args: any[]) => any> =
    T extends (...args: infer P) => any ? P : never;
type Extract<T, U> = T extends U ? T : never;

type OverloadDefinitions = { readonly [P in number]: (...args: any[]) => any; };
type OverloadKeys<T extends OverloadDefinitions> = Extract<keyof T, number>;
type OverloadParameters<T extends OverloadDefinitions> =
    Parameters<{ [P in OverloadKeys<T>]: T[P]; }[OverloadKeys<T>]>;
"#,
    );
    assert!(
        !diagnostics.contains(&2344),
        "mapped numeric-key callable indexed access should satisfy Parameters constraint, got: {diagnostics:?}"
    );
}

#[test]
fn test_mapped_numeric_key_callback_property_contextually_types_destructuring_param() {
    let diagnostics = compile_and_get_diagnostic_codes_with_options(
        r#"
type Parameters<T extends (...args: any[]) => any> =
    T extends (...args: infer P) => any ? P : never;
type Extract<T, U> = T extends U ? T : never;

type OverloadDefinitions = { readonly [P in number]: (...args: any[]) => any; };
type OverloadKeys<T extends OverloadDefinitions> = Extract<keyof T, number>;
type OverloadParameters<T extends OverloadDefinitions> =
    Parameters<{ [P in OverloadKeys<T>]: T[P]; }[OverloadKeys<T>]>;
type OverloadBinders<T extends OverloadDefinitions> =
    { [P in OverloadKeys<T>]: (args: OverloadParameters<T>) => boolean | undefined; };

declare function bind<T extends OverloadDefinitions>(
    overloads: T,
    binder: OverloadBinders<T>,
): void;

bind({
    0(node: string, count: number): string { return node; },
}, {
    0: ([node, count]) => node.length > count,
});
"#,
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !diagnostics.contains(&7031),
        "mapped numeric-key callback property should contextually type destructuring parameters, got: {diagnostics:?}"
    );
}

#[test]
fn test_fluent_mapped_numeric_key_callback_property_suppresses_provisional_ts7031() {
    let diagnostics = compile_and_get_diagnostic_codes_with_options(
        r#"
type Parameters<T extends (...args: any[]) => any> =
    T extends (...args: infer P) => any ? P : never;
type Extract<T, U> = T extends U ? T : never;

type OverloadDefinitions = { readonly [P in number]: (...args: any[]) => any; };
type OverloadKeys<T extends OverloadDefinitions> = Extract<keyof T, number>;
type OverloadParameters<T extends OverloadDefinitions> =
    Parameters<{ [P in OverloadKeys<T>]: T[P]; }[OverloadKeys<T>]>;
type OverloadBinders<T extends OverloadDefinitions> =
    { [P in OverloadKeys<T>]: (args: OverloadParameters<T>) => boolean | undefined; };

interface Builder {
    overload<T extends OverloadDefinitions>(overloads: T): {
        bind(binder: OverloadBinders<T>): void;
    };
}
declare function build(): Builder;

build().overload({
    0(node: string, count: number): string { return node; },
}).bind({
    0: ([node, count]) => node.length > count,
});
"#,
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !diagnostics.contains(&7031),
        "fluent generic mapped callback context should not leak provisional TS7031, got: {diagnostics:?}"
    );
}
