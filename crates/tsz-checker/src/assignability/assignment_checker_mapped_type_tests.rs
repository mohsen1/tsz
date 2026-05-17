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
