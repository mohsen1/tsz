//! Regression guard for #14317 (mined from es-toolkit `isEqualWith.ts`): pre-try
//! flow narrowing must survive into a `finally` block even when the `try` body
//! completes abruptly (`return` / `throw`). Fixed by #14407
//! ("fix(binder): seed try/finally flow entry with pre-try and abrupt states"),
//! which shipped without a test — this fills that gap.
//!
//! Structural rule:
//!
//! > A `finally` body runs on every path out of the protected region, so its
//! > entry flow is the union of the pre-try state, the normal try/catch exits,
//! > and every abrupt completion (`return` / `throw` / `break` / `continue`)
//! > that unwinds through it. Narrowing established *before* the `try` is
//! > therefore observable in `finally` even when the `try` always returns.
//! > Previously the binder rooted the finally entry only at the normal try/catch
//! > end label; when the try completed abruptly that label was unreachable, so a
//! > binding reverted to its declared `T | undefined` and tsz emitted a false
//! > `TS18048` in the finally body.
//!
//! `tsc` keeps the pre-try narrowing alive in `finally` in every positive case
//! here and reports `TS18048` only on the negative controls (a path that
//! genuinely leaves the binding possibly-undefined when the finally runs — an
//! unnarrowed binding, and narrowing established only *inside* the try, which the
//! pre-assignment throw path bypasses). Types are declared in-source so the
//! cases depend on no lib surface, and each uses distinct binder / parameter
//! names so the behavior follows the control-flow shape, not any identifier
//! spelling (CLAUDE.md anti-hardcoding gate).

use tsz_checker::test_utils::check_source_strict_codes;

const TS18048_POSSIBLY_UNDEFINED: u32 = 18048;

fn assert_no_possibly_undefined(source: &str) {
    let diags = check_source_strict_codes(source);
    assert!(
        !diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "pre-try narrowing must survive into the finally block \
         (unexpected TS18048); got: {diags:?}",
    );
}

fn assert_possibly_undefined(source: &str) {
    let diags = check_source_strict_codes(source);
    assert!(
        diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "a genuinely possibly-undefined read in finally must still report TS18048; \
         got: {diags:?}",
    );
}

// =========================================================================
// Pre-try narrowing is PRESERVED in finally despite abrupt try completion.
// =========================================================================

/// The minimal #14317 repro: `?? <new>` narrows before the try, the try returns
/// abruptly, and the finally reads the narrowed binding. `tsc` clean.
#[test]
fn nullish_default_before_try_survives_abrupt_return_in_finally() {
    assert_no_possibly_undefined(
        r#"
interface RegistryBox { entries: number; reset(): void; }
function compareStacks(visited: RegistryBox | undefined): number {
    visited = visited ?? { entries: 0, reset() {} };
    try {
        return 1;
    } finally {
        visited.reset();
    }
}
"#,
    );
}

/// `|| <new>` narrowing form, with a `throw` as the abrupt completion. `tsc` clean.
#[test]
fn or_default_before_try_survives_throw_in_finally() {
    assert_no_possibly_undefined(
        r#"
interface HandleBox { closeIt(): void; }
function withResource(handle?: HandleBox): void {
    handle = handle || { closeIt() {} };
    try {
        throw new Error("boom");
    } finally {
        handle.closeIt();
    }
}
"#,
    );
}

/// `if (x === undefined) x = ...` narrowing form before the try. `tsc` clean.
#[test]
fn if_undefined_assignment_before_try_survives_return_in_finally() {
    assert_no_possibly_undefined(
        r#"
interface CacheBox { count: number; clearIt(): void; }
function withCache(lookup?: CacheBox): number {
    if (lookup === undefined) {
        lookup = { count: 0, clearIt() {} };
    }
    try {
        return lookup.count;
    } finally {
        lookup.clearIt();
    }
}
"#,
    );
}

/// A `try`/`catch`/`finally` where both the try and the catch complete abruptly:
/// the finally still observes the pre-try narrowing. `tsc` clean.
#[test]
fn try_catch_both_abrupt_keeps_pre_try_narrowing_in_finally() {
    assert_no_possibly_undefined(
        r#"
interface PoolBox { addOne(n: number): void; }
function guardedRun(pool: PoolBox | undefined): number {
    pool = pool ?? { addOne(_n: number) {} };
    try {
        return 1;
    } catch (err) {
        throw err;
    } finally {
        pool.addOne(0);
    }
}
"#,
    );
}

/// A local (non-parameter) `let` with the pre-try narrow + abrupt-return shape.
/// `tsc` clean.
#[test]
fn local_let_pre_try_narrowing_survives_in_finally() {
    assert_no_possibly_undefined(
        r#"
interface TableBox { count: number; put(k: number, v: number): void; }
function buildTable(seed: boolean): number {
    let table: TableBox | undefined = seed ? { count: 1, put() {} } : undefined;
    table = table ?? { count: 0, put() {} };
    try {
        return table.count;
    } finally {
        table.put(0, 0);
    }
}
"#,
    );
}

// =========================================================================
// NEGATIVE controls: paths that genuinely leave the binding possibly-undefined
// when the finally runs must still report TS18048 (parity with `tsc`).
// =========================================================================

/// No narrowing before/within the try: the parameter is still `T | undefined`
/// when the finally reads it, so the false-positive fix must NOT suppress this
/// genuine TS18048.
#[test]
fn unnarrowed_binding_still_reports_possibly_undefined_in_finally() {
    assert_possibly_undefined(
        r#"
interface ChannelBox { send(n: number): void; }
function leaky(channel: ChannelBox | undefined): number {
    try {
        return 1;
    } finally {
        channel.send(0);
    }
}
"#,
    );
}

/// Narrowing established only INSIDE the try (just before the abrupt return) is
/// NOT observable in finally: a throw before the assignment still unwinds through
/// the finally with the binding undefined, so `tsc` reports TS18048 here. This
/// pins that the fix preserves only the pre-try state, not mid-try narrowing.
#[test]
fn narrowing_inside_try_still_reports_possibly_undefined_in_finally() {
    assert_possibly_undefined(
        r#"
interface DisposableBox { disposeIt(): void; }
function openResource(handle?: DisposableBox): void {
    try {
        handle = handle ?? { disposeIt() {} };
        return;
    } finally {
        handle.disposeIt();
    }
}
"#,
    );
}
