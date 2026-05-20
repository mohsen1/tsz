/// <reference lib="es2015.iterable" />
interface Generator<T = unknown, TReturn = any, TNext = any> extends IteratorObject<T, TReturn, TNext> {
    next(...[value]: [] | [TNext]): IteratorResult<T, TReturn>;
    return(value: TReturn): IteratorResult<T, TReturn>;
    throw(e: any): IteratorResult<T, TReturn>;
    [Symbol.iterator](): Generator<T, TReturn, TNext>;
}
interface GeneratorFunction {
    new (...args: any[]): Generator;
    (...args: any[]): Generator;
    readonly length: number;
    readonly name: string;
    readonly prototype: Generator;
}
interface GeneratorFunctionConstructor {
    new (...args: string[]): GeneratorFunction;
    (...args: string[]): GeneratorFunction;
    readonly length: number;
    readonly name: string;
    readonly prototype: GeneratorFunction;
}
