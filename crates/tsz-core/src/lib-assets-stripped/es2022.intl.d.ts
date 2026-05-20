declare namespace Intl {
    interface SegmenterOptions {
        localeMatcher?: "best fit" | "lookup" | undefined;
        granularity?: "grapheme" | "word" | "sentence" | undefined;
    }
    interface Segmenter {
        segment(input: string): Segments;
        resolvedOptions(): ResolvedSegmenterOptions;
    }
    interface ResolvedSegmenterOptions {
        locale: string;
        granularity: "grapheme" | "word" | "sentence";
    }
    interface SegmentIterator<T> extends IteratorObject<T, BuiltinIteratorReturn, unknown> {
        [Symbol.iterator](): SegmentIterator<T>;
    }
    interface Segments {
        containing(codeUnitIndex?: number): SegmentData | undefined;
        [Symbol.iterator](): SegmentIterator<SegmentData>;
    }
    interface SegmentData {
        segment: string;
        index: number;
        input: string;
        isWordLike?: boolean;
    }
    const Segmenter: {
        prototype: Segmenter;
        new (locales?: LocalesArgument, options?: SegmenterOptions): Segmenter;
        supportedLocalesOf(locales: LocalesArgument, options?: Pick<SegmenterOptions, "localeMatcher">): UnicodeBCP47LocaleIdentifier[];
    };
    function supportedValuesOf(key: "calendar" | "collation" | "currency" | "numberingSystem" | "timeZone" | "unit"): string[];
}
