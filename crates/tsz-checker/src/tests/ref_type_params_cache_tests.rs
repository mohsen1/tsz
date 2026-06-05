use crate::context::CheckerOptions;
use crate::test_utils::{
    check_computed_type_argument_resolution_counts, check_multi_file_with_global_index,
    check_source_diagnostics,
};

/// When multiple type references in the same alias body refer to the same
/// generic utility type, tsz must resolve that type's parameter list once per
/// symbol (not once per reference site).  These tests verify correctness and
/// that arity validation still fires when argument counts are wrong.

#[test]
fn alias_body_explicit_type_refs_use_validator_only_path() {
    let checker_source = std::fs::read_to_string("src/types/type_checking/type_alias_checking.rs")
        .expect("read type alias checker");
    assert!(
        checker_source.contains("check_explicit_type_reference_for_alias_body_validation"),
        "alias body validation should validate explicit type reference arguments \
         without forcing full type-reference lowering"
    );
}

#[test]
fn composed_generic_alias_body_can_defer_semantic_construction() {
    let checker_source =
        std::fs::read_to_string("src/types/type_checking/type_alias_missing_name_coverage.rs")
            .expect("read type alias missing-name coverage");
    assert!(
        checker_source.contains("type_ref.type_arguments.as_ref().is_none_or")
            && checker_source.contains("syntax_kind_ext::TEMPLATE_LITERAL_TYPE")
            && checker_source.contains("syntax_kind_ext::TYPE_OPERATOR"),
        "generic alias bodies composed from type references, template literals, \
         and type operators should be eligible for lazy semantic construction"
    );

    let diags = check_source_diagnostics(
        r#"
type ObjectOf<List> = { [Key in keyof List]: List[Key] };
type PickObject<Obj, Key extends keyof Obj> = { [P in Key]: Obj[P] };
type ListOf<Obj> = Obj[keyof Obj][];

type PickList<List, Key extends string | number | symbol> =
    ListOf<PickObject<ObjectOf<List>, `${Key & number}` | Key>>;
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2304 | 2314 | 2315 | 2344 | 2558))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "composed generic alias bodies must still validate names and explicit \
         type arguments without eager semantic construction; got: {relevant:#?}"
    );
}

#[test]
fn type_parameter_predicate_skips_intrinsic_cache_lookup() {
    let checker_source =
        std::fs::read_to_string("src/types/queries/core.rs").expect("read type queries");
    assert!(
        checker_source.contains("if type_id.is_intrinsic()")
            && checker_source.contains("return false;"),
        "intrinsic types cannot contain type parameters and should bypass the \
         hot predicate cache map"
    );
}

#[test]
fn property_only_type_literal_alias_body_missing_names_covered_by_validation() {
    let checker_source =
        std::fs::read_to_string("src/types/type_checking/type_alias_missing_name_coverage.rs")
            .expect("read type alias missing-name coverage");
    assert!(
        checker_source.contains("syntax_kind_ext::TYPE_LITERAL")
            && checker_source.contains("get_property_decl")
            && checker_source.contains("prop.type_annotation.is_some()"),
        "property-only type literal alias bodies should be covered by the \
         validation walk without broadening to signatures or unannotated members"
    );
}

#[test]
fn tuple_alias_body_missing_names_covered_by_validation() {
    let checker_source =
        std::fs::read_to_string("src/types/type_checking/type_alias_missing_name_coverage.rs")
            .expect("read type alias missing-name coverage");
    assert!(
        checker_source.contains("syntax_kind_ext::TUPLE_TYPE")
            && checker_source.contains("get_tuple_type")
            && checker_source.contains("syntax_kind_ext::NAMED_TUPLE_MEMBER")
            && checker_source.contains("get_named_tuple_member"),
        "tuple and named-tuple alias bodies should be covered by the validation \
         walk without falling back to a second missing-name traversal"
    );
}

#[test]
fn reference_composite_alias_body_keeps_missing_name_diagnostics() {
    let diags = check_source_diagnostics(
        r#"
type Known<Value> = { value: Value };
type Combined<Item> = Known<Item> | Missing<Item>;
"#,
    );

    let missing_names: Vec<_> = diags.iter().filter(|d| d.code == 2304).collect();
    assert_eq!(
        missing_names.len(),
        1,
        "Reference/composite-only alias bodies should still report unresolved \
         type names through check_type_node; got: {diags:#?}"
    );
}

#[test]
fn mapped_alias_body_keeps_conservative_missing_name_path() {
    let checker_source =
        std::fs::read_to_string("src/types/type_checking/type_alias_missing_name_coverage.rs")
            .expect("read type alias missing-name coverage");
    assert!(
        checker_source.contains("type_alias_body_missing_names_covered_inner")
            && checker_source.contains("syntax_kind_ext::MAPPED_TYPE")
            && checker_source.contains("_ => false"),
        "Mapped alias bodies must keep the resolving coverage path because \
         mapped parameters and template diagnostics need scoped handling"
    );
}

#[test]
fn type_node_validation_cache_is_context_keyed() {
    let checker_source = std::fs::read_to_string("src/types/type_checking/type_alias_checking.rs")
        .expect("read type alias checker");
    assert!(
        checker_source.contains("active_resolving_alias_set_key")
            && checker_source.contains("type_reference_arg_validation_scope_key")
            && checker_source.contains("type_node_validation"),
        "type-node validation success caching must be keyed by lexical scope \
         and active alias-resolution context"
    );
}

#[test]
fn type_node_validation_cache_only_records_clean_walks() {
    let checker_source = std::fs::read_to_string("src/types/type_checking/type_alias_checking.rs")
        .expect("read type alias checker");
    assert!(
        checker_source.contains("let diagnostics_before = self.ctx.diagnostics.len();")
            && checker_source.contains("if self.ctx.diagnostics.len() == diagnostics_before")
            && checker_source.contains(".type_node_validation")
            && checker_source.contains(".insert(validation_cache_key)"),
        "type-node validation success caching must not record diagnostic-bearing walks"
    );
}

#[test]
fn direct_conditional_true_branch_validation_skips_check_type_resolution() {
    let checker_source = std::fs::read_to_string("src/types/type_checking/type_alias_checking.rs")
        .expect("read type alias checker");
    let direct_branch = checker_source
        .find("if true_needs_direct_type_ref_validation")
        .expect("direct true-branch validation path");
    let mapped_branch = checker_source[direct_branch..]
        .find("} else if true_is_mapped")
        .map(|offset| direct_branch + offset)
        .expect("mapped true-branch validation path");
    let direct_branch_source = &checker_source[direct_branch..mapped_branch];
    assert!(
        direct_branch_source.contains("push_infer_bindings_from_extends")
            && direct_branch_source.contains("check_child_type_node_in_current_scope")
            && !direct_branch_source.contains("get_type_from_type_node(cond.check_type)"),
        "Direct conditional true-branch type-reference validation should only push \
         infer bindings and validate the branch; resolving the check type eagerly \
         evaluates recursive aliases such as ts-toolbelt Drop/Flatten."
    );
}

#[test]
fn boolean_dispatch_index_alias_uses_declared_key_subset() {
    let diags = check_source_diagnostics(
        r#"
type Boolean = 0 | 1;
type Or<B1 extends Boolean, B2 extends Boolean> = {
    0: {
        0: 0;
        1: 1;
    };
    1: {
        0: 1;
        1: 1;
    };
}[B1][B2];
type Dispatch<A extends Boolean, B extends Boolean> = {
    0: string;
    1: number;
}[Or<A, B>];

type Good = Dispatch<0, 1>;
type BadBoolean = 0 | 2;
type Bad = {
    0: string;
    1: number;
}[BadBoolean];
"#,
    );

    let indexed_access_errors: Vec<_> = diags.iter().filter(|d| d.code == 2536).collect();
    assert_eq!(
        indexed_access_errors.len(),
        1,
        "Boolean dispatch aliases should validate when their declared result \
         is covered by the dispatch table, while uncovered literal aliases \
         still report TS2536; got: {diags:#?}"
    );

    let checker_source = std::fs::read_to_string("src/types/type_checking/indexed_access.rs")
        .expect("read indexed access checker");
    let fast_path = checker_source
        .find("type_literal_dispatch_index_is_declared_key_subset")
        .expect("dispatch index fast path");
    let index_resolution = checker_source
        .find("let index_type = self.get_type_from_type_node(data.index_type);")
        .expect("indexed access index resolution");
    assert!(
        fast_path < index_resolution,
        "Dispatch-table index validation must run before resolving the index type"
    );
}

#[test]
fn renamed_tuple_dispatch_alias_still_validates_annotations() {
    let diags = check_source_diagnostics(
        r#"
type Boxed<Item> = { value: Item };
type Walk<Thing, Cursor> = [
    current: Boxed<Thing>,
    next: Walk<Thing, Cursor>
];
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2304 | 2314 | 2315 | 2344 | 2558))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Renamed tuple dispatch aliases should validate without name-sensitive \
         behavior; got: {relevant:#?}"
    );
}

#[test]
fn renamed_property_only_dispatch_alias_still_validates_annotations() {
    let diags = check_source_diagnostics(
        r#"
type Boxed<Item> = { value: Item };
type Walk<Thing, Cursor> = {
    zero: Boxed<Thing>;
    one: Walk<Thing, Cursor>;
};
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2304 | 2314 | 2315 | 2344 | 2558))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Renamed property-only dispatch aliases should validate without \
         name-sensitive behavior; got: {relevant:#?}"
    );
}

#[test]
fn signature_type_literal_alias_keeps_missing_name_path() {
    let diags = check_source_diagnostics(
        r#"
type Callable<T> = {
    <U>(input: MissingInput<T>): MissingOutput<U>;
};
"#,
    );

    let missing_names: Vec<_> = diags.iter().filter(|d| d.code == 2304).collect();
    assert_eq!(
        missing_names.len(),
        2,
        "Type literals with signatures must keep the existing missing-name \
         validation path; got: {diags:#?}"
    );
}

#[test]
fn type_reference_body_reuses_resolved_type_arguments() {
    let counts = check_computed_type_argument_resolution_counts(
        r#"
declare const key: unique symbol;
type Box<T> = { value: T };
type Use = Box<{ [key](): string }>;
declare const value: Use;
"#,
    );

    assert_eq!(
        counts,
        vec![1],
        "computed type-literal arguments should be resolved once and reused; got {counts:?}"
    );
}

#[test]
fn multiple_refs_to_same_two_param_utility_no_spurious_errors() {
    let diags = check_source_diagnostics(
        r#"
type Without<T, U> = { [P in Exclude<keyof T, keyof U>]?: never };
type Exclusive<T, U> = (T & Without<U, T>) | (U & Without<T, U>);
type Exclude<T, U> = T extends U ? never : T;

type A = { a: number; shared: string };
type B = { b: number; shared: string };

declare const x: Exclusive<A, B>;
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2344 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Multiple references to Without should not produce spurious errors; got: {relevant:#?}"
    );
}

#[test]
fn multiple_refs_renamed_params_k_v_produce_same_arity_errors() {
    let diags = check_source_diagnostics(
        r#"
type Pair<K, V> = { key: K; value: V };

type Combined = Pair<string, number> | Pair<number, string>;
type BadArity = Pair<string>;
"#,
    );

    let arity_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2314 || d.code == 2558)
        .collect();
    assert_eq!(
        arity_errors.len(),
        1,
        "Expected exactly one arity error for Pair<string> (missing second arg); got: {arity_errors:#?}"
    );
}

#[test]
fn repeated_refs_to_three_param_utility_all_valid() {
    let diags = check_source_diagnostics(
        r#"
type Triple<A, B, C> = { first: A; second: B; third: C };

type T1 = Triple<string, number, boolean>;
type T2 = Triple<number, boolean, string>;
type T3 = Triple<boolean, string, number>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2344 | 2345 | 2558))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Three valid instantiations of Triple should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn alias_wrapping_same_generic_shares_params() {
    let diags = check_source_diagnostics(
        r#"
type Box<T> = { value: T };

type StringBox = Box<string>;
type NumberBox = Box<number>;
type BadBox = Box<string, number>;
"#,
    );

    let arity_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2314 || d.code == 2558)
        .collect();
    assert_eq!(
        arity_errors.len(),
        1,
        "Expected one arity error for Box<string, number>; got: {arity_errors:#?}"
    );
}

#[test]
fn nested_generic_refs_same_utility_no_spurious_errors() {
    let diags = check_source_diagnostics(
        r#"
type Nullable<T> = T | null;
type Optional<T> = T | undefined;
type Maybe<T> = Nullable<Optional<T>>;
type MaybeString = Maybe<string>;
type MaybeNumber = Maybe<number>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2344 | 2345))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Nested alias chains reusing the same generic should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn default_type_param_instantiations_produce_no_errors() {
    let diags = check_source_diagnostics(
        r#"
type Container<T, U = string> = { item: T; meta: U };

type C1 = Container<number>;
type C2 = Container<number, boolean>;
type C3 = Container<string>;
type C4 = Container<boolean, null>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2344 | 2558))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Four valid Container instantiations should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn constrained_type_param_validated_consistently_across_refs() {
    let diags = check_source_diagnostics(
        r#"
type StringMap<K extends string, V> = { [key in K]: V };

type M1 = StringMap<"a" | "b", number>;
type M2 = StringMap<string, boolean>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2344 | 2314 | 2558))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Valid instantiations of constrained StringMap should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn partial_indexed_access_keeps_lib_alias_body_state() {
    let diags = check_source_diagnostics(
        r#"
type Foo = {
    x: number;
    y: string;
};

function getValueConcrete<K extends keyof Foo>(
    o: Partial<Foo>,
    k: K
): Foo[K] | undefined {
    return o[k];
}
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2536))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Indexed access through Partial<T> should preserve lib alias body state; got: {errors:#?}"
    );
}

#[test]
fn many_refs_to_same_two_param_type_all_valid() {
    let diags = check_source_diagnostics(
        r#"
type Either<L, R> = { tag: "left"; value: L } | { tag: "right"; value: R };

type E01 = Either<string, number>;
type E02 = Either<number, string>;
type E03 = Either<boolean, string>;
type E04 = Either<string, boolean>;
type E05 = Either<null, string>;
type E06 = Either<string, null>;
type E07 = Either<undefined, number>;
type E08 = Either<number, undefined>;
type E09 = Either<E01, E02>;
type E10 = Either<E03, E04>;
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2344 | 2558))
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Ten valid instantiations of Either should produce no errors; got: {errors:#?}"
    );
}

#[test]
fn imported_ref_type_params_cache_is_keyed_by_target_file() {
    let diags = check_multi_file_with_global_index(
        &[
            (
                "sources/List/one.ts",
                r#"
export type Key = string | number | symbol;
export type List<A = any> = readonly A[];
export type One<T, Path extends List<Key>, M extends any = any> = {
    readonly one: T;
    readonly path: Path;
    readonly match: M;
};
"#,
            ),
            (
                "sources/Object/two.ts",
                r#"
export type Key = string | number | symbol;
export type Two<K extends Key, V extends any = unknown> = {
    readonly key: K;
    readonly value: V;
};
"#,
            ),
            (
                "sources/List/main.ts",
                r#"
import { Key, List, One as Pathish } from "./one";
import { Two as Pairish } from "../Object/two";

type A = Pathish<object, List<Key>, string>;
type B = Pairish<string, number>;
"#,
            ),
            (
                "sources/Object/HasPath.ts",
                r#"
export type HasPath<O extends object, Path, M extends any = any, match extends string = 'default'> = {
    readonly object: O;
    readonly path: Path;
    readonly match: M;
    readonly mode: match;
};
"#,
            ),
            (
                "sources/Any/Compute.ts",
                r#"
export type ComputeRaw<A extends any> = A;
"#,
            ),
            (
                "sources/List/toolbelt.ts",
                r#"
import { HasPath as OHasPath } from "../Object/HasPath";
import { ComputeRaw } from "../Any/Compute";

type C = OHasPath<object, readonly ['a'], string, 'default'>;
type D = ComputeRaw<{ readonly a: string }>;
"#,
            ),
        ],
        "sources/List/toolbelt.ts",
        CheckerOptions::default(),
    );

    let arity_errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2558))
        .collect();
    assert_eq!(
        arity_errors.len(),
        0,
        "Imported generic aliases with colliding raw ids should keep distinct arities; got: {diags:#?}"
    );
}

#[test]
fn imported_ref_type_params_follow_barrel_reexports_to_decl_file() {
    let diags = check_multi_file_with_global_index(
        &[
            (
                "src/generic.ts",
                r#"
export type Boxed<T, U = unknown> = {
    readonly value: T;
    readonly extra: U;
};
"#,
            ),
            (
                "src/namedBarrel.ts",
                r#"export { Boxed } from "./generic";"#,
            ),
            ("src/starBarrel.ts", r#"export * from "./generic";"#),
            (
                "src/main.ts",
                r#"
import { Boxed as NamedBoxed } from "./namedBarrel";
import { Boxed as StarBoxed } from "./starBarrel";

type A = NamedBoxed<string>;
type B = StarBoxed<number, boolean>;
"#,
            ),
        ],
        "src/main.ts",
        CheckerOptions::default(),
    );

    let arity_errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2558))
        .collect();
    assert_eq!(
        arity_errors.len(),
        0,
        "Imported generic aliases should follow barrel re-exports to the declaration file; got: {diags:#?}"
    );
}

#[test]
fn local_ref_type_params_ignore_cross_file_raw_symbol_owner_collision() {
    let diags = check_multi_file_with_global_index(
        &[
            (
                "sources/remote.ts",
                r#"
export type Remote<A, B> = { first: A; second: B };
"#,
            ),
            (
                "sources/main.ts",
                r#"
type Local<T> = { value: T };
type UseLocal = Local<string>;
"#,
            ),
        ],
        "sources/main.ts",
        CheckerOptions::default(),
    );

    let type_arg_errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2314 | 2315 | 2558))
        .collect();
    assert_eq!(
        type_arg_errors.len(),
        0,
        "Current-file generic aliases must not read type parameters from a colliding cross-file raw SymbolId; got: {diags:#?}"
    );
}
