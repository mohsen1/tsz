/// <reference lib="es2018.intl" />

declare namespace Intl {
    type DurationFormatLocaleMatcher = "lookup" | "best fit";
    type DurationFormatStyle = "long" | "short" | "narrow" | "digital";
    type DurationFormatDisplayOption = "always" | "auto";
    type DurationFormatUnit =
        | "years"
        | "months"
        | "weeks"
        | "days"
        | "hours"
        | "minutes"
        | "seconds"
        | "milliseconds"
        | "microseconds"
        | "nanoseconds";

    type DurationFormatUnitSingular =
        | "year"
        | "month"
        | "week"
        | "day"
        | "hour"
        | "minute"
        | "second"
        | "millisecond"
        | "microsecond"
        | "nanosecond";
    type DurationFormatPart =
        | {
            type: "literal";
            value: string;
            unit?: DurationFormatUnitSingular;
        }
        | {
            type: Exclude<NumberFormatPartTypes, "literal">;
            value: string;
            unit: DurationFormatUnitSingular;
        };
    interface DurationFormatOptions {
        localeMatcher?: DurationFormatLocaleMatcher | undefined;
        numberingSystem?: string | undefined;
        style?: DurationFormatStyle | undefined;
        years?: "long" | "short" | "narrow" | undefined;
        yearsDisplay?: DurationFormatDisplayOption | undefined;
        months?: "long" | "short" | "narrow" | undefined;
        monthsDisplay?: DurationFormatDisplayOption | undefined;
        weeks?: "long" | "short" | "narrow" | undefined;
        weeksDisplay?: DurationFormatDisplayOption | undefined;
        days?: "long" | "short" | "narrow" | undefined;
        daysDisplay?: DurationFormatDisplayOption | undefined;
        hours?: "long" | "short" | "narrow" | "numeric" | "2-digit" | undefined;
        hoursDisplay?: DurationFormatDisplayOption | undefined;
        minutes?: "long" | "short" | "narrow" | "numeric" | "2-digit" | undefined;
        minutesDisplay?: DurationFormatDisplayOption | undefined;
        seconds?: "long" | "short" | "narrow" | "numeric" | "2-digit" | undefined;
        secondsDisplay?: DurationFormatDisplayOption | undefined;
        milliseconds?: "long" | "short" | "narrow" | "numeric" | undefined;
        millisecondsDisplay?: DurationFormatDisplayOption | undefined;
        microseconds?: "long" | "short" | "narrow" | "numeric" | undefined;
        microsecondsDisplay?: DurationFormatDisplayOption | undefined;
        nanoseconds?: "long" | "short" | "narrow" | "numeric" | undefined;
        nanosecondsDisplay?: DurationFormatDisplayOption | undefined;
        fractionalDigits?: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | undefined;
    }
    interface DurationFormat {
        format(duration: Partial<Record<DurationFormatUnit, number>>): string;
        formatToParts(duration: Partial<Record<DurationFormatUnit, number>>): DurationFormatPart[];
        resolvedOptions(): ResolvedDurationFormatOptions;
    }

    interface ResolvedDurationFormatOptions {
        locale: UnicodeBCP47LocaleIdentifier;
        numberingSystem: string;
        style: DurationFormatStyle;
        years: "long" | "short" | "narrow";
        yearsDisplay: DurationFormatDisplayOption;
        months: "long" | "short" | "narrow";
        monthsDisplay: DurationFormatDisplayOption;
        weeks: "long" | "short" | "narrow";
        weeksDisplay: DurationFormatDisplayOption;
        days: "long" | "short" | "narrow";
        daysDisplay: DurationFormatDisplayOption;
        hours: "long" | "short" | "narrow" | "numeric" | "2-digit";
        hoursDisplay: DurationFormatDisplayOption;
        minutes: "long" | "short" | "narrow" | "numeric" | "2-digit";
        minutesDisplay: DurationFormatDisplayOption;
        seconds: "long" | "short" | "narrow" | "numeric" | "2-digit";
        secondsDisplay: DurationFormatDisplayOption;
        milliseconds: "long" | "short" | "narrow" | "numeric";
        millisecondsDisplay: DurationFormatDisplayOption;
        microseconds: "long" | "short" | "narrow" | "numeric";
        microsecondsDisplay: DurationFormatDisplayOption;
        nanoseconds: "long" | "short" | "narrow" | "numeric";
        nanosecondsDisplay: DurationFormatDisplayOption;
        fractionalDigits?: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;
    }

    const DurationFormat: {
        prototype: DurationFormat;
        new (locales?: LocalesArgument, options?: DurationFormatOptions): DurationFormat;
        supportedLocalesOf(locales?: LocalesArgument, options?: { localeMatcher?: DurationFormatLocaleMatcher; }): UnicodeBCP47LocaleIdentifier[];
    };
}
