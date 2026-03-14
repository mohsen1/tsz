/// <reference lib="es2015.iterable" />
/// <reference lib="es2015.symbol" />
interface SymbolConstructor {
    readonly matchAll: unique symbol;
}
interface RegExpStringIterator<T> extends IteratorObject<T, BuiltinIteratorReturn, unknown> {
    [Symbol.iterator](): RegExpStringIterator<T>;
}
interface RegExp {
    [Symbol.matchAll](str: string): RegExpStringIterator<RegExpMatchArray>;
}
