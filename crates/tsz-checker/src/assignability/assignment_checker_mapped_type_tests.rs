use super::strict_diagnostics_for;

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
    let diagnostics = strict_diagnostics_for(
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
        diag.message_text.contains("Property '[E.B]' is missing"),
        "single missing key from a string-enum mapped type renders as `[E.B]`, got: {diag:?}"
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
        diag.message_text.contains("Property '[E.B]' is missing"),
        "numeric-enum mapped key renders as `[E.B]`, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Property '1' is missing"),
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
        diag.message_text
            .contains("Property '[Color.Green]' is missing"),
        "renamed enum still renders the member reference, got: {diag:?}"
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
