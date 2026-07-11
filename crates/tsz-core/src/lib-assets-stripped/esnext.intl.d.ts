/// <reference lib="esnext.temporal" />
declare namespace Intl {
    type FormattableTemporalObject = Temporal.PlainDate | Temporal.PlainYearMonth | Temporal.PlainMonthDay | Temporal.PlainTime | Temporal.PlainDateTime | Temporal.Instant;
    interface DateTimeFormat {
        format(date?: FormattableTemporalObject | Date | number): string;
        formatToParts(date?: FormattableTemporalObject | Date | number): DateTimeFormatPart[];
        formatRange(startDate: FormattableTemporalObject | Date | number, endDate: FormattableTemporalObject | Date | number): string;
        formatRangeToParts(startDate: FormattableTemporalObject | Date | number, endDate: FormattableTemporalObject | Date | number): DateTimeRangeFormatPart[];
    }
    interface Locale {
        getCalendars(): string[];
        getCollations(): string[];
        getHourCycles(): string[];
        getNumberingSystems(): string[];
        getTextInfo(): TextInfo;
        getTimeZones(): string[] | undefined;
        getWeekInfo(): WeekInfo;
    }
    interface TextInfo {
        direction?: "ltr" | "rtl";
    }
    interface WeekInfo {
        firstDay: number;
        weekend: number[];
    }
}
