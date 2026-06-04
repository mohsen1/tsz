/// Downstream check: `BuildTree` recursive conditional type should terminate
/// at depth N now that `Prepend<V, T>` infers correctly for mixed
/// fixed+rest params.
///
/// Without the `match_rest_infer_tuple` fix, `Prepend<any, I>` collapsed
/// to `any` and `BuildTree` never terminated, producing a false TS2741.
/// With the fix, the unit-level Prepend behaviour above is correct and the
/// instantiated indexed-access key is deferred until the resolver can expand
/// aliases like `Length<I>`.
#[test]
fn test_build_tree_no_false_ts2741() {
    // Without the fix, Prepend evaluated to `any`, causing BuildTree never to
    // terminate and emitting TS2741 (required property `children` missing).
    let source = r#"
type Length<T extends any[]> = T["length"];
type Prepend<V, T extends any[]> = ((head: V, ...args: T) => void) extends (
  ...args: infer R
) => void
  ? R
  : any;

type BuildTree<T, N extends number = -1, I extends any[] = []> = {
  1: T;
  0: T & { children: BuildTree<T, N, Prepend<any, I>>[] };
}[Length<I> extends N ? 1 : 0];

interface User {
  name: string;
}

type GrandUser = BuildTree<User, 2>;

// A correctly-typed assignment — depth-2 tree has no `children` requirement
// at depth 2, so the object literal should be valid.
const grandUser: GrandUser = {
  name: "Grand User",
  children: [
    { name: "Son", children: [{ name: "Grandson" }] }
  ]
};
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2741),
        "Must NOT emit TS2741 — BuildTree must terminate at depth 2 without false property-missing errors, got: {codes:?}"
    );
}

#[test]
fn test_build_tree_terminal_property_receiver_displays_evaluated_leaf_type() {
    let element_source = r#"
type Length<T extends any[]> = T["length"];
type Prepend<V, T extends any[]> = ((head: V, ...args: T) => void) extends (
  ...args: infer R
) => void
  ? R
  : any;

type BuildTree<T, N extends number = -1, I extends any[] = []> = {
  1: T;
  0: T & { children: BuildTree<T, N, Prepend<any, I>>[] };
}[Length<I> extends N ? 1 : 0];

interface User {
  name: string;
}

type GrandUser = BuildTree<User, 2>;
declare const grandUser: GrandUser;
grandUser.children[0].children[0].children[0];
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(element_source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(ts2339.len(), 1, "Expected one TS2339, got: {diagnostics:?}");

    let message = &ts2339[0].message_text;
    assert!(
        message.contains("type 'User'"),
        "terminal recursive conditional receiver should display the evaluated leaf type, got: {message:?}"
    );
    assert!(
        !message.contains("BuildTree<"),
        "property receiver display should not preserve the recursive helper alias at the terminal leaf, got: {message:?}"
    );

    let renamed_element_source = r#"
type Length<T extends any[]> = T["length"];
type PushFront<V, T extends any[]> = ((head: V, ...args: T) => void) extends (
  ...args: infer R
) => void
  ? R
  : any;

type TreeAt<T, N extends number = -1, I extends any[] = []> = {
  1: T;
  0: T & { kids: TreeAt<T, N, PushFront<any, I>>[] };
}[Length<I> extends N ? 1 : 0];

interface Person {
  id: string;
}

type Family = TreeAt<Person, 1>;
declare const family: Family;
family.kids[0].kids[0];
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(renamed_element_source);
    let ts2339: Vec<_> = diagnostics.iter().filter(|d| d.code == 2339).collect();
    assert_eq!(ts2339.len(), 1, "Expected one TS2339, got: {diagnostics:?}");

    let message = &ts2339[0].message_text;
    assert!(
        message.contains("type 'Person'"),
        "renamed terminal recursive conditional receiver should display the evaluated leaf type, got: {message:?}"
    );
    assert!(
        !message.contains("TreeAt<"),
        "renamed property receiver display should not preserve the recursive helper alias at the terminal leaf, got: {message:?}"
    );
}

#[test]
fn test_conditional_key_selects_depth_terminal_branch() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type PickDepth<T, N extends number, I extends any[]> = {
  1: T;
  0: T & { children: any[] };
}[Length<I> extends N ? 1 : 0];

interface User {
  name: string;
}

type Depth2 = PickDepth<User, 2, [any, any]>;
const user: Depth2 = { name: "Grandson" };
"#;
    let codes = tsz_checker::test_utils::check_source_codes(source);
    assert!(
        !codes.contains(&2741),
        "Concrete depth selector must choose terminal branch without children, got: {codes:?}"
    );
}

#[test]
fn test_tuple_length_conditional_with_numeric_literal() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type IsTwo = Length<[any, any]> extends 2 ? "yes" : "no";
const value: IsTwo = "yes";
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Tuple length conditional should resolve to true branch, got: {diagnostics:?}"
    );
}

#[test]
fn test_object_index_with_tuple_length_conditional_key() {
    let source = r#"
type Length<T extends any[]> = T["length"];
type Selected = {
  1: "terminal";
  0: { children: any[] };
}[Length<[any, any]> extends 2 ? 1 : 0];
const value: Selected = "terminal";
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Object index should use evaluated conditional key, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_object_index_with_numeric_literal_key() {
    let source = r#"
type Selected<T> = {
  1: T;
  0: T & { children: any[] };
}[1];

interface User {
  name: string;
}

type Depth2 = Selected<User>;
const user: Depth2 = { name: "Grandson" };
"#;
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Generic object index should select numeric literal key, got: {diagnostics:?}"
    );
}
