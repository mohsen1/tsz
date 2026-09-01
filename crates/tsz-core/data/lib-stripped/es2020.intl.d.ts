/// <reference lib="es2018.intl" />
declare namespace Intl {
    type UnicodeBCP47LocaleIdentifier = string;
    type RelativeTimeFormatUnit =
        | "year"
        | "years"
        | "quarter"
        | "quarters"
        | "month"
        | "months"
        | "week"
        | "weeks"
        | "day"
        | "days"
        | "hour"
        | "hours"
        | "minute"
        | "minutes"
        | "second"
        | "seconds";
    type RelativeTimeFormatUnitSingular =
        | "year"
        | "quarter"
        | "month"
        | "week"
        | "day"
        | "hour"
        | "minute"
        | "second";
    type RelativeTimeFormatLocaleMatcher = "lookup" | "best fit";
    type RelativeTimeFormatNumeric = "always" | "auto";
    type RelativeTimeFormatStyle = "long" | "short" | "narrow";
    type LocalesArgument = UnicodeBCP47LocaleIdentifier | Locale | readonly (UnicodeBCP47LocaleIdentifier | Locale)[] | undefined;
    interface RelativeTimeFormatOptions {
        localeMatcher?: RelativeTimeFormatLocaleMatcher;
        numeric?: RelativeTimeFormatNumeric;
        style?: RelativeTimeFormatStyle;
    }
    interface ResolvedRelativeTimeFormatOptions {
        locale: UnicodeBCP47LocaleIdentifier;
        style: RelativeTimeFormatStyle;
        numeric: RelativeTimeFormatNumeric;
        numberingSystem: string;
    }
    type RelativeTimeFormatPart =
        | {
            type: "literal";
            value: string;
        }
        | {
            type: Exclude<NumberFormatPartTypes, "literal">;
            value: string;
            unit: RelativeTimeFormatUnitSingular;
        };
    interface RelativeTimeFormat {
        format(value: number, unit: RelativeTimeFormatUnit): string;
        formatToParts(value: number, unit: RelativeTimeFormatUnit): RelativeTimeFormatPart[];
        resolvedOptions(): ResolvedRelativeTimeFormatOptions;
    }
    const RelativeTimeFormat: {
        new (
            locales?: LocalesArgument,
            options?: RelativeTimeFormatOptions,
        ): RelativeTimeFormat;
        supportedLocalesOf(
            locales?: LocalesArgument,
            options?: RelativeTimeFormatOptions,
        ): UnicodeBCP47LocaleIdentifier[];
    };
    interface NumberFormatOptionsStyleRegistry {
        unit: never;
    }
    interface NumberFormatOptionsCurrencyDisplayRegistry {
        narrowSymbol: never;
    }
    interface NumberFormatOptionsSignDisplayRegistry {
        auto: never;
        never: never;
        always: never;
        exceptZero: never;
    }
    type NumberFormatOptionsSignDisplay = keyof NumberFormatOptionsSignDisplayRegistry;
    interface NumberFormatOptions {
        numberingSystem?: string | undefined;
        compactDisplay?: "short" | "long" | undefined;
        notation?: "standard" | "scientific" | "engineering" | "compact" | undefined;
        signDisplay?: NumberFormatOptionsSignDisplay | undefined;
        unit?: string | undefined;
        unitDisplay?: "short" | "long" | "narrow" | undefined;
        currencySign?: "standard" | "accounting" | undefined;
    }
    interface ResolvedNumberFormatOptions {
        compactDisplay?: "short" | "long";
        notation: "standard" | "scientific" | "engineering" | "compact";
        signDisplay: NumberFormatOptionsSignDisplay;
        unit?: string;
        unitDisplay?: "short" | "long" | "narrow";
        currencySign?: "standard" | "accounting";
    }
    interface NumberFormatPartTypeRegistry {
        compact: never;
        exponentInteger: never;
        exponentMinusSign: never;
        exponentSeparator: never;
        unit: never;
        unknown: never;
    }
    interface DateTimeFormatOptions {
        calendar?: string | undefined;
        dayPeriod?: "narrow" | "short" | "long" | undefined;
        numberingSystem?: string | undefined;
        dateStyle?: "full" | "long" | "medium" | "short" | undefined;
        timeStyle?: "full" | "long" | "medium" | "short" | undefined;
        hourCycle?: "h11" | "h12" | "h23" | "h24" | undefined;
    }
    type LocaleHourCycleKey = "h12" | "h23" | "h11" | "h24";
    type LocaleCollationCaseFirst = "upper" | "lower" | "false";
    interface LocaleOptions {
        baseName?: string;
        calendar?: string;
        caseFirst?: LocaleCollationCaseFirst;
        collation?: string;
        hourCycle?: LocaleHourCycleKey;
        language?: string;
        numberingSystem?: string;
        numeric?: boolean;
        region?: string;
        script?: string;
    }
    interface Locale extends LocaleOptions {
        baseName: string;
        language: string;
        maximize(): Locale;
        minimize(): Locale;
        toString(): UnicodeBCP47LocaleIdentifier;
    }
    const Locale: {
        new (tag: UnicodeBCP47LocaleIdentifier | Locale, options?: LocaleOptions): Locale;
    };
    type DisplayNamesFallback =
        | "code"
        | "none";
    type DisplayNamesType =
        | "language"
        | "region"
        | "script"
        | "calendar"
        | "dateTimeField"
        | "currency";
    type DisplayNamesLanguageDisplay =
        | "dialect"
        | "standard";
    interface DisplayNamesOptions {
        localeMatcher?: RelativeTimeFormatLocaleMatcher;
        style?: RelativeTimeFormatStyle;
        type: DisplayNamesType;
        languageDisplay?: DisplayNamesLanguageDisplay;
        fallback?: DisplayNamesFallback;
    }
    interface ResolvedDisplayNamesOptions {
        locale: UnicodeBCP47LocaleIdentifier;
        style: RelativeTimeFormatStyle;
        type: DisplayNamesType;
        fallback: DisplayNamesFallback;
        languageDisplay?: DisplayNamesLanguageDisplay;
    }
    interface DisplayNames {
        of(code: string): string | undefined;
        resolvedOptions(): ResolvedDisplayNamesOptions;
    }
    const DisplayNames: {
        prototype: DisplayNames;
        new (locales: LocalesArgument, options: DisplayNamesOptions): DisplayNames;
        supportedLocalesOf(locales?: LocalesArgument, options?: { localeMatcher?: RelativeTimeFormatLocaleMatcher; }): UnicodeBCP47LocaleIdentifier[];
    };
    interface CollatorConstructor {
        new (locales?: LocalesArgument, options?: CollatorOptions): Collator;
        (locales?: LocalesArgument, options?: CollatorOptions): Collator;
        supportedLocalesOf(locales: LocalesArgument, options?: CollatorOptions): string[];
    }
    interface DateTimeFormatConstructor {
        new (locales?: LocalesArgument, options?: DateTimeFormatOptions): DateTimeFormat;
        (locales?: LocalesArgument, options?: DateTimeFormatOptions): DateTimeFormat;
        supportedLocalesOf(locales: LocalesArgument, options?: DateTimeFormatOptions): string[];
    }
    interface NumberFormatConstructor {
        new (locales?: LocalesArgument, options?: NumberFormatOptions): NumberFormat;
        (locales?: LocalesArgument, options?: NumberFormatOptions): NumberFormat;
        supportedLocalesOf(locales: LocalesArgument, options?: NumberFormatOptions): string[];
    }
    interface PluralRulesConstructor {
        new (locales?: LocalesArgument, options?: PluralRulesOptions): PluralRules;
        (locales?: LocalesArgument, options?: PluralRulesOptions): PluralRules;
        supportedLocalesOf(locales: LocalesArgument, options?: { localeMatcher?: "lookup" | "best fit"; }): string[];
    }
}
