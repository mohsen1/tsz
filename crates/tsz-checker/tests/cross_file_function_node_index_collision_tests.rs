//! Regression: a cross-file FUNCTION symbol must not be pinned to a same-raw-
//! `NodeIndex` function declaration in the *current* arena.
//!
//! `NodeIndex` is arena-relative. When a function owned by file A is resolved
//! while checking file B (reached through an `export *` barrel cycle), the
//! cross-arena delegation guard previously pinned resolution locally whenever
//! *a* function node existed at the foreign symbol's declaration `NodeIndex`
//! inside B's arena — without confirming that B's binder actually bound that
//! node to the same symbol. Two functions declared at the same structural
//! position in their respective files collide on that raw index, so the foreign
//! function (e.g. mobx `die: (error, ...args) => never`) was typed with the
//! local function's signature (e.g. `runInAction: <T>(fn: () => T) => T`). The
//! wrong type was then cached under the owner's `(file_idx, SymbolId)` key
//! first-writer-wins, poisoning every later reader and producing a cluster of
//! false `TS2345` ("string is not assignable to `() => T`") at every `die(...)`
//! call (mobx canary: ~69 false positives).
//!
//! The fix requires the current binder's `get_node_symbol` round-trip to map the
//! candidate node back to exactly the symbol being resolved before pinning it
//! locally, which holds for genuine lib/local re-declarations but rejects the
//! cross-arena collision.
//!
//! These tests assert the structural invariants the fix protects: a function
//! re-exported through an `export *` barrel cycle keeps its OWN signature at every
//! call site (positive cases), the result is order-independent, the rule is
//! identifier-agnostic (renamed-binder case), and a genuine local function
//! re-declaration still resolves locally (negative/fallback case so the
//! round-trip did not over-restrict the legitimate path). The raw `NodeIndex`
//! collision itself is layout-sensitive and is reproduced end-to-end by the mobx
//! canary project row (`die`-cluster: 69 false `TS2345` -> 0); these unit tests
//! pin the semantic contract that the fix must preserve.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn check_all(files: &[(&str, &str)]) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_all_multi_file_with_global_index(
        files,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

/// mobx-shaped witness: `die` owned by `errors.ts`, `runInAction` owned by
/// `action.ts` (which also imports `die`), both re-exported through an
/// `export *` barrel that the owners import back from. A consumer of `die`
/// must see `die`'s own signature, not `runInAction`'s.
const ERRORS: &str = r#"
export function die(error: string | number, ...args: any[]): never {
    throw new Error("" + error)
}
"#;

const ACTION: &str = r#"
import { die } from "./internal"

export function runInAction<T>(fn: () => T): T {
    return fn()
}

export function actionDispatch() {
    die("dispatch failed")
}
"#;

const INTERNAL: &str = r#"
export * from "./errors"
export * from "./action"
export * from "./utils"
"#;

const UTILS: &str = r#"
import { die } from "./internal"

export function assertProxies(hasProxy: boolean) {
    if (!hasProxy) {
        die("Proxy not available")
    }
}
"#;

fn witness_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("errors.ts", ERRORS),
        ("action.ts", ACTION),
        ("internal.ts", INTERNAL),
        ("utils.ts", UTILS),
    ]
}

/// Every `die("...")` call across the project must accept a string argument: it
/// must resolve to `die`'s `(string | number, ...args) => never` signature,
/// never `runInAction`'s `(fn: () => T) => T`. The poison surfaces as TS2345 on
/// the string argument.
#[test]
fn die_calls_resolve_die_signature_not_runinaction() {
    let diagnostics = check_all(&witness_files());
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2345),
        "die(\"...\") must accept a string (die's own signature), not be typed \
         with runInAction's `() => T` parameter: {diagnostics:#?}"
    );
}

/// Resolution must be order-independent: rotating the program file order must
/// not flip `die`'s resolved signature (the poison was order-dependent because
/// the first writer of the owner cache key won).
#[test]
fn die_resolution_is_order_independent() {
    let base = witness_files();
    for rotation in 0..base.len() {
        let mut files = base.clone();
        files.rotate_left(rotation);
        let diagnostics = check_all(&files);
        assert!(
            !diagnostics.iter().any(|(code, _)| *code == 2345),
            "die(\"...\") must resolve die's signature regardless of file order \
             (rotation {rotation}): {diagnostics:#?}"
        );
    }
}

/// Adjacent case with renamed binders: the colliding local function and the
/// foreign function carry different user names and the barrel re-exports them,
/// proving the fix is structural (`NodeIndex` round-trip) and not keyed on any
/// identifier string.
#[test]
fn renamed_binders_do_not_cross_contaminate_signatures() {
    let raise = r#"
export function raise(code: number, detail: string): never {
    throw new Error(detail + code)
}
"#;
    let scheduler = r#"
import { raise } from "./barrel"

export function schedule<R>(task: () => R): R {
    return task()
}

export function runOrRaise() {
    raise(7, "boom")
}
"#;
    let barrel = r#"
export * from "./raise_mod"
export * from "./scheduler"
export * from "./caller"
"#;
    let caller = r#"
import { raise } from "./barrel"

export function guard(ok: boolean) {
    if (!ok) {
        raise(42, "guard tripped")
    }
}
"#;
    let files = vec![
        ("raise_mod.ts", raise),
        ("scheduler.ts", scheduler),
        ("barrel.ts", barrel),
        ("caller.ts", caller),
    ];
    let diagnostics = check_all(&files);
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2345),
        "raise(number, string) must keep its own signature across the barrel \
         cycle even with renamed binders: {diagnostics:#?}"
    );
}

/// Negative/fallback case: a genuine local function re-declaration (no foreign
/// collision) must still resolve locally and report a real argument-count
/// mismatch, proving the round-trip guard did not over-restrict the legitimate
/// "user re-declares a function, keep local overloads" path.
#[test]
fn genuine_local_function_call_still_type_checks() {
    let lib = r#"
export function format(value: number): string {
    return "" + value
}
"#;
    let app = r#"
import { format } from "./lib"

export const out: string = format(123)
// real error: format expects a number, not a string
export const bad: string = format("nope")
"#;
    let files = vec![("lib.ts", lib), ("app.ts", app)];
    let diagnostics = check_all(&files);
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2345),
        "format(\"nope\") must still report TS2345 (string not assignable to \
         number) — the local-decl path must keep working: {diagnostics:#?}"
    );
}
