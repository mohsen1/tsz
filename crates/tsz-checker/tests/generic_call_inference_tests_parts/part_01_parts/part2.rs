#[test]
fn contextual_generic_new_return_keeps_different_constructor_base_mismatch() {
    let source = r#"
type ParseReturnType<T> = { ok: true; value: T } | { ok: false };
interface BaseDef {}

abstract class Schema<Out, Def extends BaseDef = BaseDef, In = Out> {
    readonly _output!: Out;
    readonly _input!: In;
    readonly _def!: Def;
    abstract _parse(): ParseReturnType<Out>;
    constructor(def: Def) {}
}

type AnySchema = Schema<any, any, any>;
type Effect<T> = { type: "refinement"; refine: (arg: T) => unknown };

interface WrapperDef<S extends AnySchema = AnySchema> extends BaseDef {
    schema: S;
    marker: "wrapper";
    effect: Effect<any>;
}

interface OtherDef<S extends AnySchema = AnySchema> extends BaseDef {
    other: S;
    marker: "other";
}

class Wrapper<
    S extends AnySchema,
    Out = S["_output"],
    In = S["_input"]
> extends Schema<Out, WrapperDef<S>, In> {
    _parse(): ParseReturnType<Out> {
        return null as never;
    }
}

class Other<
    S extends AnySchema,
    Out = S["_output"],
    In = S["_input"]
> extends Schema<Out, OtherDef<S>, In> {
    _parse(): ParseReturnType<Out> {
        return null as never;
    }
}

function wrong<Source extends AnySchema>(
    schema: Source,
    effect: Effect<Source["_output"]>
): Other<Source, Source["_output"]> {
    return new Wrapper({ schema, marker: "wrapper", effect });
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "contextual recovery must not hide a different constructor application base. Got: {diags:#?}"
    );
}

#[test]
fn conflicting_contextual_instantiation_keeps_enclosing_return_type_param() {
    let source = r#"
declare function accept<R>(fn: (a: string, b: number) => R): R;

function outer<X>(source: <T>(a: T, b: T) => X) {
    const out = accept(source);
    const keep: X = out;
}
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|(_code, message)| message.contains("unknown")),
        "contextual conflict handling must not rewrite enclosing return type parameters to unknown. Got: {diags:#?}"
    );
}

#[test]
fn generic_callback_parameter_does_not_override_concrete_array_inference() {
    let source = r#"
export function keyOf<a>(value: { key: a; }): a {
    return value.key;
}
declare class Date {}
export interface Data {
    key: number;
    value: Date;
}

var data: Data[] = [];
declare function toKeys<a>(values: a[], toKey: (value: a) => string): string[];

toKeys(data, keyOf);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        !diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("Data[]")),
        "the concrete array argument should own `a`; the callback should be checked against `(value: Data) => string`. Got: {diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|(code, message)| *code == 2345 && message.contains("(value: Data) => string")),
        "generic callback return mismatch should be reported at the callback parameter. Got: {diags:#?}"
    );
}

#[test]
fn contextual_parameter_self_referential_no_excess_constraint_no_false_ts2345() {
    let source = r#"
type NoExcessProperties<T, U> = T & {
  readonly [K in Exclude<keyof U, keyof T>]: never;
};

interface Effect<out A> {
  readonly EffectTypeId: {
    readonly _A: (_: never) => A;
  };
}

declare function pipe<A, B>(a: A, ab: (a: A) => B): B;

interface RepeatOptions<A> {
  until?: (_: A) => boolean;
}

declare const repeat: {
  <O extends NoExcessProperties<RepeatOptions<A>, O>, A>(
    options: O,
  ): (self: Effect<A>) => Effect<A>;
};

pipe(
  {} as Effect<boolean>,
  repeat({
    until: (x) => {
      return x;
    },
  }),
);
"#;
    let diags = relevant_lib_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "self-referential NoExcessProperties constraint should not raise false TS2345. Got: {diags:#?}"
    );
}

#[test]
fn conformance_probe_nested_generic_spread_inference() {
    let source = r#"
declare function wrap<X>(x: X): { x: X };
declare function call<A extends unknown[], T>(x: { x: (...args: A) => T }, ...args: A): T;

const leak = call(wrap(<T>(x: T) => x), 1);
"#;
    let diags = relevant_strict_diagnostics(source);
    assert!(
        lacks_diagnostic_code(&diags, 2345),
        "nested generic spread inference should not produce TS2345. Got: {diags:#?}"
    );
}
