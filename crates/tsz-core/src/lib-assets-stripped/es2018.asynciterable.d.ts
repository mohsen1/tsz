/// <reference lib="es2015.symbol" />
/// <reference lib="es2015.iterable" />
interface SymbolConstructor {
    readonly asyncIterator: unique symbol;
}
interface AsyncIterator<T, TReturn = any, TNext = any> {
    next(...[value]: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
    return?(value?: TReturn | PromiseLike<TReturn>): Promise<IteratorResult<T, TReturn>>;
    throw?(e?: any): Promise<IteratorResult<T, TReturn>>;
}
interface AsyncIterable<T, TReturn = any, TNext = any> {
    [Symbol.asyncIterator](): AsyncIterator<T, TReturn, TNext>;
}
interface AsyncIterableIterator<T, TReturn = any, TNext = any> extends AsyncIterator<T, TReturn, TNext> {
    [Symbol.asyncIterator](): AsyncIterableIterator<T, TReturn, TNext>;
}
interface AsyncIteratorObject<T, TReturn = unknown, TNext = unknown> extends AsyncIterator<T, TReturn, TNext> {
    [Symbol.asyncIterator](): AsyncIteratorObject<T, TReturn, TNext>;
}
