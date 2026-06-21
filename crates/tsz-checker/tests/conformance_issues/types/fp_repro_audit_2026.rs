//! False-positive reproduction audit (2026): triage of reported FP issues to
//! determine which are FIXED on `main` (kept as regression guards) and which
//! still REPRODUCE (kept as `#[ignore]`d witnesses documenting the bug).
//!
//! This file is triage-only: no source behavior is changed here. FIXED tests
//! assert the expected-correct (no-FP) behavior and pass. REPRODUCES tests
//! assert the same expected-correct behavior but are `#[ignore]`d with a
//! reference to the open issue, so the suite stays green while pinning the
//! witness.

use super::super::core::*;

/// #14230 (TS2536, single-file): a homomorphic-keys mapped type whose key set is
/// `string | number | symbol` is indexable by `symbol`; `M[symbol]` must not
/// report TS2536 ("Type 'symbol' cannot be used to index type 'M'").
#[test]
#[ignore = "reproduces #14230; FP still present (TS2536 on `M[symbol]` for a `string | number | symbol`-keyed mapped type)"]
fn issue_14230_mapped_over_keyof_any_indexed_by_symbol_no_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type M = { [K in string | number | symbol]: number };
type A = M[symbol];
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2536),
        "no TS2536 expected — `M` is keyed by `string | number | symbol`, so `M[symbol]` \
         is a valid indexed access. Actual: {diagnostics:#?}"
    );
    // Negative control: indexing by a key NOT in the key set must still report
    // TS2536. `boolean` is not part of `string | number | symbol`.
    let neg = compile_and_get_diagnostics(
        r#"
type M = { [K in string | number]: number };
type Bad = M[boolean];
export {};
"#,
    );
    assert!(
        has_error(&neg, 2536),
        "TS2536 expected — `boolean` is not in the `string | number` key set of `M`. \
         Actual: {neg:#?}"
    );
}

/// #14228 (TS2503, multi-file): `export * as Ns from './m'` produces a namespace
/// binding usable as a type-qualifier (`Ns.SomeType`) after it is re-imported.
/// Using `GlobalsGuard.TTypeArray` must not report TS2503 ("Cannot find
/// namespace `GlobalsGuard`").
#[test]
fn issue_14228_export_star_as_namespace_type_qualifier_no_ts2503() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        (
            "globals.ts",
            "export type TTypeArray = Int8Array | Uint8Array",
        ),
        ("index.ts", "export * as GlobalsGuard from './globals'"),
        (
            "consumer.ts",
            "import { GlobalsGuard } from './index'\nfunction f(v: GlobalsGuard.TTypeArray): GlobalsGuard.TTypeArray { return v }",
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "consumer.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2503),
        "no TS2503 expected — `export * as GlobalsGuard` re-imported as a value binding \
         is a usable namespace type-qualifier (`GlobalsGuard.TTypeArray`). Actual: {diagnostics:#?}"
    );
}

/// #14225 (TS2503, multi-file): a re-exported namespace import (`import * as T`,
/// then `export { T }`) re-imported elsewhere is usable as a type-qualifier
/// (`T.Intersect<...>`) even when a same-named local *type* `T` shadows in type
/// space. Using `T.Intersect` must not report TS2503.
#[test]
#[ignore = "reproduces #14225; FP still present (TS2503 'Cannot find namespace T' on a re-exported namespace import shadowed by a local `type T`; also emits a downstream TS2661)"]
fn issue_14225_reexported_namespace_qualifier_with_local_type_shadow_no_ts2503() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        ("lib.ts", "export type Intersect<A, B> = A & B"),
        ("index.ts", "import * as T from './lib'\nexport { T }"),
        (
            "use.ts",
            "import { T } from './index'\ntype T = [1, 2, 3]\ntype R = T.Intersect<{ a: 1 }, { b: 2 }>\nconst r: R = { a: 1, b: 2 }\nexport { r }",
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "use.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2503),
        "no TS2503 expected — the re-imported namespace binding `T` is usable as a \
         type-qualifier (`T.Intersect`) regardless of the local type-space `type T`. \
         Actual: {diagnostics:#?}"
    );
}

/// #13484 (generic base-class member, multi-file): a derived class extending a
/// generic base with a concrete type argument (`extends Base<string>`) inherits
/// the substituted member signature, so `d.getValue()` is `string`. Assigning
/// it to `string` must not report TS2322, and the member must resolve (no
/// TS2552/TS2304 "cannot find" family).
#[test]
fn issue_13484_generic_base_class_member_substituted_no_ts2322() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        (
            "base.ts",
            "export class Base<T> {\n  getValue(): T { return null as any; }\n}",
        ),
        (
            "main.ts",
            "import { Base } from './base'\nclass Derived extends Base<string> {}\nconst d = new Derived()\nconst s: string = d.getValue()",
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "main.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `Derived extends Base<string>` makes `d.getValue()` \
         `string`. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2552) && !has_error(&diagnostics, 2304),
        "no TS2552/TS2304 expected — the inherited `getValue` member must resolve. \
         Actual: {diagnostics:#?}"
    );

    // Negative control: a genuine mismatch must still error. `d.getValue()` is
    // `string`, so assigning it to `number` must report TS2322.
    let neg_files = &[
        (
            "base.ts",
            "export class Base<T> {\n  getValue(): T { return null as any; }\n}",
        ),
        (
            "main.ts",
            "import { Base } from './base'\nclass Derived extends Base<string> {}\nconst d = new Derived()\nconst n: number = d.getValue()",
        ),
    ];
    let neg = compile_named_files_get_diagnostics_with_lib_and_options(
        neg_files,
        "main.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert!(
        has_error(&neg, 2322),
        "TS2322 expected — `d.getValue()` is `string`, not assignable to `number`. \
         Actual: {neg:#?}"
    );
}

/// #14213 (TS7006, single-file): an expression-bodied arrow with an explicit
/// return-type annotation whose body is an object literal contextually types the
/// object literal's method params from the annotated return type. The nested
/// (curried) form `(a) => (): T => ({ m: (x, y) => ... })` must not report
/// TS7006 ("Parameter implicitly has an 'any' type") on `x`/`y`.
#[test]
#[ignore = "reproduces #14213; FP still present (TS7006 on inner-arrow object-literal method params under a curried annotated return)"]
fn issue_14213_curried_arrow_object_literal_method_params_no_ts7006() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Algebra { readonly meet: (x: number, y: number) => number }
export const f =
  (base: number) =>
  (): Algebra => ({
    meet: (x, y) => base + x + y
  })
"#,
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "no TS7006 expected — the inner arrow's `: Algebra` return annotation \
         contextually types the object literal's `meet` params `x`/`y`. \
         Actual: {diagnostics:#?}"
    );
}

/// #14156 (TS1360, single-file): a `satisfies` whose target property type is an
/// intersection of an object type and a call signature (`Meta & ((...args) =>
/// Fn)`). A function-typed source assignable to every constituent (object
/// constituent vacuously, call constituent by return covariance) must satisfy
/// the target — no TS1360.
#[test]
fn issue_14156_satisfies_intersection_object_and_call_signature_no_ts1360() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type LazyResult<R> = { done: boolean; next: R };
type LazyEvaluator<T = unknown, R = T> = (item: T, index: number, data: readonly T[]) => LazyResult<R>;
type LazyFn = (value: unknown, index: number, items: readonly unknown[]) => LazyResult<unknown>;
type LazyMeta = { readonly single?: boolean };
type LazyDefinition = {
  readonly lazy: LazyMeta & ((...args: any) => LazyFn);
  readonly lazyArgs: readonly unknown[];
};

export function make(lazy: (...args: any) => LazyEvaluator, args: readonly unknown[]) {
  const [, ...rest] = args;
  return { lazy, lazyArgs: rest } satisfies LazyDefinition;
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 1360),
        "no TS1360 expected — the source `lazy` function is assignable to each \
         constituent of `LazyMeta & ((...args: any) => LazyFn)` independently. \
         Actual: {diagnostics:#?}"
    );
}

/// #14231 (TS2677, single-file): a function-type-node type predicate whose
/// parameter type is written through a type alias must resolve the alias before
/// the predicate relation. `let g2: (p: A) => p is string` with `type A = string`
/// must not report TS2677 ("A type predicate's type must be assignable to its
/// parameter's type").
#[test]
#[ignore = "reproduces #14231; FP still present (TS2677 on alias-typed type-predicate parameter in a function-type node)"]
fn issue_14231_type_predicate_through_alias_no_ts2677() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Alias<T> = keyof T;
let g: <T>(p: Alias<T>) => p is keyof T;
type A = string;
let g2: (p: A) => p is string;
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2677),
        "no TS2677 expected — the alias-typed predicate parameter must be resolved \
         before the predicate-assignability relation. Actual: {diagnostics:#?}"
    );
    // Negative control: a predicate that genuinely doesn't assign must still error.
    let neg = compile_and_get_diagnostics(
        r#"
let g3: (p: string) => p is number;
export {};
"#,
    );
    assert!(
        has_error(&neg, 2677),
        "TS2677 expected — `p is number` is not assignable to `p: string`. \
         Actual: {neg:#?}"
    );
}

/// #14229 (TS2304, single-file): `typeof <param>` inside a function-type alias's
/// type-predicate asserted type must resolve the parameter, which is in scope for
/// every type position of the signature. `type Guard = (a: { z: string }) => a is
/// typeof a & { y: boolean }` must not report TS2304 ("Cannot find name 'a'").
#[test]
#[ignore = "reproduces #14229; FP still present (TS2304 'Cannot find name a' for `typeof a` in a function-type-alias predicate asserted type)"]
fn issue_14229_typeof_param_in_predicate_asserted_type_no_ts2304() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Guard = (a: { z: string }) => a is typeof a & { y: boolean };
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2304),
        "no TS2304 expected — the signature's value parameter `a` is in scope for \
         the predicate's asserted type (`typeof a`). Actual: {diagnostics:#?}"
    );
    // Negative control: an undeclared name in the asserted type must still error.
    let neg = compile_and_get_diagnostics(
        r#"
type Guard2 = (a: { z: string }) => a is typeof undeclared & { y: boolean };
export {};
"#,
    );
    assert!(
        has_error(&neg, 2304),
        "TS2304 expected — `typeof undeclared` references an undeclared name. \
         Actual: {neg:#?}"
    );
}

/// #14255 (TS2315, single-file): an `export { Orig as Exp }` rename whose
/// exported name collides with a string-mapping intrinsic must not create an
/// in-module binding for `Exp`; in-module `Capitalize<S>` must keep its intrinsic
/// meaning. The witness must not report TS2315 ("'Capitalize' is not generic").
#[test]
#[ignore = "reproduces #14255; FP still present (TS2315 'Type Local is not generic' — `export { Local as Capitalize }` shadows the string-mapping intrinsic in-module)"]
fn issue_14255_export_rename_shadowing_intrinsic_no_ts2315() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
export type Cap<S extends string> = Capitalize<S>;
interface Local { tag: 'kind' }
export { Local as Capitalize };
"#,
    );
    assert!(
        !has_error(&diagnostics, 2315),
        "no TS2315 expected — the renamed export `Capitalize` is recorded only on \
         the export surface; in-module `Capitalize<S>` keeps the intrinsic meaning. \
         Actual: {diagnostics:#?}"
    );
}

/// #14259 (TS2345, single-file): a self-referential generic call on a loop
/// back-edge (`h = pipe(h, f)`) must not leak the call's bare un-instantiated
/// return type parameter into `h`'s loop-flow type. The next iteration's argument
/// check must not report TS2345.
#[test]
fn issue_14259_self_referential_generic_call_loop_backedge_no_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function pipe<A, B = never>(a: A, ab: (a: A) => B): B;
declare const f: (self: number) => number;
let h = 6151;
for (let i = 0; i < 3; i++) { h = pipe(h, f); }
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "no TS2345 expected — a bare return type parameter from the self-referential \
         generic call must not become `h`'s loop-flow type. Actual: {diagnostics:#?}"
    );
}

/// #14258 (TS2749, multi-file): a barrel `export * from './apply'` re-export of a
/// `type` alias must keep its type meaning at a named import used in type
/// position. `import { apply } from './index'; type L = apply<[1,2,3]>` must not
/// report TS2749 ("'apply' refers to a value, but is being used as a type").
#[test]
fn issue_14258_export_star_reexport_type_alias_named_import_no_ts2749() {
    if !lib_files_available() {
        return;
    }
    let files = &[
        (
            "apply.ts",
            "type apply<X extends unknown[] = []> = X[\"length\"];\nexport { apply };",
        ),
        ("index.ts", "export * from './apply'"),
        (
            "consumer.ts",
            "import { apply } from './index'\ntype L = apply<[1, 2, 3]>\nconst l: L = 3\nexport { l }",
        ),
    ];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "consumer.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2749),
        "no TS2749 expected — `apply` re-exported via `export *` retains type \
         meaning at the named import used in type position. Actual: {diagnostics:#?}"
    );
}

/// #14216 (TS2552/TS2749, multi-file): a module declaring a local `class Box`
/// that also re-exports a distinct local binding under the same name via
/// `export { box as Box }`. In-module references to `Box` must keep the local
/// class meaning (value and type). No TS2749 at type sites, no TS2552 at value
/// sites.
#[test]
#[ignore = "reproduces #14216; FP still present (export-alias `export { box as Box }` clobbers the local `class Box` in value and type space)"]
fn issue_14216_export_alias_collides_local_class_no_ts2552_ts2749() {
    if !lib_files_available() {
        return;
    }
    let files = &[(
        "m.ts",
        "class Box { constructor(public value: number) {} }\n\
         const useType = (b: Box): number => b.value\n\
         const useValue = (): Box => new Box(1)\n\
         const box = (n: number) => new Box(n)\n\
         export { box as Box }",
    )];
    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        files,
        "m.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            strict_null_checks: true,
            module: tsz_common::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    assert!(
        !has_error(&diagnostics, 2749),
        "no TS2749 expected — in-module `Box` type positions resolve to the local \
         class, not the export alias. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2552),
        "no TS2552 expected — in-module `Box` value positions resolve to the local \
         class constructor. Actual: {diagnostics:#?}"
    );
}
