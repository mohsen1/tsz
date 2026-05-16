//! Regression tests for contextual literal narrowing through `keyof Lazy(LibType)`
//! and `IndexAccess(Lazy(LibType), key)`.
//!
//! When a string literal is assigned to an indexed-access or keyof target whose
//! object/operand is a `Lazy(DefId)` reference to a namespace interface (such as
//! `Intl.NumberFormatOptions` from the lib, or a user-declared namespace),
//! `evaluate_type_with_env` may not be able to resolve the Lazy because the def
//! hasn't been registered in the type environment yet. Previously this caused
//! fresh literals like `'currency'` to be widened to `string`, producing false
//! TS2322 errors. The fix forces a stronger Lazy resolution before retrying the
//! keyof evaluation, plus an `IndexAccess` fallback that looks up property types
//! through the contextual property API.
//!
//! Repro for the original arrayToLocaleStringES2015 / ES2020 conformance cases.
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::ScriptTarget;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source, check_source_with_libs, load_lib_files};

fn check(source: &str) -> Vec<Diagnostic> {
    check_source(source, "test.ts", CheckerOptions::default())
}

fn check_with_named_libs(source: &str, lib_names: &[&str]) -> Vec<Diagnostic> {
    let lib_files = load_lib_files(lib_names);
    assert!(
        !lib_files.is_empty(),
        "test libs should be available for {lib_names:?}"
    );
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2020,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
}

const NS_PRELUDE: &str = r#"
declare namespace Lib {
    interface StyleRegistry {
        decimal: never;
        percent: never;
        currency: never;
    }
    type Style = keyof StyleRegistry;
    interface Options {
        style?: Style | undefined;
        currency?: string | undefined;
    }
}
"#;

/// `const x: T = 'currency'` where `type T = Lib.Options['style']` must keep
/// the fresh literal narrow rather than widening to `string`. tsc accepts this
/// assignment (the literal matches `keyof StyleRegistry | undefined`).
#[test]
fn keeps_literal_narrow_via_alias_to_namespace_indexed_access() {
    let mut source = String::from(NS_PRELUDE);
    source.push_str(
        r#"
type S = Lib.Options["style"];
const x: S = "currency";
"#,
    );
    let diagnostics = check(&source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "literal 'currency' must satisfy contextual `keyof StyleRegistry | undefined` via Lazy alias; got {ts2322:?}",
    );
}

/// Direct indexed access on a namespace interface (`Lib.Options['style']`)
/// must also preserve fresh literals. This is the bare form before any alias
/// indirection.
#[test]
fn keeps_literal_narrow_via_direct_namespace_indexed_access() {
    let mut source = String::from(NS_PRELUDE);
    source.push_str(
        r#"
const x: Lib.Options["style"] = "currency";
"#,
    );
    let diagnostics = check(&source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "literal 'currency' must satisfy contextual `Lib.Options['style']`; got {ts2322:?}",
    );
}

/// Intersection of a namespace interface with `{}` must surface the
/// inner-property contextual type so an object literal property keeps its
/// fresh literal type. tsc accepts this; tsz used to widen to `string`.
#[test]
fn intersection_with_namespace_keeps_property_literal_narrow() {
    let mut source = String::from(NS_PRELUDE);
    source.push_str(
        r#"
const x: Lib.Options & {} = { style: "currency" };
"#,
    );
    let diagnostics = check(&source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "object literal `{{ style: 'currency' }}` must satisfy intersection target; got {ts2322:?}",
    );
}

/// Aliasing the intersection (`type T = Lib.X & {}`) must also narrow.
#[test]
fn alias_of_intersection_with_namespace_keeps_property_literal_narrow() {
    let mut source = String::from(NS_PRELUDE);
    source.push_str(
        r#"
type T = Lib.Options & {};
const x: T = { style: "currency" };
"#,
    );
    let diagnostics = check(&source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "object literal must narrow via aliased intersection target; got {ts2322:?}",
    );
}

#[test]
fn temporal_style_generic_options_contextually_type_literal_properties() {
    let source = r#"
type DateUnit = "year" | "month" | "week" | "day";
type TimeUnit = "hour" | "minute" | "second" | "millisecond" | "microsecond" | "nanosecond";
type PluralizeUnit<T extends DateUnit | TimeUnit> =
    | T
    | {
        year: "years";
        month: "months";
        week: "weeks";
        day: "days";
        hour: "hours";
        minute: "minutes";
        second: "seconds";
        millisecond: "milliseconds";
        microsecond: "microseconds";
        nanosecond: "nanoseconds";
    }[T];

interface RoundingOptions<Units extends TimeUnit> {
    smallestUnit?: PluralizeUnit<Units> | undefined;
}

interface ToStringRoundingOptions<Units extends TimeUnit> extends Pick<RoundingOptions<Units>, "smallestUnit"> {}

interface PlainTimeToStringOptions extends ToStringRoundingOptions<Exclude<TimeUnit, "hour">> {
    fractionalSecondDigits?: "auto" | 0 | 1 | 2 | 3 | undefined;
}

interface InstantToStringOptions extends PlainTimeToStringOptions {
    timeZone?: string | undefined;
}

declare const instant: { toString(options?: InstantToStringOptions): string };

instant.toString({ smallestUnit: "second" });
instant.toString({ fractionalSecondDigits: 3 });
instant.toString({ timeZone: "UTC" });
"#;

    let diagnostics = check(source);
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "Temporal-style option literals should be contextually typed; got {ts2345:?}",
    );
}

#[test]
fn temporal_style_rounding_options_fallback_uses_shape_not_alias_display() {
    let source = r#"
type DateUnit = "year" | "month" | "week" | "day";
type TimeUnit = "hour" | "minute" | "second" | "millisecond" | "microsecond" | "nanosecond";
type PluralizeUnit<T extends DateUnit | TimeUnit> =
    | T
    | {
        year: "years";
        month: "months";
        week: "weeks";
        day: "days";
        hour: "hours";
        minute: "minutes";
        second: "seconds";
        millisecond: "milliseconds";
        microsecond: "microseconds";
        nanosecond: "nanoseconds";
    }[T];

interface UnitPairOptions<Units extends DateUnit | TimeUnit> {
    smallestUnit?: PluralizeUnit<Units> | undefined;
    largestUnit?: PluralizeUnit<Units> | undefined;
}

interface RenamedRangeOptions<Units extends DateUnit | TimeUnit> extends UnitPairOptions<Units> {}

declare function compare(options?: RenamedRangeOptions<DateUnit | TimeUnit>): void;

compare({ smallestUnit: "second", largestUnit: "hour" });
"#;

    let diagnostics = check(source);
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "Temporal-style unit pair literals should be accepted by structural shape rather than alias display; got {ts2345:?}",
    );
}

#[test]
fn temporal_lib_to_string_options_contextually_type_literal_arguments() {
    let source = r#"
const instant = Temporal.Instant.fromEpochMilliseconds(1574074321816);
const opts: Temporal.InstantToStringOptions = { timeZone: "UTC" };
declare const time: Temporal.PlainTime;
declare const dt: Temporal.PlainDateTime;
declare const duration: Temporal.Duration;

instant.toString({ timeZone: "UTC" });
instant.toString({ smallestUnit: "minute" });
instant.toString({ fractionalSecondDigits: 4 });
time.toString({ smallestUnit: "minute" });
time.toString({ fractionalSecondDigits: 4 });
dt.toString({ smallestUnit: "minute" });
dt.toString({ fractionalSecondDigits: 4 });
duration.toString({ smallestUnit: "second" });
duration.toString({ fractionalSecondDigits: 4 });
"#;

    let diagnostics = check_with_named_libs(
        source,
        &[
            "es6.d.ts",
            "es2021.intl.d.ts",
            "esnext.date.d.ts",
            "esnext.intl.d.ts",
            "esnext.temporal.d.ts",
            "dom.d.ts",
        ],
    );
    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "Temporal lib option literals should be contextually typed; got {ts2345:?}",
    );
}
