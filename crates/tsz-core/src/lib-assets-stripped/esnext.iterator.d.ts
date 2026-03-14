/// <reference lib="es2015.iterable" />
export {};
declare abstract class Iterator<T, TResult = undefined, TNext = unknown> { // eslint-disable-line @typescript-eslint/no-unsafe-declaration-merging
    abstract next(value?: TNext): IteratorResult<T, TResult>;
}
interface Iterator<T, TResult, TNext> extends globalThis.IteratorObject<T, TResult, TNext> {}
type IteratorObjectConstructor = typeof Iterator;
declare global {
    interface IteratorObject<T, TReturn, TNext> {
        [Symbol.iterator](): IteratorObject<T, TReturn, TNext>;
        map<U>(callbackfn: (value: T, index: number) => U): IteratorObject<U, undefined, unknown>;
        filter<S extends T>(predicate: (value: T, index: number) => value is S): IteratorObject<S, undefined, unknown>;
        filter(predicate: (value: T, index: number) => unknown): IteratorObject<T, undefined, unknown>;
        take(limit: number): IteratorObject<T, undefined, unknown>;
        drop(count: number): IteratorObject<T, undefined, unknown>;
        flatMap<U>(callback: (value: T, index: number) => Iterator<U, unknown, undefined> | Iterable<U, unknown, undefined>): IteratorObject<U, undefined, unknown>;
        reduce(callbackfn: (previousValue: T, currentValue: T, currentIndex: number) => T): T;
        reduce(callbackfn: (previousValue: T, currentValue: T, currentIndex: number) => T, initialValue: T): T;
        reduce<U>(callbackfn: (previousValue: U, currentValue: T, currentIndex: number) => U, initialValue: U): U;
        toArray(): T[];
        forEach(callbackfn: (value: T, index: number) => void): void;
        some(predicate: (value: T, index: number) => unknown): boolean;
        every(predicate: (value: T, index: number) => unknown): boolean;
        find<S extends T>(predicate: (value: T, index: number) => value is S): S | undefined;
        find(predicate: (value: T, index: number) => unknown): T | undefined;
        readonly [Symbol.toStringTag]: string;
    }
    interface IteratorConstructor extends IteratorObjectConstructor {
        from<T>(value: Iterator<T, unknown, undefined> | Iterable<T, unknown, undefined>): IteratorObject<T, undefined, unknown>;
    }
    var Iterator: IteratorConstructor;
}
