//! Constructor intersections follow overload-set semantics unless a
//! constituent is a true mixin constructor.
//!
//! Ordinary constructor constituents append their signatures in source order;
//! direct literal annotations then participate in the shared specialized-first
//! ordering. A true mixin has exactly one non-generic `...args: any[]`
//! construct signature, which is removed as a candidate and has its instance
//! type intersected into the selected ordinary return.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

fn compile_source(source: &str) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("main.ts"), source).expect("write repro file");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022",
        "main.ts",
    ])
    .expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

#[test]
fn constructor_intersections_select_ordered_overloads_and_fold_only_mixins() {
    let diagnostics = compile_source(
        r#"
type Same<X, Y> =
    (<T>() => T extends X ? 1 : 2) extends
    (<T>() => T extends Y ? 1 : 2)
        ? true
        : false;
type Assert<T extends true> = T;

interface FirstResult<Payload> { owner: "first"; payload: Payload }
interface SecondResult<Payload> { owner: "second"; payload: Payload }
interface FirstCtor<Item> {
    new (value: string): FirstResult<Item>;
}
interface SecondCtor<Element> {
    new (value: string): SecondResult<Element>;
}
type OrdinaryPair<Value> = FirstCtor<Value> & SecondCtor<Value>;
declare const ordinary: OrdinaryPair<number>;
const ordinaryResult = new ordinary("value");
type OrdinaryKeepsFirstOwner =
    Assert<Same<typeof ordinaryResult, FirstResult<number>>>;

type SemanticAlias = "pick";
interface AliasCtor {
    new (value: SemanticAlias): FirstResult<"alias">;
}
interface DirectLiteralCtor {
    new (value: "pick"): SecondResult<"literal">;
}
declare const specialized: AliasCtor & DirectLiteralCtor;
const specializedResult = new specialized("pick");
type DirectLiteralMovesAcrossOwners =
    Assert<Same<typeof specializedResult, SecondResult<"literal">>>;

interface MixinResult { mixed: true }
interface ConcreteResult { concrete: true }
type TrueMixin = new (...renamed: any[]) => MixinResult;
type ConcreteCtor = new (value: string) => ConcreteResult;
declare const mixed: TrueMixin & ConcreteCtor;
const mixedResult = new mixed("value");
type MixinReturnIsFolded =
    Assert<Same<typeof mixedResult, MixinResult & ConcreteResult>>;

type AliasedMixinArgs = any[];
declare const aliasedMixin:
    (new (...aliasedArgs: AliasedMixinArgs) => MixinResult) &
    ConcreteCtor;
const aliasedMixinResult = new aliasedMixin("value");
type AliasedMixinReturnIsFolded =
    Assert<Same<typeof aliasedMixinResult, MixinResult & ConcreteResult>>;

type GenericMixinArgs<Element> = Element[];
declare const genericAliasedMixin:
    (new (...genericAliasedArgs: GenericMixinArgs<any>) => MixinResult) &
    ConcreteCtor;
const genericAliasedMixinResult = new genericAliasedMixin("value");
type GenericAliasedMixinReturnIsFolded =
    Assert<Same<typeof genericAliasedMixinResult, MixinResult & ConcreteResult>>;

type ConditionalMixinArgs<Element> =
    Element extends string ? any[] : never;
declare const conditionalAliasedMixin:
    (new (...conditionalArgs: ConditionalMixinArgs<string>) => MixinResult) &
    ConcreteCtor;
const conditionalAliasedMixinResult = new conditionalAliasedMixin("value");
type ConditionalAliasedMixinReturnIsFolded =
    Assert<Same<
        typeof conditionalAliasedMixinResult,
        MixinResult & ConcreteResult
    >>;

type IndexedMixinArgs<Key extends "args"> = { args: any[] }[Key];
declare const indexedAliasedMixin:
    (new (...indexedArgs: IndexedMixinArgs<"args">) => MixinResult) &
    ConcreteCtor;
const indexedAliasedMixinResult = new indexedAliasedMixin("value");
type IndexedAliasedMixinReturnIsFolded =
    Assert<Same<
        typeof indexedAliasedMixinResult,
        MixinResult & ConcreteResult
    >>;

type MappedMixinArgs<Value> = { [Key in keyof Value]: Value[Key] };
declare const mappedAliasedMixin:
    (new (...mappedArgs: MappedMixinArgs<any[]>) => MixinResult) &
    ConcreteCtor;
const mappedAliasedMixinResult = new mappedAliasedMixin("value");
type MappedAliasedMixinReturnIsFolded =
    Assert<Same<
        typeof mappedAliasedMixinResult,
        MixinResult & ConcreteResult
    >>;

declare const queryMixinArgs: any[];
type QueryMixinArgs = typeof queryMixinArgs;
declare const queryAliasedMixin:
    (new (...queryArgs: QueryMixinArgs) => MixinResult) &
    ConcreteCtor;
const queryAliasedMixinResult = new queryAliasedMixin("value");
type QueryAliasedMixinReturnIsFolded =
    Assert<Same<
        typeof queryAliasedMixinResult,
        MixinResult & ConcreteResult
    >>;

interface OtherMixinResult { other: true }
declare const allMixins:
    (new (...leftArgs: any[]) => MixinResult) &
    (new (...rightArgs: any[]) => OtherMixinResult);
const allMixinResult = new allMixins();
type AllMixinReturnsAreFolded =
    Assert<Same<typeof allMixinResult, MixinResult & OtherMixinResult>>;

function constrainedRestIsOrdinary<Spread extends any[]>(
    ctor:
        (new (...spread: Spread) => MixinResult) &
        ConcreteCtor,
) {
    const result = new ctor("value");
    type ConstrainedRestDoesNotFold =
        Assert<Same<typeof result, ConcreteResult>>;
}

interface TupleRestResult { tuple: true }
declare const tupleRest:
    (new (...tupleArgs: [unknown]) => TupleRestResult) &
    ConcreteCtor;
const tupleRestResult = new tupleRest("value");
type TupleRestStaysOrdinary =
    Assert<Same<typeof tupleRestResult, TupleRestResult>>;

interface NoInferRestResult { noInfer: true }
declare const noInferRest:
    (new (...args: NoInfer<any[]>) => NoInferRestResult) &
    ConcreteCtor;
const noInferRestResult = new noInferRest("value");
type NoInferRestStaysOrdinary =
    Assert<Same<typeof noInferRestResult, NoInferRestResult>>;

type AliasedNoInferArgs = NoInfer<any[]>;
interface AliasedNoInferRestResult { aliasedNoInfer: true }
declare const aliasedNoInferRest:
    (new (...args: AliasedNoInferArgs) => AliasedNoInferRestResult) &
    ConcreteCtor;
const aliasedNoInferRestResult = new aliasedNoInferRest("value");
type AliasedNoInferRestStaysOrdinary =
    Assert<
        Same<
            typeof aliasedNoInferRestResult,
            AliasedNoInferRestResult
        >
    >;

type TextConstrainedCtor =
    new <T extends string>() => ConcreteResult;
type NumberConstrainedCtor =
    new <T extends number>() => ConcreteResult;
declare const constrainedGeneric:
    TextConstrainedCtor & NumberConstrainedCtor;
const constrainedGenericResult: ConcreteResult =
    new constrainedGeneric<number>();

interface CallbackStringResult { callback: "string" }
interface CallbackNumberResult { callback: "number" }
type CallbackStringCtor =
    new (callback: (value: string) => string) => CallbackStringResult;
type CallbackNumberCtor =
    new (callback: (value: number) => number) => CallbackNumberResult;
declare const contextualCtor: CallbackStringCtor & CallbackNumberCtor;
const contextualResult =
    new contextualCtor(value => value.toUpperCase());
const contextualSelection: CallbackStringResult = contextualResult;

type CallbackMixinArgs<Element> = Element[];
declare const contextualMixedCtor:
    (new (...args: CallbackMixinArgs<any>) => MixinResult) &
    CallbackStringCtor;
const contextualMixedResult =
    new contextualMixedCtor(value => value.toUpperCase());
type ContextualMixinAliasReturnIsFolded =
    Assert<
        Same<
            typeof contextualMixedResult,
            MixinResult & CallbackStringResult
        >
    >;

new ordinary(true);
"#,
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2345],
        "ordinary intersections must select one ordered overload, direct literal syntax must reorder across owners, true mixins alone fold returns, and the inapplicable fallback must remain an error; got: {diagnostics:?}"
    );
}

#[test]
fn constructor_intersection_effective_signature_identity_matches_tsc() {
    let diagnostics = compile_source(
        r#"
interface ConstructResult { constructed: true }
type TupleRestConstructor =
    new (...args: [string, number]) => ConstructResult;
type PositionalConstructor =
    new (text: string, count: number) => ConstructResult;
declare const Constructor:
    TupleRestConstructor & PositionalConstructor;

new Constructor(true, 1);

type RequiredVoidConstructor =
    new (value: void | undefined) => ConstructResult;
type OptionalVoidConstructor =
    new (value?: void) => ConstructResult;
declare const VoidConstructor:
    RequiredVoidConstructor & OptionalVoidConstructor;

new VoidConstructor(1);
"#,
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2345, 2345],
        "fixed tuple-rest spelling and trailing-void minimum arity both use tsc's effective constructor identity, so each failure uses the single-signature TS2345 path; got: {diagnostics:?}"
    );
}

#[test]
fn constructor_intersection_alias_identity_uses_checker_resolver() {
    let diagnostics = compile_source(
        r#"
interface ConstructResult { constructed: true }
type PositionalConstructor =
    new (text: string, count: number) => ConstructResult;

declare const DirectNoInferConstructor:
    (new (...args: NoInfer<[string, number]>) => ConstructResult) &
    PositionalConstructor;
new DirectNoInferConstructor(true, 1);

type TupleAlias = [string, number];
declare const AliasConstructor:
    (new (...args: TupleAlias) => ConstructResult) &
    PositionalConstructor;
new AliasConstructor(true, 1);

type RenamedTupleAlias = TupleAlias;
declare const RenamedAliasConstructor:
    (new (...args: RenamedTupleAlias) => ConstructResult) &
    PositionalConstructor;
new RenamedAliasConstructor(true, 1);

type ReadonlyTupleAlias = readonly [string, number];
declare const ReadonlyAliasConstructor:
    (new (...args: ReadonlyTupleAlias) => ConstructResult) &
    PositionalConstructor;
new ReadonlyAliasConstructor(true, 1);

type NoInferTupleAlias = NoInfer<[string, number]>;
declare const NoInferAliasConstructor:
    (new (...args: NoInferTupleAlias) => ConstructResult) &
    PositionalConstructor;
new NoInferAliasConstructor(true, 1);

type NoInferArrayAlias = NoInfer<string[]>;
declare const NoInferArrayConstructor:
    (new (...args: NoInferArrayAlias) => ConstructResult) &
    (new (...args: string[]) => ConstructResult);
new NoInferArrayConstructor(true);

type NestedNoInferArrayAlias = NoInfer<NoInfer<string[]>>;
declare const NestedNoInferArrayConstructor:
    (new (...args: NestedNoInferArrayAlias) => ConstructResult) &
    (new (...args: string[]) => ConstructResult);
new NestedNoInferArrayConstructor(true);

type WrappedStringConstructor =
    new (value: string) => ConstructResult;
type NumericValueConstructor =
    new (value: number) => ConstructResult;
declare const NoInferWrappedConstructor:
    NoInfer<WrappedStringConstructor> & NumericValueConstructor;
new NoInferWrappedConstructor("value");
new NoInferWrappedConstructor(1);
new NoInferWrappedConstructor(true);

type GenericConstructorAlias<Value> =
    new (value: Value) => ConstructResult;
declare const AppliedAliasConstructor:
    GenericConstructorAlias<string> & NumericValueConstructor;
new AppliedAliasConstructor("value");
new AppliedAliasConstructor(1);
new AppliedAliasConstructor(true);
"#,
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();

    assert_eq!(
        codes,
        vec![2769, 2345, 2345, 2345, 2769, 2345, 2345, 2769, 2769],
        "direct and aliased `NoInfer` block tuple arity exposure; direct, renamed, and readonly tuple aliases collapse through the checker resolver; `NoInfer` remains transparent to array element identity but transparent around constructor containers; generic constructor alias applications project before overload resolution; got: {diagnostics:?}"
    );
}
