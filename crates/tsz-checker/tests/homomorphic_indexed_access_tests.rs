//! Tests for homomorphic mapped type indexed access correctness.
//!
//! Structural rule: for any homomorphic mapped type `H<T>` over `T`,
//! `keyof H<T>` = `keyof T`. Therefore any `K in keyof T` is also a
//! valid index for `H<T>`. The checker must not emit TS2536 for `H<T>[K]`
//! in such contexts.
//!
//! Covers: Required, Partial, Readonly, user-defined homomorphic types,
//! various modifier combinations (-?, +?, -readonly), and nested patterns.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_es5(source: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);
    assert!(!lib_files.is_empty(), "es5.d.ts lib file not loaded");
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
}

fn ts2536(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2536).collect()
}

fn ts2344(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2344).collect()
}

// ──────────────────────────────────────────────────────────────────────────
// Standard lib utilities
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn required_t_k_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]: Required<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Required<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn partial_t_k_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]: Partial<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Partial<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn readonly_t_k_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]: Readonly<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Readonly<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// ObjectEntries pattern (the secondary case from issue #6616)
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn object_entries_pattern_no_ts2536() {
    let diags =
        check_es5("type ObjectEntries<T> = { [K in keyof T]-?: [K, Required<T>[K]] }[keyof T];");
    assert!(
        ts2536(&diags).is_empty(),
        "ObjectEntries pattern must not emit TS2536: {diags:?}"
    );
}

#[test]
fn object_entries_concrete_use_no_errors() {
    let diags = check_es5(
        r#"type ObjectEntries<T> = { [K in keyof T]-?: [K, Required<T>[K]] }[keyof T];
type Obj = { a: number; b?: string };
type OE = ObjectEntries<Obj>;"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "ObjectEntries concrete use must not emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Modifier variants (-?, +?, -readonly)
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn required_t_k_with_remove_optional_modifier_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]-?: Required<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Required<T>[K] with -? modifier must not emit TS2536: {diags:?}"
    );
}

#[test]
fn partial_t_k_with_add_optional_modifier_no_ts2536() {
    let diags = check_es5("type Test<T> = { [K in keyof T]+?: Partial<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Partial<T>[K] with +? modifier must not emit TS2536: {diags:?}"
    );
}

#[test]
fn readonly_t_k_with_remove_readonly_modifier_no_ts2536() {
    let diags = check_es5("type Test<T> = { -readonly [K in keyof T]: Readonly<T>[K] };");
    assert!(
        ts2536(&diags).is_empty(),
        "Readonly<T>[K] with -readonly modifier must not emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// User-defined homomorphic utilities (renamed parameter)
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn user_defined_required_indexed_by_mapped_keyof_key_no_ts2536() {
    let diags = check_es5(
        "type MyReq<T> = { [P in keyof T]-?: T[P] };\n\
         type Test<T> = { [K in keyof T]: MyReq<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "User-defined MyReq<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn user_defined_partial_indexed_by_mapped_keyof_key_no_ts2536() {
    let diags = check_es5(
        "type MyPartial<T> = { [P in keyof T]?: T[P] };\n\
         type Test<T> = { [K in keyof T]: MyPartial<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "User-defined MyPartial<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn user_defined_readonly_indexed_by_mapped_keyof_key_no_ts2536() {
    let diags = check_es5(
        "type MyReadonly<T> = { readonly [P in keyof T]: T[P] };\n\
         type Test<T> = { [K in keyof T]: MyReadonly<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "User-defined MyReadonly<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

/// Renamed iteration var: outer K, inner P — must be independent names.
#[test]
fn different_iteration_var_names_no_ts2536() {
    let diags = check_es5(
        "type MyMap<T> = { readonly [Q in keyof T]: T[Q] };\n\
         type Test<T> = { [J in keyof T]: MyMap<T>[J] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Renamed vars Q/J must not emit TS2536 when T is the same: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Correct results (regression guard)
// ──────────────────────────────────────────────────────────────────────────

/// After removing optionality via Required, b? becomes required b: string.
#[test]
fn required_t_k_resolves_correctly() {
    let diags = check_es5(
        r#"type Test<T> = { [K in keyof T]: Required<T>[K] };
type Obj = { a: number; b?: string };
type T1 = Test<Obj>;
const t1: T1 = { a: 1, b: 'x' };"#,
    );
    assert!(
        diags.is_empty(),
        "Required<T>[K] should resolve to correct type without errors: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Negative cases: TS2536 must still be emitted for unrelated key spaces
// ──────────────────────────────────────────────────────────────────────────

/// K extends keyof A must NOT index B when B ≠ A.
#[test]
fn unrelated_keyof_still_emits_ts2536() {
    let diags = check_es5(
        "interface A { x: number; }\n\
         interface B { y: string; }\n\
         type Test<K extends keyof A> = B[K];",
    );
    assert!(
        !ts2536(&diags).is_empty(),
        "B[K] where K extends keyof A but B ≠ A must emit TS2536: {diags:?}"
    );
}

/// Local user-defined Required with different shape must still emit TS2536
/// (regression from `required_constraint_local_alias_tests`).
#[test]
fn local_required_unrelated_shape_emits_ts2536() {
    let diags = check_es5(
        "type Required<T> = { marker: string };\n\
         type Test<T> = { [K in keyof T]: Required<T>[K] };",
    );
    assert!(
        !ts2536(&diags).is_empty(),
        "Local Required with unrelated body must still emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Recursive conditional utility types (DeepRequired / DeepPartial patterns)
//
// Structural rule: when a generic alias body is `T extends C ? A : B` and
// each branch shares the source argument's key space (identity, or a
// non-remapped mapped type whose constraint is `keyof T`), `keyof F<T>` =
// `keyof T`. So `F<T>[K]` with `K in keyof T` must not emit TS2536.
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn deep_required_mapped_key_no_ts2536() {
    let diags = check_es5(
        "type DeepRequired<T> = T extends object ? { [P in keyof T]-?: DeepRequired<T[P]> } : T;\n\
         type Test<T> = { [K in keyof T]: DeepRequired<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "DeepRequired<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn deep_partial_mapped_key_no_ts2536() {
    let diags = check_es5(
        "type DeepPartial<T> = T extends object ? { [P in keyof T]?: DeepPartial<T[P]> } : T;\n\
         type Test<T> = { [K in keyof T]: DeepPartial<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "DeepPartial<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

#[test]
fn deep_readonly_mapped_key_no_ts2536() {
    let diags = check_es5(
        "type DeepReadonly<T> = T extends object ? { readonly [P in keyof T]: DeepReadonly<T[P]> } : T;\n\
         type Test<T> = { [K in keyof T]: DeepReadonly<T>[K] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "DeepReadonly<T>[K] where K in keyof T must not emit TS2536: {diags:?}"
    );
}

/// Renamed parameters prove the rule is structural, not keyed on identifier spelling.
#[test]
fn deep_required_renamed_param_no_ts2536() {
    let diags = check_es5(
        "type DeepReq<U> = U extends object ? { [Q in keyof U]-?: DeepReq<U[Q]> } : U;\n\
         type Test<V> = { [J in keyof V]: DeepReq<V>[J] };",
    );
    assert!(
        ts2536(&diags).is_empty(),
        "DeepReq<V>[J] with renamed params must not emit TS2536: {diags:?}"
    );
}

#[test]
fn deep_required_concrete_no_errors() {
    let diags = check_es5(
        r#"type DeepRequired<T> = T extends object ? { [P in keyof T]-?: DeepRequired<T[P]> } : T;
type Test<T> = { [K in keyof T]: DeepRequired<T>[K] };
type Obj = { a: number; b?: string };
type Result = Test<Obj>;"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Concrete DeepRequired use must not emit TS2536: {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Negative cases: conditional bodies whose branches do NOT share keyof T
// must still emit TS2536 — tsc reports these at the generic definition
// level because the solver cannot prove keyof F<T> = keyof T.
// ──────────────────────────────────────────────────────────────────────────

/// Verified against `tsc 6.0.3 --noEmit`: this exact shape emits
/// `TS2536: Type 'K' cannot be used to index type 'Fixed<T>'.`
/// The solver-side branch proof must reject this (true-branch is `{ x: number }`,
/// whose keyof is `"x"`, not `keyof T`), and the checker must not defer it.
#[test]
fn conditional_unrelated_true_branch_emits_ts2536() {
    let diags = check_es5(
        "type Fixed<T> = T extends object ? { x: number } : T;\n\
         type Test<T> = { [K in keyof T]: Fixed<T>[K] };",
    );
    assert!(
        !ts2536(&diags).is_empty(),
        "Fixed<T>[K] with unrelated true-branch must emit TS2536 (parity with tsc): {diags:?}"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Nested mapped type in type-argument position (issue #6562)
//
// Structural rule: when `{ [P in K]: T[P] }` appears inside a type argument
// where K is the iteration variable of an enclosing `[K in keyof T]` mapped
// type, the chain P → K → keyof T must be recognised and TS2536 suppressed.
// ──────────────────────────────────────────────────────────────────────────

/// Simple two-argument wrapper — both mapped-body variants must be accepted.
#[test]
fn nested_mapped_in_type_arg_simple_wrapper_no_ts2536() {
    let diags = check_es5(
        r#"type Wrap<A, B> = [A, B];
type Test<T> = {
  [K in keyof T]-?: Wrap<
    { [P in K]: T[P] },
    { -readonly [Q in K]: T[Q] }
  > extends any ? K : never;
}[keyof T];"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "{{[P in K]: T[P]}} in Wrap<> type arg must not emit TS2536: {diags:?}"
    );
}

/// Higher-order conditional (Equal pattern) — the original issue #6562 repro.
#[test]
fn nested_mapped_in_higher_order_conditional_no_ts2536() {
    let diags = check_es5(
        r#"type IsIdentical<X, Y> = (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? true : false;
type PickMutable<T> = {
  [K in keyof T]-?: IsIdentical<
    { [P in K]: T[P] },
    { -readonly [P in K]: T[P] }
  > extends true ? K : never;
}[keyof T];"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "{{[P in K]: T[P]}} in higher-order conditional must not emit TS2536: {diags:?}"
    );
}

/// Renamed iteration variables (outer J, inner X and Y) — names must not matter.
#[test]
fn nested_mapped_renamed_vars_no_ts2536() {
    let diags = check_es5(
        r#"type Pair<A, B> = { fst: A; snd: B };
type Test<T> = {
  [J in keyof T]-?: Pair<
    { [X in J]: T[X] },
    { readonly [Y in J]?: T[Y] }
  >;
};"#,
    );
    assert!(
        ts2536(&diags).is_empty(),
        "Renamed outer/inner vars must not emit TS2536: {diags:?}"
    );
}

/// Negative: inner mapped iterates over unrelated key space — TS2536 must fire.
#[test]
fn nested_mapped_unrelated_key_space_still_emits_ts2536() {
    let diags = check_es5(
        r#"interface A { x: number; }
interface B { y: string; }
type Wrap<A, B> = [A, B];
type Test<T> = {
  [K in keyof T]: Wrap<
    { [P in keyof A]: B[P] },
    K
  >;
};"#,
    );
    assert!(
        !ts2536(&diags).is_empty(),
        "B[P] where P extends keyof A but B ≠ A must still emit TS2536: {diags:?}"
    );
}

#[test]
fn create_type_options_satisfies_required_options_constraint() {
    let diags = check_es5(
        r#"
type CreateTypeOptions<
  Options extends Required<Options>,
  OverrideOptions extends Partial<Options>,
  DefaultOptions extends Required<Options>,
> = {
  [Key in keyof Options]: OverrideOptions[Key] extends Options[Key] ? OverrideOptions[Key] : DefaultOptions[Key];
};

type DefaultPathsOptions = {
  depth: 7;
  anyArrayIndexAccessor: `${number}`;
};

type PathsOptions = {
  depth: number;
  anyArrayIndexAccessor: string;
};

type UnsafePaths<Type, Options extends Required<PathsOptions>> = Type;

type Paths<Type, OverridePathOptions extends Partial<PathsOptions> = {}> = UnsafePaths<
  Type,
  CreateTypeOptions<PathsOptions, OverridePathOptions, DefaultPathsOptions>
>;

type Ok = Paths<{ value: string }>;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "CreateTypeOptions must not emit TS2344: {diags:?}"
    );
}

#[test]
fn renamed_create_type_options_binders_satisfy_required_constraint() {
    let diags = check_es5(
        r#"
type MergeOptions<
  Bag extends Required<Bag>,
  Patch extends Partial<Bag>,
  Fallback extends Required<Bag>,
> = {
  [Name in keyof Bag]: Patch[Name] extends Bag[Name] ? Patch[Name] : Fallback[Name];
};

interface Shape {
  mode: "deep";
  count: number;
}

interface ShapeDefaults {
  mode: "deep";
  count: 1;
}

type UseShape<Value, Config extends Required<Shape>> = Value;
type Ok = UseShape<string, MergeOptions<Shape, {}, ShapeDefaults>>;
"#,
    );
    assert!(
        ts2344(&diags).is_empty(),
        "renamed option binders must not affect TS2344: {diags:?}"
    );
}

#[test]
fn incompatible_create_type_options_defaults_still_emit_ts2344() {
    let diags = check_es5(
        r#"
type CreateTypeOptions<
  Options extends Required<Options>,
  OverrideOptions extends Partial<Options>,
  DefaultOptions extends Required<Options>,
> = {
  [Key in keyof Options]: OverrideOptions[Key] extends Options[Key] ? OverrideOptions[Key] : DefaultOptions[Key];
};

interface PathBag {
  depth: number;
}

interface BadDefaults {
  depth: string;
}

type UsePaths<Type, Options extends Required<PathBag>> = Type;
type Bad = UsePaths<unknown, CreateTypeOptions<PathBag, {}, BadDefaults>>;
"#,
    );
    assert!(
        !ts2344(&diags).is_empty(),
        "incompatible defaults must still emit TS2344"
    );
}

// ──────────────────────────────────────────────────────────────────────────
// Generic indexed access by a bare type parameter stays deferred (no TS2322)
//
// Structural rule: `O[K]` for a bare type parameter `K extends keyof O`
// (the object currently being indexed) is the homomorphic element type and
// must stay deferred as `O[K]`. It must NOT distribute into `O[keyof O]` (the
// union over every value type). The defect surfaced when a homomorphic mapped
// element function `(x: O[K]) => O[K]` is invoked: distributing the receiver's
// `O[K]` return into the value-type union made the call return the union and
// fired a false TS2322 against the declared `O[K]` return type.
//
// Counter-boundary: indexing by `keyof O` itself (a `KeyOf`, not a type
// parameter) must still expand to the value-type union, so
// `{ [P in keyof T]: T[P] }[keyof T]` stays a union.
// ──────────────────────────────────────────────────────────────────────────

fn ts2322(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2322).collect()
}

#[test]
fn homomorphic_reader_call_return_no_ts2322() {
    // Binder names deliberately non-canonical to avoid name fast-paths.
    let diags = check_es5(
        r#"
interface Bag { alpha: { v: number }; beta: { v: string } }
type Readers<Src> = { [Field in keyof Src]: (x: Src[Field]) => Src[Field] };
declare const readers: Readers<Bag>;
declare const model: Bag;
function readIndexed<Probe extends keyof Bag>(key: Probe): Bag[Probe] {
  return readers[key](model[key]);
}
"#,
    );
    assert!(
        ts2322(&diags).is_empty(),
        "homomorphic reader call must return Bag[Probe], not the value union: {diags:?}"
    );
}

#[test]
fn homomorphic_nested_indexed_access_call_no_ts2322() {
    let diags = check_es5(
        r#"
interface Bag { alpha: { v: number }; beta: { v: string } }
type Boxes<Src> = { [Field in keyof Src]: { box: Src[Field] } };
declare const boxes: Boxes<Bag>;
function readBox<Probe extends keyof Bag>(key: Probe): Bag[Probe] {
  return boxes[key].box;
}
"#,
    );
    assert!(
        ts2322(&diags).is_empty(),
        "nested homomorphic indexed access must stay deferred: {diags:?}"
    );
}

#[test]
fn generic_object_indexed_by_bare_type_param_defers() {
    // The object is a still-generic `Src`; `Src[Probe]` must stay deferred.
    let diags = check_es5(
        r#"
function generic<Src, Probe extends keyof Src>(
  readers: { [Field in keyof Src]: (x: Src[Field]) => Src[Field] },
  model: Src,
  key: Probe,
): Src[Probe] {
  return readers[key](model[key]);
}
"#,
    );
    assert!(
        ts2322(&diags).is_empty(),
        "generic homomorphic reader call must return Src[Probe]: {diags:?}"
    );
}

#[test]
fn indexed_values_by_keyof_stays_union() {
    // Counter-boundary: indexing by `keyof T` (NOT a bare type param) must
    // still produce the value-type union. Assigning a non-member must error.
    let diags = check_es5(
        r#"
interface Bag { alpha: { v: number }; beta: { v: string } }
type IndexedValues<Src> = { [Field in keyof Src]: Src[Field] }[keyof Src];
const ok1: IndexedValues<Bag> = { v: 1 };
const ok2: IndexedValues<Bag> = { v: "x" };
const bad: IndexedValues<Bag> = { v: true };
"#,
    );
    let errs = ts2322(&diags);
    assert_eq!(
        errs.len(),
        1,
        "IndexedValues<Bag> must remain a union: only `{{ v: true }}` should error, got: {diags:?}"
    );
}

#[test]
fn subset_constrained_type_param_resolves_member() {
    // `K extends "alpha"` resolves through the literal constraint (not deferred
    // homomorphically), so the reader element is the concrete `alpha` function.
    let diags = check_es5(
        r#"
interface Bag { alpha: { v: number }; beta: { v: string } }
type Readers<Src> = { [Field in keyof Src]: (x: Src[Field]) => Src[Field] };
declare const readers: Readers<Bag>;
declare const model: Bag;
function readAlpha<Probe extends "alpha">(key: Probe): Bag[Probe] {
  return readers[key](model[key]);
}
"#,
    );
    assert!(
        ts2322(&diags).is_empty(),
        "subset-constrained reader call must type-check: {diags:?}"
    );
}
