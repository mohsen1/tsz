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

type cases = [
  Must<Same<VisibleOnly<{ c?: number }>, { c?: number }>>,
  Must<Same<VisibleOnly<{ readonly c?: number }>, { readonly c?: number }>>,
  Must<Same<VisibleOnlyRenamed<{ readonly kept?: string; _hidden: boolean }>, { readonly kept?: string }>>,
  Must<Same<PrefixVisible<{ kept?: string }>, { xkept?: string }>>,
  Must<Same<VisibleOnly<{ _hidden?: number }>, {}>>,
];
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "key-remapped mapped types should preserve declared optional source property types; got: {diagnostics:?}"
    );
}
