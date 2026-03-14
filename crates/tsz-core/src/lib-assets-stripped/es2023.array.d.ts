interface Array<T> {
    findLast<S extends T>(predicate: (value: T, index: number, array: T[]) => value is S, thisArg?: any): S | undefined;
    findLast(predicate: (value: T, index: number, array: T[]) => unknown, thisArg?: any): T | undefined;
    findLastIndex(predicate: (value: T, index: number, array: T[]) => unknown, thisArg?: any): number;
    toReversed(): T[];
    toSorted(compareFn?: (a: T, b: T) => number): T[];
    toSpliced(start: number, deleteCount: number, ...items: T[]): T[];
    toSpliced(start: number, deleteCount?: number): T[];
    with(index: number, value: T): T[];
}
interface ReadonlyArray<T> {
    findLast<S extends T>(
        predicate: (value: T, index: number, array: readonly T[]) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (value: T, index: number, array: readonly T[]) => unknown,
        thisArg?: any,
    ): T | undefined;
    findLastIndex(
        predicate: (value: T, index: number, array: readonly T[]) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): T[];
    toSorted(compareFn?: (a: T, b: T) => number): T[];
    toSpliced(start: number, deleteCount: number, ...items: T[]): T[];
    toSpliced(start: number, deleteCount?: number): T[];
    with(index: number, value: T): T[];
}
interface Int8Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (value: number, index: number, array: this) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (value: number, index: number, array: this) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Int8Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Int8Array<ArrayBuffer>;
    with(index: number, value: number): Int8Array<ArrayBuffer>;
}
interface Uint8Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (value: number, index: number, array: this) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (value: number, index: number, array: this) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Uint8Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Uint8Array<ArrayBuffer>;
    with(index: number, value: number): Uint8Array<ArrayBuffer>;
}
interface Uint8ClampedArray<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Uint8ClampedArray<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Uint8ClampedArray<ArrayBuffer>;
    with(index: number, value: number): Uint8ClampedArray<ArrayBuffer>;
}
interface Int16Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (value: number, index: number, array: this) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (value: number, index: number, array: this) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Int16Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Int16Array<ArrayBuffer>;
    with(index: number, value: number): Int16Array<ArrayBuffer>;
}
interface Uint16Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Uint16Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Uint16Array<ArrayBuffer>;
    with(index: number, value: number): Uint16Array<ArrayBuffer>;
}
interface Int32Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (value: number, index: number, array: this) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (value: number, index: number, array: this) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Int32Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Int32Array<ArrayBuffer>;
    with(index: number, value: number): Int32Array<ArrayBuffer>;
}
interface Uint32Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Uint32Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Uint32Array<ArrayBuffer>;
    with(index: number, value: number): Uint32Array<ArrayBuffer>;
}
interface Float32Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Float32Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Float32Array<ArrayBuffer>;
    with(index: number, value: number): Float32Array<ArrayBuffer>;
}
interface Float64Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends number>(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number | undefined;
    findLastIndex(
        predicate: (
            value: number,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): Float64Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Float64Array<ArrayBuffer>;
    with(index: number, value: number): Float64Array<ArrayBuffer>;
}
interface BigInt64Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends bigint>(
        predicate: (
            value: bigint,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (
            value: bigint,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): bigint | undefined;
    findLastIndex(
        predicate: (
            value: bigint,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): BigInt64Array<ArrayBuffer>;
    toSorted(compareFn?: (a: bigint, b: bigint) => number): BigInt64Array<ArrayBuffer>;
    with(index: number, value: bigint): BigInt64Array<ArrayBuffer>;
}
interface BigUint64Array<TArrayBuffer extends ArrayBufferLike> {
    findLast<S extends bigint>(
        predicate: (
            value: bigint,
            index: number,
            array: this,
        ) => value is S,
        thisArg?: any,
    ): S | undefined;
    findLast(
        predicate: (
            value: bigint,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): bigint | undefined;
    findLastIndex(
        predicate: (
            value: bigint,
            index: number,
            array: this,
        ) => unknown,
        thisArg?: any,
    ): number;
    toReversed(): BigUint64Array<ArrayBuffer>;
    toSorted(compareFn?: (a: bigint, b: bigint) => number): BigUint64Array<ArrayBuffer>;
    with(index: number, value: bigint): BigUint64Array<ArrayBuffer>;
}
