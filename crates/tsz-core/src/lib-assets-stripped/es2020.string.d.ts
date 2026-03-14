/// <reference lib="es2015.iterable" />
/// <reference lib="es2020.intl" />
/// <reference lib="es2020.symbol.wellknown" />
interface String {
    matchAll(regexp: RegExp): RegExpStringIterator<RegExpExecArray>;
    toLocaleLowerCase(locales?: Intl.LocalesArgument): string;
    toLocaleUpperCase(locales?: Intl.LocalesArgument): string;
    localeCompare(that: string, locales?: Intl.LocalesArgument, options?: Intl.CollatorOptions): number;
}
