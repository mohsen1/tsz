/// <reference lib="es2015.iterable" />
interface ObjectConstructor {
    fromEntries<T = any>(entries: Iterable<readonly [PropertyKey, T]>): { [k: string]: T; };
    fromEntries(entries: Iterable<readonly any[]>): any;
}
