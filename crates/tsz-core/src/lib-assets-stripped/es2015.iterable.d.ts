/// <reference lib="es2015.symbol" />
interface SymbolConstructor {
    readonly iterator: unique symbol;
}
interface IteratorYieldResult<TYield> {
    done?: false;
    value: TYield;
}
interface IteratorReturnResult<TReturn> {
    done: true;
    value: TReturn;
}
type IteratorResult<T, TReturn = any> = IteratorYieldResult<T> | IteratorReturnResult<TReturn>;
interface Iterator<T, TReturn = any, TNext = any> {
    next(...[value]: [] | [TNext]): IteratorResult<T, TReturn>;
    return?(value?: TReturn): IteratorResult<T, TReturn>;
    throw?(e?: any): IteratorResult<T, TReturn>;
}
interface Iterable<T, TReturn = any, TNext = any> {
    [Symbol.iterator](): Iterator<T, TReturn, TNext>;
}
interface IterableIterator<T, TReturn = any, TNext = any> extends Iterator<T, TReturn, TNext> {
    [Symbol.iterator](): IterableIterator<T, TReturn, TNext>;
}
interface IteratorObject<T, TReturn = unknown, TNext = unknown> extends Iterator<T, TReturn, TNext> {
    [Symbol.iterator](): IteratorObject<T, TReturn, TNext>;
}
type BuiltinIteratorReturn = intrinsic;
interface ArrayIterator<T> extends IteratorObject<T, BuiltinIteratorReturn, unknown> {
    [Symbol.iterator](): ArrayIterator<T>;
}
interface Array<T> {
    [Symbol.iterator](): ArrayIterator<T>;
    entries(): ArrayIterator<[number, T]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<T>;
}
interface ArrayConstructor {
    from<T>(iterable: Iterable<T> | ArrayLike<T>): T[];
    from<T, U>(iterable: Iterable<T> | ArrayLike<T>, mapfn: (v: T, k: number) => U, thisArg?: any): U[];
}
interface ReadonlyArray<T> {
    [Symbol.iterator](): ArrayIterator<T>;
    entries(): ArrayIterator<[number, T]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<T>;
}
interface IArguments {
    [Symbol.iterator](): ArrayIterator<any>;
}
interface MapIterator<T> extends IteratorObject<T, BuiltinIteratorReturn, unknown> {
    [Symbol.iterator](): MapIterator<T>;
}
interface Map<K, V> {
    [Symbol.iterator](): MapIterator<[K, V]>;
    entries(): MapIterator<[K, V]>;
    keys(): MapIterator<K>;
    values(): MapIterator<V>;
}
interface ReadonlyMap<K, V> {
    [Symbol.iterator](): MapIterator<[K, V]>;
    entries(): MapIterator<[K, V]>;
    keys(): MapIterator<K>;
    values(): MapIterator<V>;
}
interface MapConstructor {
    new (): Map<any, any>;
    new <K, V>(iterable?: Iterable<readonly [K, V]> | null): Map<K, V>;
}
interface WeakMap<K extends WeakKey, V> {}
interface WeakMapConstructor {
    new <K extends WeakKey, V>(iterable: Iterable<readonly [K, V]>): WeakMap<K, V>;
}
interface SetIterator<T> extends IteratorObject<T, BuiltinIteratorReturn, unknown> {
    [Symbol.iterator](): SetIterator<T>;
}
interface Set<T> {
    [Symbol.iterator](): SetIterator<T>;
    entries(): SetIterator<[T, T]>;
    keys(): SetIterator<T>;
    values(): SetIterator<T>;
}
interface ReadonlySet<T> {
    [Symbol.iterator](): SetIterator<T>;
    entries(): SetIterator<[T, T]>;
    keys(): SetIterator<T>;
    values(): SetIterator<T>;
}
interface SetConstructor {
    new <T>(iterable?: Iterable<T> | null): Set<T>;
}
interface WeakSet<T extends WeakKey> {}
interface WeakSetConstructor {
    new <T extends WeakKey = WeakKey>(iterable: Iterable<T>): WeakSet<T>;
}
interface Promise<T> {}
interface PromiseConstructor {
    all<T>(values: Iterable<T | PromiseLike<T>>): Promise<Awaited<T>[]>;
    race<T>(values: Iterable<T | PromiseLike<T>>): Promise<Awaited<T>>;
}
interface StringIterator<T> extends IteratorObject<T, BuiltinIteratorReturn, unknown> {
    [Symbol.iterator](): StringIterator<T>;
}
interface String {
    [Symbol.iterator](): StringIterator<string>;
}
interface Int8Array<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Int8ArrayConstructor {
    new (elements: Iterable<number>): Int8Array<ArrayBuffer>;
    from(elements: Iterable<number>): Int8Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Int8Array<ArrayBuffer>;
}
interface Uint8Array<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Uint8ArrayConstructor {
    new (elements: Iterable<number>): Uint8Array<ArrayBuffer>;
    from(elements: Iterable<number>): Uint8Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Uint8Array<ArrayBuffer>;
}
interface Uint8ClampedArray<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Uint8ClampedArrayConstructor {
    new (elements: Iterable<number>): Uint8ClampedArray<ArrayBuffer>;
    from(elements: Iterable<number>): Uint8ClampedArray<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Uint8ClampedArray<ArrayBuffer>;
}
interface Int16Array<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Int16ArrayConstructor {
    new (elements: Iterable<number>): Int16Array<ArrayBuffer>;
    from(elements: Iterable<number>): Int16Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Int16Array<ArrayBuffer>;
}
interface Uint16Array<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Uint16ArrayConstructor {
    new (elements: Iterable<number>): Uint16Array<ArrayBuffer>;
    from(elements: Iterable<number>): Uint16Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Uint16Array<ArrayBuffer>;
}
interface Int32Array<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Int32ArrayConstructor {
    new (elements: Iterable<number>): Int32Array<ArrayBuffer>;
    from(elements: Iterable<number>): Int32Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Int32Array<ArrayBuffer>;
}
interface Uint32Array<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Uint32ArrayConstructor {
    new (elements: Iterable<number>): Uint32Array<ArrayBuffer>;
    from(elements: Iterable<number>): Uint32Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Uint32Array<ArrayBuffer>;
}
interface Float32Array<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Float32ArrayConstructor {
    new (elements: Iterable<number>): Float32Array<ArrayBuffer>;
    from(elements: Iterable<number>): Float32Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Float32Array<ArrayBuffer>;
}
interface Float64Array<TArrayBuffer extends ArrayBufferLike> {
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
}
interface Float64ArrayConstructor {
    new (elements: Iterable<number>): Float64Array<ArrayBuffer>;
    from(elements: Iterable<number>): Float64Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Float64Array<ArrayBuffer>;
}
