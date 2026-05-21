// Regression test for stack overflow in match_infer_pattern when
// a type has self-referential callable return types forming a recursive chain.
//
// Structural rule: when infer-pattern matching recurses through callable return
// types that form a cycle (e.g. `optional(): ZodOptional<this>`), the outer
// visited set must be propagated so the cycle is detected rather than producing
// an infinite recursion.
//
// Three equivalent shapes are tested:
//  1. Zod-like: ZodType<T> with optional(): ZodOptional<ZodType<T>>
//  2. Builder pattern: Chain<T> with wrap(): Wrapped<Chain<T>>
//  3. Codec pattern: Codec<T> with nullable(): Nullable<Codec<T>>

export {};

// Shape 1: Zod-like optional chaining
interface ZodType<Output, Def = unknown> {
    optional(): ZodOptional<this>;
    _output: Output;
}
interface ZodOptional<T extends ZodType<unknown>> extends ZodType<T["_output"] | undefined> {
    unwrap(): T;
}
interface ZodString extends ZodType<string> {
    min(n: number): this;
}

type Output<T extends ZodType<unknown>> = T extends ZodType<infer O, unknown> ? O : never;

declare const zStr: ZodString;
const zOpt = zStr.optional();
type ZodStringOutput = Output<ZodString>;
const _check1: ZodStringOutput = "hello";
type ZodOptStringOutput = Output<typeof zOpt>;
const _check2: string | undefined = null as unknown as ZodOptStringOutput;

// Shape 2: Builder/wrapper pattern
interface Chain<T> {
    wrap(): Wrapped<this>;
    value: T;
}
interface Wrapped<T extends Chain<unknown>> extends Chain<T["value"]> {
    inner(): T;
}
interface NumberChain extends Chain<number> {
    inc(): this;
}

type Unwrap<T extends Chain<unknown>> = T extends Chain<infer V> ? V : never;

declare const numChain: NumberChain;
const wrapped = numChain.wrap();
type NumberChainValue = Unwrap<NumberChain>;
const _check3: NumberChainValue = 42;
type WrappedValue = Unwrap<typeof wrapped>;
const _check4: number = null as unknown as WrappedValue;

// Shape 3: Codec/nullable pattern
interface Codec<A> {
    nullable(): NullableCodec<this>;
    decode(input: unknown): A;
}
interface NullableCodec<C extends Codec<unknown>> extends Codec<ReturnType<C["decode"]> | null> {
    base(): C;
}
interface StringCodec extends Codec<string> {
    trim(): this;
}

type Decoded<C extends Codec<unknown>> =
    C extends Codec<infer A> ? A : never;

declare const strCodec: StringCodec;
const nullableStr = strCodec.nullable();
type StringDecoded = Decoded<StringCodec>;
const _check5: StringDecoded = "test";
type NullableStringDecoded = Decoded<typeof nullableStr>;
const _check6: string | null = null as unknown as NullableStringDecoded;
