//! Regression tests for recursive conditional tuple reconstruction followed by
//! homomorphic mapped-type indexing.
//!
//! Structural rule: when a recursive conditional reconstructs a finite
//! variadic tuple and a homomorphic mapped type later indexes a fixed numeric
//! slot, `tsc` preserves that slot's concrete element type. tsz owns this in
//! solver tuple/mapped evaluation; the checker should not report a missing
//! property from a union of the wrong tuple slots.

use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn recursive_tuple_then_homomorphic_map_preserves_fixed_slot() {
    let source = r#"
interface Leaf {
    id: string;
    flags: { labels: readonly ["fast", "safe"] };
}
type NormalizeBox<Input> =
    Input extends object ? { [Field in keyof Input]: NormalizeBox<Input[Field]> } : Input;
type DeepTuple<Subject> =
    Subject extends readonly [infer First, ...infer Rest]
        ? readonly [DeepTuple<First>, ...DeepTuple<Rest>]
        : Subject;
type Source = { readonly value: Leaf; readonly next: { readonly value: Leaf } };
type Tuple = NormalizeBox<DeepTuple<readonly [Source, Leaf, { wrapped: Source }]>>;
declare const tuple: Tuple;
const value: string = tuple[2].wrapped.next.value.flags.labels[0];
"#;

    let diagnostics = check_source_code_messages(source);
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "recursive tuple mapped access must keep slot 2 as the object with `wrapped`. Got: {diagnostics:#?}"
    );
}

#[test]
fn recursive_tuple_then_homomorphic_map_preserves_fixed_slot_renamed() {
    let source = r#"
interface Node {
    name: string;
    meta: { tags: readonly ["hot", "path"] };
}
type WalkBox<Item> =
    Item extends object ? { [Part in keyof Item]: WalkBox<Item[Part]> } : Item;
type RebuildList<Entry> =
    Entry extends readonly [infer Head, ...infer Tail]
        ? readonly [RebuildList<Head>, ...RebuildList<Tail>]
        : Entry;
type Branch = { readonly node: Node; readonly child: { readonly node: Node } };
type Result = WalkBox<RebuildList<readonly [Branch, Node, { focus: Branch }]>>;
declare const result: Result;
const tag: string = result[2].focus.child.node.meta.tags[1];
"#;

    let diagnostics = check_source_code_messages(source);
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "renamed recursive tuple mapped access must keep slot 2 as the object with `focus`. Got: {diagnostics:#?}"
    );
}

#[test]
fn recursive_alias_pipeline_preserves_mapped_tuple_fixed_slot() {
    let source = r#"
type AliasCompute<Value> = Value extends (...args: infer Params) => infer Result
    ? (...args: Params) => Result
    : Value extends readonly [infer Head, ...infer Tail]
        ? readonly [AliasCompute<Head>, ...AliasCompute<Tail>]
        : Value extends object
            ? { [Key in keyof Value]: AliasCompute<Value[Key]> }
            : Value;

type NormalizeBox<Input> = Input extends object
    ? { [Field in keyof Input]: NormalizeBox<Input[Field]> }
    : Input;

type DeepReadonlyVariant<Subject> = Subject extends (...args: any[]) => any
    ? Subject
    : Subject extends readonly [infer First, ...infer Rest]
        ? readonly [DeepReadonlyVariant<First>, ...DeepReadonlyVariant<Rest>]
        : Subject extends object
            ? { readonly [Name in keyof Subject]: DeepReadonlyVariant<Subject[Name]> }
            : Subject;

type PickStringKeys<RecordLike> = {
    [Member in keyof RecordLike as Member extends string ? Member : never]: RecordLike[Member]
};

type UtilityPipeline<Seed> = AliasCompute<
    NormalizeBox<
        DeepReadonlyVariant<
            PickStringKeys<Seed>
        >
    >
>;

interface LeafPayload {
    flags: { labels: readonly ["fast", "safe"] };
}

type Variant<Source> = UtilityPipeline<{
    readonly item: Source;
    readonly nested: {
        readonly tuple: readonly [Source, LeafPayload, { wrapped: Source }];
    };
}>;

type Materialized = Variant<{
    readonly next: {
        readonly value: LeafPayload;
    };
}>;

declare const materialized: Materialized;
const value: string = materialized.nested.tuple[2].wrapped.next.value.flags.labels[0];
"#;

    let diagnostics = check_source_code_messages(source);
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "recursive alias pipeline must keep slot 2 as the object with `wrapped`. Got: {diagnostics:#?}"
    );
}
