//! Call arity of a generic signature whose rest parameter is a conditional must
//! be computed against the *erased* signature (type parameters → `any`), so a
//! conditional whose check still references a free type parameter does not
//! collapse to its required branch and over-count required arguments.
//!
//! Regression for #14326 (arktype): for
//! `Fn = <s>(head: readonly s[], ...[opts]: [s] extends [PropertyKey] ?
//! [opts?: Opts] : [opts: Opts]) => void`, the call `fn([1, 2])` produced a
//! spurious TS2554 "Expected 2 arguments, but got 1". The arity computation
//! evaluated the rest conditional with `s` unresolved, collapsing it to the
//! false branch `[opts: Opts]` (one required element) → min 2 args. tsc computes
//! arity from the erased signature (`[any] extends [PropertyKey]` → true →
//! `[opts?: Opts]`) → min 1 arg, so the call is accepted and inference then
//! resolves `s`.
//!
//! Structural rule (one sentence):
//!
//! > When a generic signature's rest parameter is a conditional whose check
//! > still references a free type parameter, call-argument-count bounds are
//! > computed with those type parameters erased to `any` (tsc
//! > `getErasedSignature` parity); a conditional with no free type parameter is
//! > resolved normally.
//!
//! Lib-free: `readonly s[]` and `string | number | symbol` stand in for
//! `ReadonlyArray<s>` / `PropertyKey` so the cases resolve without the full lib.
//! Every test varies user-chosen names so the fix is structural, not name-keyed.

use tsz_checker::test_utils::check_source_code_messages;

const TS2554: u32 = 2554;
const TS2555: u32 = 2555;

fn codes(source: &str) -> Vec<u32> {
    check_source_code_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .filter(|c| *c != 0)
        .collect()
}

// ───────────────────────── 1. reported repro ──────────────────────────────

#[test]
fn generic_conditional_rest_does_not_overcount_required_args() {
    let src = r#"
type Opts = { a?: number };
type Fn = <s>(head: readonly s[], ...[opts]: [s] extends [string | number | symbol] ? [opts?: Opts] : [opts: Opts]) => void;
declare const fn: Fn;
fn([1, 2]);
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "a generic conditional rest must not over-count required args (no TS2554)"
    );
}

// ───────────────────── 2. binder-name variation ───────────────────────────

#[test]
fn generic_conditional_rest_renamed_binders() {
    let src = r#"
type Cfg = { verbose?: boolean };
type Handler = <T>(items: readonly T[], ...[cfg]: [T] extends [string | number | symbol] ? [cfg?: Cfg] : [cfg: Cfg]) => void;
declare const h: Handler;
h(["a", "b"]);
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "renamed generic conditional rest must not over-count required args"
    );
}

// ───────────────────────── 3. negative controls ───────────────────────────

/// A *concrete* (non-generic) conditional rest whose check has no free type
/// parameter must resolve normally: when it picks the required branch, the
/// missing argument still reports TS2554. This proves the erasure is gated on
/// free type parameters, not on the mere presence of a conditional.
#[test]
fn concrete_conditional_rest_required_branch_still_ts2554() {
    let src = r#"
type Opts = { a?: number };
type FnC = (head: readonly object[], ...[opts]: [object] extends [string | number | symbol] ? [opts?: Opts] : [opts: Opts]) => void;
declare const fnc: FnC;
fnc([{}]);
export {};
"#;
    assert!(
        codes(src).contains(&TS2554),
        "a concrete conditional rest resolving to its required branch must still emit TS2554"
    );
}

/// A genuinely-required (non-conditional) trailing parameter still reports
/// TS2554 — the fix only relaxes generic conditional rests.
#[test]
fn genuinely_required_param_still_ts2554() {
    let src = r#"
type Opts = { a?: number };
type Fn2 = <s>(head: readonly s[], opts: Opts) => void;
declare const fn2: Fn2;
fn2([1, 2]);
export {};
"#;
    assert!(
        codes(src).contains(&TS2554),
        "a genuinely-required second parameter must still emit TS2554"
    );
}

/// A non-conditional generic variadic rest (`...rest: T`) keeps its normal arity
/// treatment: a required leading parameter still drives a too-few-args error.
/// Guards against the erasure over-relaxing non-conditional generic rests.
#[test]
fn non_conditional_generic_rest_keeps_arity() {
    let src = r#"
function f3<T extends unknown[]>(first: string, ...rest: T): void {}
f3("a");
f3();
export {};
"#;
    let got = codes(src);
    assert!(
        got.contains(&TS2555) || got.contains(&TS2554),
        "a required leading param before a generic variadic rest must still flag the empty call; got {got:?}"
    );
}

/// Control: a concrete conditional rest resolving to its *optional* branch is
/// accepted (no error) — the optional-branch path is unaffected.
#[test]
fn concrete_conditional_rest_optional_branch_ok() {
    let src = r#"
type Opts = { a?: number };
type FnK = (head: readonly string[], ...[opts]: [string] extends [string | number | symbol] ? [opts?: Opts] : [opts: Opts]) => void;
declare const fnk: FnK;
fnk(["x"]);
export {};
"#;
    assert_eq!(
        codes(src),
        Vec::<u32>::new(),
        "a concrete conditional rest resolving to its optional branch must be accepted"
    );
}
