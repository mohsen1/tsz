use super::diagnostics_for;

#[test]
fn tuple_with_homomorphic_passthrough_over_union_no_error() {
    let diagnostics = diagnostics_for(
        r#"
type PassThrough<U> = { [P in keyof U]: U[P] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
declare const c: [PassThrough<NodeA | NodeB>];
const d: [NodeA | NodeB] = c;
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "Expected no TS2322 for [PassThrough<NodeA|NodeB>] assigned to [NodeA|NodeB], got: {diagnostics:?}"
    );
}

#[test]
fn tuple_with_homomorphic_passthrough_different_type_param_name_no_error() {
    let diagnostics = diagnostics_for(
        r#"
type Id<K> = { [Q in keyof K]: K[Q] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
declare const c: [Id<NodeA | NodeB>];
const d: [NodeA | NodeB] = c;
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "Expected no TS2322 for [Id<NodeA|NodeB>] (renamed type param K) assigned to [NodeA|NodeB], got: {diagnostics:?}"
    );
}

#[test]
fn tuple_with_passthrough_direct_still_no_error() {
    let diagnostics = diagnostics_for(
        r#"
type PassThrough<U> = { [P in keyof U]: U[P] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
declare const a: PassThrough<NodeA | NodeB>;
const b: NodeA | NodeB = a;
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "Expected no TS2322 for direct PassThrough<NodeA|NodeB> assigned to NodeA|NodeB, got: {diagnostics:?}"
    );
}

#[test]
fn tuple_with_multi_element_passthrough_no_error() {
    let diagnostics = diagnostics_for(
        r#"
type PassThrough<U> = { [P in keyof U]: U[P] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
declare const e: [PassThrough<NodeA | NodeB>, string];
const f: [NodeA | NodeB, string] = e;
"#,
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "Expected no TS2322 for multi-element tuple [PassThrough<NodeA|NodeB>, string], got: {diagnostics:?}"
    );
}

#[test]
fn tuple_structural_mismatch_still_fails() {
    let diagnostics = diagnostics_for(
        r#"
type PassThrough<U> = { [P in keyof U]: U[P] };
type NodeA = { name: string; id: number };
type NodeB = { name: boolean; id: number };
type DifferentShape = { x: string; y: number };
declare const c: [PassThrough<NodeA | NodeB>];
const bad: [DifferentShape] = c;
"#,
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "Expected TS2322 for [PassThrough<NodeA|NodeB>] assigned to [DifferentShape], got: {diagnostics:?}"
    );
}

#[test]
fn callable_source_satisfies_callable_union_across_assignment_entrypoints() {
    let diagnostics = diagnostics_for(
        r#"
type CallableNode<TProps> = (props: TProps) => string | number;
type ConstructNode<TProps> = new (props: TProps) => { render(): string };
type Elementish<TProps> = CallableNode<TProps> | ConstructNode<TProps>;
type Props = { label: string };

const Component = (props: Props) => 1;
const initialized: Elementish<Props> = Component;

let assigned!: Elementish<Props>;
assigned = Component;

declare function accepts(value: Elementish<Props>): void;
accepts(Component);
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != 2322 && diag.code != 2345),
        "Expected callable source to satisfy callable union arm through TS2322 and TS2345 paths, got: {diagnostics:?}"
    );
}

#[test]
fn callable_source_does_not_satisfy_callable_union_arm_with_static_requirements() {
    let diagnostics = diagnostics_for(
        r#"
type CallableWithStatic<TProps> = {
    (props: TProps): number;
    required: string;
};
type ConstructNode<TProps> = new (props: TProps) => { render(): string };
type Elementish<TProps> = CallableWithStatic<TProps> | ConstructNode<TProps>;
type Props = { label: string };

const Component = (props: Props) => 1;
const initialized: Elementish<Props> = Component;

declare function accepts(value: Elementish<Props>): void;
accepts(Component);
"#,
    );

    assert!(
        diagnostics.iter().any(|diag| diag.code == 2322),
        "Expected TS2322 when callable union arm has static requirements, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 2345),
        "Expected TS2345 when callable union arm has static requirements, got: {diagnostics:?}"
    );
}
