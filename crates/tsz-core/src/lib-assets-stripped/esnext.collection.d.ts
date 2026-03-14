/// <reference lib="es2024.collection" />
interface ReadonlySetLike<T> {
    keys(): Iterator<T>;
    has(value: T): boolean;
    readonly size: number;
}
interface Set<T> {
    union<U>(other: ReadonlySetLike<U>): Set<T | U>;
    intersection<U>(other: ReadonlySetLike<U>): Set<T & U>;
    difference<U>(other: ReadonlySetLike<U>): Set<T>;
    symmetricDifference<U>(other: ReadonlySetLike<U>): Set<T | U>;
    isSubsetOf(other: ReadonlySetLike<unknown>): boolean;
    isSupersetOf(other: ReadonlySetLike<unknown>): boolean;
    isDisjointFrom(other: ReadonlySetLike<unknown>): boolean;
}
interface ReadonlySet<T> {
    union<U>(other: ReadonlySetLike<U>): Set<T | U>;
    intersection<U>(other: ReadonlySetLike<U>): Set<T & U>;
    difference<U>(other: ReadonlySetLike<U>): Set<T>;
    symmetricDifference<U>(other: ReadonlySetLike<U>): Set<T | U>;
    isSubsetOf(other: ReadonlySetLike<unknown>): boolean;
    isSupersetOf(other: ReadonlySetLike<unknown>): boolean;
    isDisjointFrom(other: ReadonlySetLike<unknown>): boolean;
}
