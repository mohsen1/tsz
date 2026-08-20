use super::strict_diagnostics_for;

/// Strict diagnostics with the standard lib loaded, for fixtures that
/// reference lib globals (`Symbol`, `PropertyKey`, ...). Mirrors the strict
/// option triple of [`strict_diagnostics_for`] but wires in the default lib
/// set and filters TS2318 missing-default-lib noise.
fn strict_lib_diagnostics_for(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use crate::test_utils::{check_source_with_libs, load_default_lib_files};

    let lib_files = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .collect()
}

#[test]
fn key_remapped_mapped_types_preserve_optional_declared_property_types() {
    let diagnostics = strict_diagnostics_for(
        r#"
type Same<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2)
    ? true
    : false;
type Must<T extends true> = T;

type VisibleOnly<Input extends object> = {
  [Key in keyof Input as Key extends `_${string}` ? never : Key]: Input[Key]
};

type VisibleOnlyRenamed<Shape extends object> = {
  [Prop in keyof Shape as Prop extends `_${string}` ? never : Prop]: Shape[Prop]
};

type PrefixVisible<Input extends object> = {
  [Key in keyof Input as Key extends string ? `x${Key}` : never]: Input[Key]
};

type RequiredVisible<Input extends object> = {
  [Key in keyof Input as Key extends `_${string}` ? never : Key]-?: Input[Key]
};

type cases = [
  Must<Same<VisibleOnly<{ c?: number }>, { c?: number }>>,
  Must<Same<VisibleOnly<{ readonly c?: number }>, { readonly c?: number }>>,
  Must<Same<VisibleOnlyRenamed<{ readonly kept?: string; _hidden: boolean }>, { readonly kept?: string }>>,
  Must<Same<PrefixVisible<{ kept?: string }>, { xkept?: string }>>,
  Must<Same<RequiredVisible<{ kept?: string; _hidden?: boolean }>, { kept: string }>>,
  Must<Same<VisibleOnly<{ _hidden?: number }>, {}>>,
];
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "key-remapped mapped types should preserve declared optional source property types; got: {diagnostics:?}"
    );
}

#[test]
fn tuple_to_object_preserves_unique_symbol_keys_from_tuple_index_access() {
    let diagnostics = strict_lib_diagnostics_for(
        r#"
type Same<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2)
    ? true
    : false;
type Must<T extends true> = T;

type TupleToObject<T extends readonly PropertyKey[]> = { [Key in T[number]]: Key };

const first = Symbol(1);
const second = Symbol(2);
const symbols = [first, second] as const;
const mixed = [1, "two", first] as const;

type cases = [
  Must<Same<TupleToObject<typeof symbols>, { [first]: typeof first; [second]: typeof second }>>,
  Must<Same<TupleToObject<typeof mixed>, { 1: 1; two: "two"; [first]: typeof first }>>,
];
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "tuple indexed access should preserve unique-symbol mapped keys; got: {diagnostics:?}"
    );
}

#[test]
fn replace_keys_distributes_mapped_union_for_type_challenge_equality() {
    let diagnostics = strict_diagnostics_for(
        r#"
type Same<X, Y> =
  (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2)
    ? true
    : false;
type Must<T extends true> = T;

type ReplaceKeys<U, T, Y> = {
  [P in keyof U]: P extends T
    ? P extends keyof Y
      ? Y[P]
      : never
    : U[P]
};

type NodeA = { type: 'A'; name: string; flag: number };
type NodeB = { type: 'B'; id: number; flag: number };
type NodeC = { type: 'C'; name: string; flag: number };
type Nodes = NodeA | NodeB | NodeC;

type Replaced = ReplaceKeys<Nodes, 'name' | 'flag', { name: number; flag: string }>;
type ReplaceKeysRenamed<Union, Keys, Replacements> = {
  [Field in keyof Union]: Field extends Keys
    ? Field extends keyof Replacements
      ? Replacements[Field]
      : never
    : Union[Field]
};
type Renamed = ReplaceKeysRenamed<Nodes, 'name', { name: boolean }>;

type cases = [
  Must<Same<ReplaceKeys<NodeB, 'name', { other: boolean }>, NodeB>>,
  Must<Same<ReplaceKeys<Nodes, 'name', { other: boolean }>,
    | { type: 'A'; name: never; flag: number }
    | { type: 'B'; id: number; flag: number }
    | { type: 'C'; name: never; flag: number }
  >>,
  Must<Same<Replaced,
    | { type: 'A'; name: number; flag: string }
    | { type: 'B'; id: number; flag: string }
    | { type: 'C'; name: number; flag: string }
  >>,
  Must<Same<Renamed,
    | { type: 'A'; name: boolean; flag: number }
    | { type: 'B'; id: number; flag: number }
    | { type: 'C'; name: boolean; flag: number }
  >>,
];
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "ReplaceKeys-style mapped unions should satisfy type-challenge equality; got: {diagnostics:?}"
    );
}

#[test]
fn mapped_string_enum_alias_single_missing_uses_member_display() {
    // `{ [K in E]: V }` aliased as a named type, single missing member.
    let diagnostics = super::diagnostics_for(
        r#"
enum E { A = "a", B = "b" }
type M = { [K in E]: number };
const bad: M = { a: 1 };
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741");
    assert!(
        diag.message_text.contains("Property 'b' is missing"),
        "single missing key from a string-enum mapped type renders as the bare literal `b` (tsc 7.0.2), got: {diag:?}"
    );
}

#[test]
fn mapped_numeric_enum_single_missing_uses_member_display() {
    // Numeric enum keys are erased to `"0"`/`"1"`; the display must still
    // recover the member reference rather than the numeric literal.
    let diagnostics = super::diagnostics_for(
        r#"
enum E { A, B }
type M = { [K in E]: number };
const bad: M = { 0: 1 };
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741");
    assert!(
        diag.message_text.contains("Property '1' is missing"),
        "numeric-enum mapped key renders as the bare literal `1` (tsc 7.0.2), got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Property '[E.B]' is missing"),
        "must not render the erased numeric key, got: {diag:?}"
    );
}

#[test]
fn mapped_enum_key_member_display_is_not_name_specific() {
    // Renaming the enum/members must not change the rule — proves the fix is
    // structural, not keyed on the spelling `E`/`B`.
    let diagnostics = super::diagnostics_for(
        r#"
enum Color { Red = "red", Green = "green" }
type Palette = { [K in Color]: number };
const bad: Palette = { red: 1 };
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741");
    assert!(
        diag.message_text.contains("Property 'green' is missing"),
        "renamed enum renders the bare literal key (tsc 7.0.2), got: {diag:?}"
    );
}

#[test]
fn mapped_string_union_key_single_missing_stays_bare() {
    // Negative control: a plain string-literal-union mapped type has no enum
    // origin, so tsc (and tsz) render the bare key `'b'`, not `[E.B]`.
    let diagnostics = super::diagnostics_for(
        r#"
type M = { [K in "a" | "b"]: number };
const bad: M = { a: 1 };
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741");
    assert!(
        diag.message_text.contains("Property 'b' is missing"),
        "string-union mapped key has no enum origin and stays bare, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains('['),
        "string-union key must not be bracketed, got: {diag:?}"
    );
}

#[test]
fn mapped_enum_keys_multiple_missing_use_bare_member_names() {
    // tsc brackets the key only for the single-property TS2741 message; the
    // multi-property TS2739 list uses bare member names (`b, c`).
    let diagnostics = super::diagnostics_for(
        r#"
enum E { A = "a", B = "b", C = "c" }
type M = { [K in E]: number };
const bad: M = { a: 1 };
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2739)
        .expect("expected TS2739 for multiple missing keys");
    assert!(
        diag.message_text.contains("from type 'M': b, c"),
        "multi-property list uses bare member names, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains('['),
        "multi-property list must not bracket enum members, got: {diag:?}"
    );
}

// ── Intersection-with-indexed-access source vs. mapped-type target ───────────
//
// When source is `T[K] & { a: string }` and target is a structural type (mapped,
// intersection, conditional, or string intrinsic), the checker must not suppress
// the assignability error — the solver checks property membership directly.
// Structural rule: `T[K] & { a: string } <: { [P in "a" | "b"]: string }` must
// report that "b" is missing. `tsc` reports this as the missing-property
// diagnostic (TS2741), NOT a bare generic TS2322 — an intersection source does
// not downgrade a genuine missing required property to TS2322.

/// Primary repro: indexed-access intersection against a two-key mapped type.
/// "b" is guaranteed absent from the source, so `tsc` reports it as a missing
/// property (TS2741). tsz previously suppressed the error entirely, then
/// over-corrected to a bare TS2322 for intersection sources; it must match
/// `tsc`'s TS2741.
#[test]
fn intersection_indexed_access_vs_mapped_type_emits_ts2741() {
    let diagnostics = strict_diagnostics_for(
        r#"
function test<T extends { a: string }, K extends keyof T>(x: T[K] & { a: string }): void {
    const _: { [P in "a" | "b"]: string } = x;
}
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2741)
        .unwrap_or_else(|| panic!("expected TS2741 for the missing 'b' key; got: {diagnostics:?}"));
    assert!(
        diag.message_text.contains("'b'"),
        "TS2741 should name the missing 'b' key, got: {}",
        diag.message_text
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "intersection source must not collapse to a bare TS2322; got: {diagnostics:?}"
    );
}

/// Anti-hardcoding: renamed type parameters (`U`/`I` instead of `T`/`K`).
/// Confirms the behavior is keyed on structural semantics, not parameter names.
#[test]
fn intersection_indexed_access_vs_mapped_type_renamed_params_emits_ts2741() {
    let diagnostics = strict_diagnostics_for(
        r#"
function test<U extends { a: string }, I extends keyof U>(x: U[I] & { a: string }): void {
    const _: { [Q in "a" | "b"]: string } = x;
}
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2741)
        .unwrap_or_else(|| {
            panic!("renamed-param variant should emit TS2741; got: {diagnostics:?}")
        });
    assert!(
        diag.message_text.contains("'b'"),
        "TS2741 should name the missing 'b' key, got: {}",
        diag.message_text
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "renamed-param intersection source must not collapse to a bare TS2322; got: {diagnostics:?}"
    );
}

#[test]
fn generic_record_alias_mixed_assignment_preserves_missing_property_reason() {
    let diagnostics = strict_diagnostics_for(
        r#"
type AliasRecord<Keys extends keyof any, Value> = {
    [Key in Keys]: Value;
};

type OtherRecord<Names extends keyof any, Item> = {
    [Name in Names]: Item;
};

function mixedFromAlias<Value>(
    target: OtherRecord<"a", Value>,
    source: AliasRecord<string, Value>,
) {
    target = source;
}

function mixedToAlias<Item>(
    target: AliasRecord<"a", Item>,
    source: OtherRecord<string, Item>,
) {
    target = source;
}
"#,
    );
    let missing_property_count = diagnostics
        .iter()
        .filter(|diag| diag.code == 2741 && diag.message_text.contains("'a'"))
        .count();
    assert_eq!(
        missing_property_count, 2,
        "generic mixed Record-style aliases should report missing 'a' with TS2741; got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2322),
        "missing generic Record-style alias properties must not collapse to bare TS2322; got: {diagnostics:?}"
    );
}

/// Negative control: a valid assignment must still not emit TS2322.
/// Source `T[K] & { a: string }` trivially satisfies a single-key mapped type
/// `{ [P in "a"]: string }` because "a" is present in the concrete member.
#[test]
fn intersection_indexed_access_valid_assignment_no_ts2322() {
    let diagnostics = strict_diagnostics_for(
        r#"
function test<T extends { a: string }, K extends keyof T>(x: T[K] & { a: string }): void {
    const _: { [P in "a"]: string } = x;
}
"#,
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "valid assignment to single-key mapped type must not emit TS2322; got: {diagnostics:?}"
    );
}

/// Indexed-access targets still need the generic non-nullish narrowing
/// suppression. `Partial<T>[K] & {}` is the flow-narrowed form of
/// `Partial<T>[K]`, and `tsc` accepts assigning it back to `T[K]`.
#[test]
fn intersection_indexed_access_source_to_indexed_access_target_no_ts2322() {
    let diagnostics = strict_diagnostics_for(
        r#"
function test<T, K extends keyof T>(
    target: T[K],
    narrowed: Partial<T>[K] & {},
    nullable: Partial<T>[K] & ({} | null),
): void {
    target = narrowed;
    target = nullable;
}
"#,
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "non-nullish narrowed indexed access should remain assignable to its indexed-access target; got: {diagnostics:?}"
    );
}

#[test]
fn mapped_enum_key_member_display_survives_wrapper_alias() {
    // Adjacent case: a wrapper alias (`type N = M`) over the enum mapped type
    // must still recover the `[E.B]` member reference. The wrapper resolves to
    // the same eagerly-instantiated object, so the enum origin is recovered
    // from the underlying mapped alias declaration.
    let diagnostics = super::diagnostics_for(
        r#"
enum E { A = "a", B = "b" }
type M = { [K in E]: number };
type N = M;
const bad: N = { a: 1 };
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741");
    assert!(
        diag.message_text.contains("Property 'b' is missing"),
        "wrapper alias over an enum mapped type renders the bare literal key (tsc 7.0.2), got: {diag:?}"
    );
}

#[test]
fn mapped_enum_key_member_display_for_inline_anonymous_mapped_type() {
    // Adjacent case: an inline (un-aliased) mapped type stays deferred as a
    // `Mapped` type, so the enum key constraint is recovered directly from the
    // type — the member reference must render identically to the aliased form.
    let diagnostics = super::diagnostics_for(
        r#"
enum E { A = "a", B = "b" }
const bad: { [K in E]: number } = { a: 1 };
"#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741");
    assert!(
        diag.message_text.contains("Property 'b' is missing"),
        "inline anonymous enum mapped type renders the bare literal key (tsc 7.0.2), got: {diag:?}"
    );
}

/// Fetch the single TS2741 message for a fixture, panicking with context when
/// absent. Shared by the anonymous-concrete-mapped display matrix below.
fn ts2741_message(source: &str) -> String {
    super::diagnostics_for(source)
        .iter()
        .find(|diag| diag.code == 2741)
        .unwrap_or_else(|| panic!("expected TS2741 for:\n{source}"))
        .message_text
        .clone()
}

// A concrete (type-parameter-free) mapped type with a finite key set is
// resolved by `tsc` to its member object for display — `{ [K in Color]: number }`
// prints as `{ green: number; red: number; }`, never as its `{ [K in E]: V }`
// source form. tsz keeps the `Mapped` node live for semantic identity (#15392),
// so the printer resolves it. An *aliased* mapped keeps its alias name and a
// *generic* / index-signature mapped keeps its source form; both are pinned as
// negative controls. Expectations verified against `tsc` 7.0.2
// (`--strict --target es2022 --lib es2022`).

#[test]
fn anon_concrete_enum_mapped_target_displays_resolved_member_object() {
    let msg = ts2741_message(
        r#"
enum Color { Red = "red", Green = "green" }
const bad: { [K in Color]: number } = { red: 1 };
"#,
    );
    assert!(
        msg.contains("required in type '{ green: number; red: number; }'"),
        "anonymous concrete enum mapped must display its resolved member object, got: {msg}"
    );
    assert!(
        !msg.contains("[K in"),
        "must not leak the mapped source form, got: {msg}"
    );
}

#[test]
fn anon_inline_string_union_mapped_target_displays_resolved_members() {
    let msg = ts2741_message(
        r#"
const bad: { [K in "a" | "b"]: number } = { a: 1 };
"#,
    );
    assert!(
        msg.contains("required in type '{ a: number; b: number; }'"),
        "inline string-literal-union mapped must display its resolved members, got: {msg}"
    );
}

#[test]
fn anon_aliased_union_constraint_mapped_target_displays_resolved_members() {
    // The key constraint is an alias reference (`Keys`), a `Lazy(DefId)` whose
    // members only materialize with a resolver — the display path must resolve
    // it, not fall back to the source form.
    let msg = ts2741_message(
        r#"
type Keys = "a" | "b";
const bad: { [K in Keys]: number } = { a: 1 };
"#,
    );
    assert!(
        msg.contains("required in type '{ a: number; b: number; }'"),
        "aliased-union mapped must display its resolved members, got: {msg}"
    );
}

#[test]
fn nested_anon_concrete_mapped_target_displays_resolved_members() {
    let msg = ts2741_message(
        r#"
enum Color { Red = "red", Green = "green" }
const bad: { inner: { [K in Color]: number } } = { inner: { red: 1 } };
"#,
    );
    assert!(
        msg.contains("required in type '{ green: number; red: number; }'"),
        "a nested anonymous concrete mapped must resolve for display, got: {msg}"
    );
}

#[test]
fn anon_concrete_mapped_display_is_not_name_specific() {
    // Renaming the enum, its members, and the iteration variable must not change
    // the rule — proves the resolution is structural, not keyed on a spelling.
    let msg = ts2741_message(
        r#"
enum Palette { First = "one", Second = "two" }
const bad: { [Prop in Palette]: number } = { one: 1 };
"#,
    );
    assert!(
        msg.contains("required in type '{ one: number; two: number; }'"),
        "renamed binders must still resolve the concrete mapped for display, got: {msg}"
    );
}

#[test]
fn anon_concrete_mapped_display_carries_optional_and_readonly_modifiers() {
    // The resolved member object must keep the mapped's optional modifier
    // (`?`), matching tsc's `{ a?: number | undefined; b?: number | undefined; }`.
    let msg = ts2741_message(
        r#"
declare const src: { a: number };
const bad: { readonly [K in "a" | "b"]-?: number } = src;
"#,
    );
    assert!(
        msg.contains("required in type '{ readonly a: number; readonly b: number; }'"),
        "modifiers must survive the resolved-member display, got: {msg}"
    );
}

#[test]
fn aliased_concrete_mapped_target_keeps_alias_name() {
    // Negative control: an aliased mapped keeps its alias surface (#15392),
    // never the expanded members — the resolution only fires for anonymous ones.
    let msg = ts2741_message(
        r#"
enum Color { Red = "red", Green = "green" }
type Palette = { [K in Color]: number };
const bad: Palette = { red: 1 };
"#,
    );
    assert!(
        msg.contains("required in type 'Palette'"),
        "an aliased mapped must keep its alias name, got: {msg}"
    );
    assert!(
        !msg.contains("green: number"),
        "an aliased mapped must not expand to members, got: {msg}"
    );
}

#[test]
fn anon_string_constraint_mapped_target_keeps_index_signature_form() {
    // Negative control: a `string`-constrained mapped is an index signature in
    // both printers (`{ [x: string]: string; }`), never a member object. It has
    // no required member, so the mismatch surfaces as TS2322 rather than TS2741.
    let msg = super::diagnostics_for(
        r#"
declare const src: { a: number };
const bad: { [K in string]: string } = src;
"#,
    )
    .iter()
    .find(|diag| diag.code == 2322)
    .expect("expected TS2322")
    .message_text
    .clone();
    assert!(
        msg.contains("{ [x: string]: string; }"),
        "a string-constrained mapped must keep its index-signature form, got: {msg}"
    );
}
