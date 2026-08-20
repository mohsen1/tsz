/// <reference lib="es2020.intl" />
interface Date {
    toLocaleString(locales?: Intl.LocalesArgument, options?: Intl.DateTimeFormatOptions): string;
    toLocaleDateString(locales?: Intl.LocalesArgument, options?: Intl.DateTimeFormatOptions): string;
    toLocaleTimeString(locales?: Intl.LocalesArgument, options?: Intl.DateTimeFormatOptions): string;
}
