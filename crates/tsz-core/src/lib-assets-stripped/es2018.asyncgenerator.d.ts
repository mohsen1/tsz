/// <reference lib="es2018.asynciterable" />
interface AsyncGenerator<T = unknown, TReturn = any, TNext = any> extends AsyncIteratorObject<T, TReturn, TNext> {
    next(...[value]: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
    return(value: TReturn | PromiseLike<TReturn>): Promise<IteratorResult<T, TReturn>>;
    throw(e: any): Promise<IteratorResult<T, TReturn>>;
    [Symbol.asyncIterator](): AsyncGenerator<T, TReturn, TNext>;
}
interface AsyncGeneratorFunction {
    new (...args: any[]): AsyncGenerator;
    (...args: any[]): AsyncGenerator;
    readonly length: number;
    readonly name: string;
    readonly prototype: AsyncGenerator;
}
interface AsyncGeneratorFunctionConstructor {
    new (...args: string[]): AsyncGeneratorFunction;
    (...args: string[]): AsyncGeneratorFunction;
    readonly length: number;
    readonly name: string;
    readonly prototype: AsyncGeneratorFunction;
}
