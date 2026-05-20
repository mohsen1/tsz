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

use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, LibContext};
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::load_lib_files;
use tsz_common::common::{ModuleKind, ScriptTarget};
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

fn compile_and_get_diagnostic_messages_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
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
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn check_multi_file_with_libs(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
    lib_files: &[Arc<LibFile>],
) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file_with_libs(parser.get_arena(), root, lib_files);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .unwrap_or_else(|| panic!("entry_file {entry_file:?} not found in files"));
    let (resolved_module_paths, resolved_modules) =
        tsz_checker::module_resolution::build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    let lib_contexts: Vec<LibContext> = lib_files
        .iter()
        .map(|lib| LibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);
    checker.ctx.diagnostics.clone()
}

#[test]
fn imported_const_array_item_type_satisfies_record_key_constraint() {
    let libs = load_lib_files(&["es5.d.ts"]);
    let diags = check_multi_file_with_libs(
        &[
            (
                "./src/util/type-utils.ts",
                "export type ArrayItemType<T> = T extends ReadonlyArray<infer I> ? I : never;",
            ),
            (
                "./src/util/object-utils.ts",
                "export declare function freeze<T>(obj: T): Readonly<T>;",
            ),
            (
                "./src/util/log.ts",
                r#"
import { freeze } from './object-utils';
import type { ArrayItemType } from './type-utils';

const logLevels = ['query', 'error'] as const;
export const LOG_LEVELS: Readonly<typeof logLevels> = freeze(logLevels);
export type LogLevel = ArrayItemType<typeof LOG_LEVELS>;
type Levels = Record<LogLevel, boolean>;
"#,
            ),
        ],
        "./src/util/log.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        !diags.iter().any(|diag| diag.code == 2344),
        "ArrayItemType<typeof LOG_LEVELS> should satisfy Record's key constraint; got: {diags:#?}"
    );
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

#[test]
fn test_user_unique_prefix_property_stays_string_key() {
    let diagnostics = compile_and_get_diagnostic_messages_with_options(
        r#"
export {};

type Source = {
  __unique_1: number;
  ordinary: string;
};

type Assert<T extends true> = T;

type IsExactlyStringKeys =
  keyof Source extends "__unique_1" | "ordinary"
    ? "__unique_1" | "ordinary" extends keyof Source
      ? true
      : false
    : false;

type Check = Assert<IsExactlyStringKeys>;

type Copy = {
  [K in keyof Source]: Source[K];
};

declare const source: Source;

const value = source.__unique_1;
const key: keyof Source = "__unique_1";
const copy: Copy = {
  __unique_1: 1,
  ordinary: "ok"
};
const copyValue = copy.__unique_1;

value;
key;
copyValue;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics for user-authored __unique_1 string key, got: {diagnostics:#?}"
    );
}

#[test]
fn test_computed_unique_prefix_property_stays_string_key() {
    let diagnostics = compile_and_get_diagnostic_messages_with_options(
        r#"
export {};

const propKey = "__unique_1" as const;
const methodKey = "__unique_2" as const;
const accessorKey = "__unique_3" as const;

interface Source {
  [propKey]: number;
  [methodKey](): string;
  get [accessorKey](): boolean;
}

type Assert<T extends true> = T;
type Keys = keyof Source;
type KeyCheck =
  Keys extends "__unique_1" | "__unique_2" | "__unique_3"
    ? ("__unique_1" | "__unique_2" | "__unique_3") extends Keys
      ? true
      : false
    : false;
type _Proof = Assert<KeyCheck>;

declare const src: Source;
const n: number = src[propKey];
const s: string = src[methodKey]();
const b: boolean = src[accessorKey];

n;
s;
b;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics for computed __unique_* string keys, got: {diagnostics:#?}"
    );
}

#[test]
fn test_mapped_literal_unique_prefix_keys_stay_string_keys() {
    let diagnostics = compile_and_get_diagnostic_codes_with_options(
        r#"
export {};

type Keys = "__unique_1" | "ordinary";
type Box = { [K in Keys]: K };
type Read<K extends keyof Box> = Box[K];

type K = keyof Box;
type Assert<T extends true> = T;
type KeyCheck =
  K extends "__unique_1" | "ordinary"
    ? ("__unique_1" | "ordinary") extends K
      ? true
      : false
    : false;
type _Proof = Assert<KeyCheck>;

const value: Read<"__unique_1"> = "__unique_1";
value;
"#,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.is_empty(),
        "mapped literal __unique_* keys should remain string keys, got: {diagnostics:#?}"
    );
}

#[test]
fn test_type_literal_keyof_with_index_signature_uses_solver_key_space() {
    let diagnostics = compile_and_get_diagnostic_codes(
        r#"
type Source = {
  [key: string]: number;
  0: number;
};

type Read<K extends keyof Source> = Source[K];
type Hit = Read<string>;
"#,
    );
    assert!(
        !diagnostics.contains(&2536),
        "index-signature key space should satisfy indexed access, got: {diagnostics:?}"
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

/// Regression test for primitive-key constraint check false-positive on
/// SYMBOL when `base` is `string | number`.
///
/// The buggy display-string match (`"string | number"` /
/// `"string | number | symbol"`) was per-base, not per-primitive_key, so
/// SYMBOL was admitted as "present in base" when base displayed as
/// `string | number`. Combined with the `!is_assignable_to(SYMBOL,
/// inst_constraint)` check, this emitted a spurious TS2344 even when the
/// base satisfied the constraint.
#[test]
fn test_string_or_number_base_does_not_falsely_demand_symbol_constraint() {
    let diagnostics = compile_and_get_diagnostic_codes(
        r#"
type SomeType = { [k: string]: any; [n: number]: any };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };

declare function pickFrom<T, K extends string | number>(
    obj: T,
    keys: K[],
): Pick<T, K & keyof T>;
"#,
    );
    assert!(
        !diagnostics.contains(&2344),
        "expected no TS2344 — `string | number` base satisfies the constraint without needing SYMBOL membership, got: {diagnostics:?}"
    );
}

#[test]
fn test_mapped_symbol_keys_satisfy_pick_keyof_constraint() {
    let diagnostics = compile_and_get_diagnostic_codes(
        r#"
declare const sym1: unique symbol;

type SymbolKeys<T> = {
  [K in keyof T]: K extends symbol ? K : never;
}[keyof T];

type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type OnlySymbolKeys<T> = Pick<T, SymbolKeys<T>>;

interface Example {
  [sym1]: number;
  name: string;
}

type Result = OnlySymbolKeys<Example>;
"#,
    );
    assert!(
        !diagnostics.contains(&2344),
        "expected no TS2344 for mapped symbol key extraction satisfying Pick's keyof constraint, got: {diagnostics:?}"
    );
}
