/// <reference lib="es2015.symbol" />
/// <reference lib="es2015.iterable" />
interface Float16Array<TArrayBuffer extends ArrayBufferLike = ArrayBufferLike> {
    readonly BYTES_PER_ELEMENT: number;
    readonly buffer: TArrayBuffer;
    readonly byteLength: number;
    readonly byteOffset: number;
    at(index: number): number | undefined;
    copyWithin(target: number, start: number, end?: number): this;
    every(predicate: (value: number, index: number, array: this) => unknown, thisArg?: any): boolean;
    fill(value: number, start?: number, end?: number): this;
    filter(predicate: (value: number, index: number, array: this) => any, thisArg?: any): Float16Array<ArrayBuffer>;
    find(predicate: (value: number, index: number, obj: this) => boolean, thisArg?: any): number | undefined;
    findIndex(predicate: (value: number, index: number, obj: this) => boolean, thisArg?: any): number;
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
    forEach(callbackfn: (value: number, index: number, array: this) => void, thisArg?: any): void;
    includes(searchElement: number, fromIndex?: number): boolean;
    indexOf(searchElement: number, fromIndex?: number): number;
    join(separator?: string): string;
    lastIndexOf(searchElement: number, fromIndex?: number): number;
    readonly length: number;
    map(callbackfn: (value: number, index: number, array: this) => number, thisArg?: any): Float16Array<ArrayBuffer>;
    reduce(callbackfn: (previousValue: number, currentValue: number, currentIndex: number, array: this) => number): number;
    reduce(callbackfn: (previousValue: number, currentValue: number, currentIndex: number, array: this) => number, initialValue: number): number;
    reduce<U>(callbackfn: (previousValue: U, currentValue: number, currentIndex: number, array: this) => U, initialValue: U): U;
    reduceRight(callbackfn: (previousValue: number, currentValue: number, currentIndex: number, array: this) => number): number;
    reduceRight(callbackfn: (previousValue: number, currentValue: number, currentIndex: number, array: this) => number, initialValue: number): number;
    reduceRight<U>(callbackfn: (previousValue: U, currentValue: number, currentIndex: number, array: this) => U, initialValue: U): U;
    reverse(): this;
    set(array: ArrayLike<number>, offset?: number): void;
    slice(start?: number, end?: number): Float16Array<ArrayBuffer>;
    some(predicate: (value: number, index: number, array: this) => unknown, thisArg?: any): boolean;
    sort(compareFn?: (a: number, b: number) => number): this;
    subarray(begin?: number, end?: number): Float16Array<TArrayBuffer>;
    toLocaleString(locales?: string | string[], options?: Intl.NumberFormatOptions): string;
    toReversed(): Float16Array<ArrayBuffer>;
    toSorted(compareFn?: (a: number, b: number) => number): Float16Array<ArrayBuffer>;
    toString(): string;
    valueOf(): this;
    with(index: number, value: number): Float16Array<ArrayBuffer>;
    [index: number]: number;
    [Symbol.iterator](): ArrayIterator<number>;
    entries(): ArrayIterator<[number, number]>;
    keys(): ArrayIterator<number>;
    values(): ArrayIterator<number>;
    readonly [Symbol.toStringTag]: "Float16Array";
}
interface Float16ArrayConstructor {
    readonly prototype: Float16Array<ArrayBufferLike>;
    new (length?: number): Float16Array<ArrayBuffer>;
    new (array: ArrayLike<number> | Iterable<number>): Float16Array<ArrayBuffer>;
    new <TArrayBuffer extends ArrayBufferLike = ArrayBuffer>(buffer: TArrayBuffer, byteOffset?: number, length?: number): Float16Array<TArrayBuffer>;
    new (buffer: ArrayBuffer, byteOffset?: number, length?: number): Float16Array<ArrayBuffer>;
    new (array: ArrayLike<number> | ArrayBuffer): Float16Array<ArrayBuffer>;
    readonly BYTES_PER_ELEMENT: number;
    of(...items: number[]): Float16Array<ArrayBuffer>;
    from(arrayLike: ArrayLike<number>): Float16Array<ArrayBuffer>;
    from<T>(arrayLike: ArrayLike<T>, mapfn: (v: T, k: number) => number, thisArg?: any): Float16Array<ArrayBuffer>;
    from(elements: Iterable<number>): Float16Array<ArrayBuffer>;
    from<T>(elements: Iterable<T>, mapfn?: (v: T, k: number) => number, thisArg?: any): Float16Array<ArrayBuffer>;
}
declare var Float16Array: Float16ArrayConstructor;
interface Math {
    f16round(x: number): number;
}
interface DataView<TArrayBuffer extends ArrayBufferLike> {
    getFloat16(byteOffset: number, littleEndian?: boolean): number;
    setFloat16(byteOffset: number, value: number, littleEndian?: boolean): void;
}
