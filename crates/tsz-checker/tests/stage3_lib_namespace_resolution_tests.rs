//! Regression coverage for Stage-3+ lib namespace resolution.
//!
//! Pins the structural behavior that conformance tests like
//! `tests/cases/compiler/temporal.ts` and
//! `tests/cases/conformance/es2024/sharedMemory.ts` rely on: when a lib file
//! declares an `interface X` + same-named `var X: XConstructor` pair inside a
//! `declare namespace`, both the *value* lookup (`new NS.X(...)`,
//! `NS.X.from(...)`) and the *type* lookup (`let v: NS.X`) must succeed
//! against the merged declaration. Locking this in at the unit-test level
//! means a regression in lib loading, per-file namespace binding, or
//! `interface`/`var` merge ordering surfaces here independently of the
//! heavyweight conformance harness — which historically masked these
//! failures behind the accepted-regressions safety net (see issue #8710).
//!
//! Rule under test (structural, not name-matched):
//!
//! > Given `declare namespace N { interface X { ... } var X: XCtor; }` with
//! > `interface XCtor { new (...): X; readonly prototype: X; }`, every lib
//! > chain that includes the file must let both `new N.X(...)` and
//! > `let v: N.X` resolve without TS2304/TS2503/TS2552 noise on the
//! > namespace qualifier itself, while the missing-property errors *tsc*
//! > does emit for genuinely absent properties still fire.
use std::sync::{Arc, OnceLock};

use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source_with_libs, diagnostics_with_any_code, load_lib_files};
use tsz_common::diagnostics::diagnostic_codes::{
    CANNOT_FIND_NAME, CANNOT_FIND_NAME_DID_YOU_MEAN, CANNOT_FIND_NAMESPACE,
    PROPERTY_DOES_NOT_EXIST_ON_TYPE,
};

/// Diagnostic codes that signal failure mode #8710: name/namespace lookup
/// gave up at the qualifier (`Temporal`, `Temporal.Now`, `SharedArrayBuffer`,
/// `Atomics`, …). Every test below requires these to be absent.
const QUALIFIER_LOOKUP_CODES: &[u32] = &[
    CANNOT_FIND_NAME,
    CANNOT_FIND_NAMESPACE,
    CANNOT_FIND_NAME_DID_YOU_MEAN,
];

fn check_with_libs(source: &str, lib_files: &[Arc<LibFile>]) -> Vec<Diagnostic> {
    assert!(
        !lib_files.is_empty(),
        "lib files must be available; missing embedded lib?",
    );
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2020,
            ..CheckerOptions::default()
        },
        lib_files,
    )
}

fn assert_no_qualifier_lookup_errors(diagnostics: &[Diagnostic], context: &str) {
    let offenders = diagnostics_with_any_code(diagnostics, QUALIFIER_LOOKUP_CODES);
    assert!(
        offenders.is_empty(),
        "{context}: unexpected qualifier lookup diagnostics: {offenders:?}",
    );
}

/// Cache parsed lib files once per group via `OnceLock` so `es5.d.ts` and the
/// other shared basenames are not re-parsed for every test in the group.
/// Mirrors the inline `OnceLock<Vec<Arc<LibFile>>>` pattern in
/// `tests/ts2802_downlevel_iteration_tests.rs` and
/// `src/tests/lib_abstract_member_ts2515_tests.rs`.
macro_rules! cached_lib_group {
    ($name:ident, [$($lib:expr),+ $(,)?]) => {
        fn $name() -> &'static [Arc<LibFile>] {
            static CACHE: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
            CACHE.get_or_init(|| load_lib_files(&[$($lib),+]))
        }
    };
}

cached_lib_group!(
    temporal_libs,
    [
        "es5.d.ts",
        "es2015.iterable.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "es2020.intl.d.ts",
        "es2021.intl.d.ts",
        "es2025.intl.d.ts",
        "esnext.date.d.ts",
        "esnext.intl.d.ts",
        "esnext.temporal.d.ts",
    ]
);

cached_lib_group!(
    shared_memory_libs,
    [
        "es5.d.ts",
        "es2015.iterable.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "es2017.sharedmemory.d.ts",
        "es2020.bigint.d.ts",
        "es2024.sharedmemory.d.ts",
    ]
);

cached_lib_group!(
    iterator_helper_libs,
    [
        "es5.d.ts",
        "es2015.iterable.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "esnext.iterator.d.ts",
    ]
);

/// `Temporal` is bound as a namespace; inside it the `Instant` value
/// (`var Instant: InstantConstructor`) and `Instant` type (`interface
/// Instant`) merge so that `new Temporal.Instant(...)` and the static
/// `Temporal.Instant.from(...)` both resolve.
#[test]
fn temporal_instant_new_resolves_constructor() {
    let source = r#"
const epoch = new Temporal.Instant(0n);
const fromIso = Temporal.Instant.from("2020-01-01T00:00:00Z");
const fromMs = Temporal.Instant.fromEpochMilliseconds(0);
"#;
    let diagnostics = check_with_libs(source, temporal_libs());
    assert_no_qualifier_lookup_errors(&diagnostics, "Temporal.Instant value form");
}

/// Same pattern, different identifier — proves the rule is not keyed off a
/// single name spelling.
#[test]
fn temporal_plain_month_day_value_and_static_methods_resolve() {
    let source = r#"
const md = Temporal.PlainMonthDay.from({ monthCode: "M01", day: 1 });
const md2 = new Temporal.PlainMonthDay(1, 1);
"#;
    let diagnostics = check_with_libs(source, temporal_libs());
    assert_no_qualifier_lookup_errors(&diagnostics, "Temporal.PlainMonthDay value form");
}

/// `Temporal.Now` is a *nested* namespace; lookup must descend two namespace
/// levels and resolve `function` declarations as values.
#[test]
fn temporal_now_nested_namespace_function_resolves() {
    let source = r#"
const now = Temporal.Now.instant();
const tz = Temporal.Now.timeZoneId();
const plainDate = Temporal.Now.plainDateISO();
"#;
    let diagnostics = check_with_libs(source, temporal_libs());
    assert_no_qualifier_lookup_errors(&diagnostics, "Temporal.Now nested namespace");
}

/// Canonical constructors the conformance fixture exercises directly.
#[test]
fn temporal_duration_and_zoned_date_time_constructors_resolve() {
    let source = r#"
const zdt = new Temporal.ZonedDateTime(0n, "UTC");
const dur = Temporal.Duration.from({ hours: 1 });
const _t: Temporal.ZonedDateTimeLike = "2020-01-01T00:00:00Z[UTC]";
"#;
    let diagnostics = check_with_libs(source, temporal_libs());
    assert_no_qualifier_lookup_errors(
        &diagnostics,
        "Temporal.{Duration,ZonedDateTime} constructors",
    );
}

/// Type-position lookup goes through a different resolution path than value
/// position — pin it separately.
#[test]
fn temporal_namespace_members_resolve_as_types() {
    let source = r#"
declare const d: Temporal.PlainDate;
declare const z: Temporal.ZonedDateTime;
declare const md: Temporal.PlainMonthDay;
declare const i: Temporal.Instant;
declare const dur: Temporal.Duration;
declare const t: Temporal.PlainTime;
declare const ym: Temporal.PlainYearMonth;
"#;
    let diagnostics = check_with_libs(source, temporal_libs());
    assert_no_qualifier_lookup_errors(&diagnostics, "Temporal types via namespace qualifier");
}

/// The TS2339 errors the conformance fixture asserts on must keep firing —
/// their presence proves the namespace resolved to the real interfaces
/// rather than degrading to `any`, which would silently swallow the access.
/// Position-based (not message-string-based) check per the §25 anti-hardcoding
/// rule.
#[test]
fn temporal_instant_year_and_plain_month_day_month_emit_ts2339() {
    let source = r#"
declare const instant: Temporal.Instant;
declare const md: Temporal.PlainMonthDay;
instant.year;
md.month;
"#;
    let diagnostics = check_with_libs(source, temporal_libs());
    assert_no_qualifier_lookup_errors(&diagnostics, "Temporal property TS2339 expected");

    let property_errors =
        diagnostics_with_any_code(&diagnostics, &[PROPERTY_DOES_NOT_EXIST_ON_TYPE]);
    assert_eq!(
        property_errors.len(),
        2,
        "expected exactly two TS2339 (Instant.year, PlainMonthDay.month); got {property_errors:?}",
    );
    let year_pos = source
        .find("instant.year")
        .map(|i| i + "instant.".len())
        .expect("source must contain instant.year");
    let month_pos = source
        .find("md.month")
        .map(|i| i + "md.".len())
        .expect("source must contain md.month");
    let property_positions: Vec<u32> = property_errors.iter().map(|d| d.start).collect();
    assert!(
        property_positions.contains(&(year_pos as u32)),
        "missing TS2339 anchored at `instant.year`; got starts {property_positions:?}",
    );
    assert!(
        property_positions.contains(&(month_pos as u32)),
        "missing TS2339 anchored at `md.month`; got starts {property_positions:?}",
    );
}

/// `SharedArrayBuffer` is declared in `es2017.sharedmemory.d.ts` and
/// augmented in `es2024.sharedmemory.d.ts` (with `growable`, `maxByteLength`,
/// `grow`). The two interface declarations must merge so the augmented
/// members surface on instances and the augmented constructor signature
/// accepts the `{ maxByteLength }` options bag.
#[test]
fn shared_array_buffer_es2024_augmentation_merges() {
    let source = r#"
declare const sab: SharedArrayBuffer;
sab.grow(2048);
const grown: boolean = sab.growable;
const cap: number = sab.maxByteLength;
const sab2: SharedArrayBuffer = new SharedArrayBuffer(1024, { maxByteLength: 4096 });
const _bytes: number = sab2.byteLength;
"#;
    let diagnostics = check_with_libs(source, shared_memory_libs());
    assert_no_qualifier_lookup_errors(&diagnostics, "SharedArrayBuffer + es2024 augmentation");
    let mismatches = diagnostics_with_any_code(&diagnostics, &[PROPERTY_DOES_NOT_EXIST_ON_TYPE]);
    assert!(
        mismatches.is_empty(),
        "augmented SharedArrayBuffer should not produce property errors; got {mismatches:?}",
    );
}

/// `Atomics.waitAsync` is added in es2024.sharedmemory. Its result is a
/// discriminated union; destructuring it must resolve `async`/`value` names
/// without TS2304/TS2503.
#[test]
fn atomics_wait_async_discriminated_result_destructures() {
    let source = r#"
const sab = new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT * 1024);
const int32 = new Int32Array(sab);
const { async, value } = Atomics.waitAsync(int32, 0, 0);
async;
value;
"#;
    let diagnostics = check_with_libs(source, shared_memory_libs());
    assert_no_qualifier_lookup_errors(&diagnostics, "Atomics.waitAsync destructure");
}

/// `esnext.iterator.d.ts` adds helper methods (`map`, `filter`, `take`,
/// `toArray`, …) onto `IteratorObject` via `declare global { interface
/// IteratorObject<...> { ... } }`. The adjacent case #8710 calls out — once
/// the lib is loaded those methods must be visible on the merged interface.
#[test]
fn iterator_helpers_es2025_methods_resolve_on_built_in_iterators() {
    let source = r#"
declare const it: IteratorObject<number, undefined, unknown>;
const mapped = it.map((x) => x + 1);
const filtered = it.filter((x): x is number => typeof x === "number");
const taken = it.take(3);
const arr: number[] = it.toArray();
"#;
    let diagnostics = check_with_libs(source, iterator_helper_libs());
    assert_no_qualifier_lookup_errors(&diagnostics, "Iterator helpers resolved");
    let property_errors =
        diagnostics_with_any_code(&diagnostics, &[PROPERTY_DOES_NOT_EXIST_ON_TYPE]);
    assert!(
        property_errors.is_empty(),
        "Iterator helper methods should resolve on IteratorObject; got {property_errors:?}",
    );
}
