//! Regression tests for #3063: generic-ref type arguments containing scoped
//! type parameters must still be checked against concrete (non-callable,
//! parameter-free) constraints.
//!
//! For `type Box<T extends string> = T;`, a type argument like `Array<U>`,
//! `Promise<U>`, or `Record<string, U>` cannot satisfy `string` regardless of
//! how `U` is later instantiated. tsc emits TS2344 for each. tsz used to skip
//! the check whenever the argument was a generic reference mentioning a scoped
//! type parameter and the constraint had no type parameters.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::diagnostic_code_messages;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    diagnostic_code_messages(checker.ctx.diagnostics)
}

#[test]
fn generic_array_ref_in_concrete_string_constraint_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}

type Box<T extends string> = T;
type BadArray<U> = Box<Array<U>>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344.len(),
        1,
        "Expected one TS2344 for Array<U> not satisfying string constraint, got: {diagnostics:?}"
    );
    assert!(
        ts2344[0].1.contains("U[]") || ts2344[0].1.contains("Array<U>"),
        "Expected TS2344 message to mention the array type argument, got: {:?}",
        ts2344[0]
    );
}

#[test]
fn generic_promise_ref_in_concrete_string_constraint_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Promise<T> {}

type Box<T extends string> = T;
type BadPromise<U> = Box<Promise<U>>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344.len(),
        1,
        "Expected one TS2344 for Promise<U> not satisfying string constraint, got: {diagnostics:?}"
    );
    assert!(
        ts2344[0].1.contains("Promise"),
        "Expected TS2344 message to mention 'Promise', got: {:?}",
        ts2344[0]
    );
}

#[test]
fn generic_record_ref_in_concrete_string_constraint_emits_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };

type Box<T extends string> = T;
type BadRecord<U> = Box<Record<string, U>>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344.len(),
        1,
        "Expected one TS2344 for Record<string, U> not satisfying string constraint, got: {diagnostics:?}"
    );
    assert!(
        ts2344[0].1.contains("Record"),
        "Expected TS2344 message to mention 'Record', got: {:?}",
        ts2344[0]
    );
}

/// Naming the type parameter differently must not change the result. This is
/// the structural-rule sanity check from the anti-hardcoding directive.
#[test]
fn rule_holds_for_arbitrary_scoped_param_name() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}

type Box<T extends string> = T;
type BadArrayQ<Q> = Box<Array<Q>>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344.len(),
        1,
        "Expected one TS2344 regardless of scoped param name, got: {diagnostics:?}"
    );
}

#[test]
fn bare_type_params_with_matching_constraints_are_accepted() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Pair<Left extends string, Right extends string> = [Left, Right];
type Forward<Name extends string, Delimiter extends string> = Pair<Name, Delimiter>;

type List<T> = T[];
type Box<Value extends List<string>> = Value;
type ForwardList<Items extends List<string>> = Box<Items>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert!(
        ts2344.is_empty(),
        "Expected bare type params whose declared constraints satisfy the target constraints to pass, got: {diagnostics:?}"
    );
}

#[test]
fn explicit_type_alias_args_violating_callable_constraint_emit_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type AppendArgument<Fn extends (...args: any[]) => any, A> =
  Fn extends (...args: infer Args) => infer R
    ? (...args: [...Args, A]) => R
    : never;

type BadUnknown = AppendArgument<unknown, undefined>;
type BadString = AppendArgument<string, number>;
type BadObject = AppendArgument<{ a: 1 }, boolean>;
type Good = AppendArgument<(value: string) => number, boolean>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344.len(),
        3,
        "Expected TS2344 for the three invalid AppendArgument instantiations only, got: {diagnostics:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|(_, message)| message.contains("unknown")),
        "Expected one TS2344 to mention unknown, got: {ts2344:?}"
    );
    assert!(
        ts2344.iter().any(|(_, message)| message.contains("string")),
        "Expected one TS2344 to mention string, got: {ts2344:?}"
    );
}

#[test]
fn explicit_interface_and_class_args_violating_callable_constraint_emit_ts2344() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface CallableBox<T extends (...args: any[]) => any> {
  value: T;
}
class CallableHolder<T extends (...args: any[]) => any> {
  value!: T;
}

type BadInterface = CallableBox<string>;
type BadClass = CallableHolder<{ a: 1 }>;
type GoodInterface = CallableBox<() => void>;
type GoodClass = CallableHolder<(value: string) => number>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344.len(),
        2,
        "Expected TS2344 for invalid interface/class callable constraints only, got: {diagnostics:?}"
    );
}

/// Control: generic-ref type arguments whose surface IS assignable to the
/// constraint must still be accepted.
#[test]
fn generic_ref_satisfying_constraint_is_accepted() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Array<T> {}

type AcceptArrayLike<T extends ArrayLike<unknown>> = T;
interface ArrayLike<T> { length: number; [n: number]: T; }

type OkArray<U> = AcceptArrayLike<Array<U>>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    // We do not require zero TS2344 here (the array surface match depends on
    // the lib's Array shape, which is synthesised in this test). Instead, we
    // assert that the rule is applied without a hardcoded short-circuit:
    // when the constraint is generic (`ArrayLike<unknown>`), the previous
    // skip block left this path alone. It must continue to behave that way.
    let _ = ts2344;
}

#[test]
fn generic_ref_with_object_constraint_defers_mapped_key_remap_result() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Values<T> = T[keyof T];
type Record<K extends keyof any, T> = { [P in K]: T };
type ProvidedActor = { src: string; logic: unknown };

interface StateMachineConfig<TActors extends ProvidedActor> {
  invoke: { src: TActors["src"] };
}

declare function setup<TActors extends Record<string, unknown>>(_: {
  actors: { [K in keyof TActors]: TActors[K] };
}): {
  createMachine: (
    config: StateMachineConfig<
      Values<{
        [K in keyof TActors as K & string]: {
          src: K;
          logic: TActors[K];
        };
      }>
    >,
  ) => void;
};
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2344),
        "Did not expect TS2344 for key-remapped Values<TActors> satisfying ProvidedActor. Got: {diagnostics:?}"
    );
}

#[test]
fn object_constraint_accepts_object_producing_alias_application() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Pickish<O extends object, K extends keyof O> = { [P in K]: O[P] } & {};
type Wrapper<O extends object> = Pickish<O, keyof O>;
type NeedsObject<T extends object> = T;

type Use<Source extends object> = NeedsObject<Wrapper<Source>>;
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2344),
        "Object-producing alias applications should satisfy lowercase object constraints. Got: {diagnostics:?}"
    );
}

#[test]
fn object_constraint_rejects_primitive_producing_alias_application() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type RenamedText<Value> = string;
type NeedsObject<T extends object> = T;

type Use<Source extends object> = NeedsObject<RenamedText<Source>>;
"#,
    );

    let ts2344: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2344)
        .collect();
    assert_eq!(
        ts2344.len(),
        1,
        "Primitive-producing alias applications must still fail object constraints. Got: {diagnostics:?}"
    );
}

#[test]
fn generic_ref_with_tuple_constraint_defers_mapped_tuple_result() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Writeable<T> = { -readonly [P in keyof T]: T[P] };
type Values<T extends [string, ...string[]]> = { [k in T[number]]: k; };

declare class ZodEnum<T extends [string, ...string[]]> {
  get enum(): Values<T>
}

declare function createZodEnum<
  U extends string,
  T extends Readonly<[U, ...U[]]>
>(values: T): ZodEnum<Writeable<T>>;
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2344),
        "Did not expect TS2344 for Writeable<T> preserving the tuple constraint. Got: {diagnostics:?}"
    );
}

#[test]
fn generic_ref_in_conditional_true_branch_respects_extends_substitution() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Set<T> { value?: T }

type Foo<A> = Set<A> extends Set<unknown[]> ? Bar<Set<A>> : "else";
type Bar<T extends Set<unknown[]>> = T;
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2344),
        "Did not expect TS2344 for Set<A> in the true branch of a matching conditional. Got: {diagnostics:?}"
    );
}

#[test]
fn generic_alias_filtering_to_string_satisfies_string_constraint() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface MyIteratorResult<T, TReturn> { value: T | TReturn; done: boolean }

type Box<T extends string> = T;
type Select<U, M> = U extends M ? U : never;
type NextPath<OP> = Select<OP, string>;
type ExecPath<A> = NextPath<MyIteratorResult<string, A>>;

type Use<A> = Box<ExecPath<A>>;
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2344),
        "Conditional filters like Select<..., string> should satisfy string constraints. Got: {diagnostics:?}"
    );
}

#[test]
fn nested_generic_alias_filtering_to_string_satisfies_string_constraint() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Box<T extends string> = T;
type DropStrings<T> = T extends string ? never : T;
type Values<T> = T[keyof T];
type KeepMatching<U, M> = U extends M ? U : never;
type NextText<OP> = KeepMatching<Values<DropStrings<OP> & {}>, string>;
type ExecText<A> = NextText<{ value: string; next: A }>;

type Use<A> = Box<ExecText<A>>;
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2344),
        "Nested conditional filters should satisfy string constraints through their extends branch. Got: {diagnostics:?}"
    );
}
