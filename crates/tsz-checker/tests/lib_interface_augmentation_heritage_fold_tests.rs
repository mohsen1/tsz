//! Regression: declaration-merging a user `interface` into an existing lib
//! interface must not corrupt unrelated lib interfaces' member/heritage
//! resolution.
//!
//! Rule under test:
//!
//! > When a program declaration-merges a user `interface X {}` into a lib
//! > interface `X` (a bare global `interface Error {}` or a
//! > `declare global { interface Error {} }`), lib globals are hoisted into the
//! > primary binder's `file_locals`. A later lib type-name reference — e.g.
//! > `Array<string>` in `RegExpMatchArray extends Array<string>`, or
//! > `FlatArray`/`ReadonlyArray` inside `Array.flat`/`Array.flatMap` — must
//! > still resolve to its own lib def. tsc resolves it; tsz previously
//! > canonicalized the reference through a binder-relative `SymbolId`, and
//! > because every lib-binder symbol shares the `u32::MAX` declaration-file
//! > sentinel, the raw `SymbolId -> DefId` lookup answered with an UNRELATED
//! > lib def whose raw index collided (`FlatArray -> eval`,
//! > `ReadonlyArray -> isNaN`), dropping the `Array<string>` numeric-index /
//! > `map` / `length` surface from `RegExpMatchArray`'s heritage fold. The fix
//! > keys `get_canonical_lib_def_id` on the collision-free `DefinitionStore`
//! > name index, and name-verifies the raw fallback for the early window (the
//! > `register_boxed_types` `Array` lowering) before the referenced defs
//! > register.
//!
//! The trigger is *structural* (any user interface merging into any lib
//! interface), not name-bound: the cases below vary the merged binder name and
//! include a control that never merges, so the behavior cannot be a
//! fixture-name fast path.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::{check_multi_file_with_libs, load_default_lib_files};
use tsz_common::diagnostics::Diagnostic;

fn options() -> CheckerOptions {
    CheckerOptions {
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    }
}

fn check(source: &str) -> Vec<Diagnostic> {
    let libs = load_default_lib_files();
    check_multi_file_with_libs(&[("test.ts", source)], "test.ts", options(), &libs)
}

/// Codes emitted when the `RegExpMatchArray -> Array<string>` heritage fold is
/// dropped by the collision: `m[1]` loses the numeric index signature (TS7053),
/// `m.map` / `m.length` lose the inherited members (TS2339, with a downstream
/// TS7006 on the now-untyped callback parameter), and the folded element type
/// no longer matches its annotation (TS2322).
const FOLD_LOSS_CODES: &[u32] = &[7053, 2339, 7006, 2322];

fn fold_loss_offenders(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .filter(|d| FOLD_LOSS_CODES.contains(&d.code))
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// `String.prototype.match` returns `RegExpMatchArray | null`, and
/// `RegExpMatchArray extends Array<string>`. Narrowed to `RegExpMatchArray`,
/// its Array heritage must fold in: the numeric index signature (so `m[1]` is
/// `string`, not TS7053) and `map` / `length` (so they resolve, not TS2339).
/// A `declare global { interface Error {} }` merges into the lib `Error`, which
/// used to corrupt this unrelated fold.
#[test]
fn lib_interface_merge_keeps_regexp_match_array_heritage() {
    let diags = check(
        r#"
export {};
declare global { interface Error { extra?: number } }
const m = "x".match(/x/);
if (m) {
    const first: string = m[1];
    const mapped: string[] = m.map(x => x);
    const len: number = m.length;
}
"#,
    );
    let offenders = fold_loss_offenders(&diags);
    assert!(
        offenders.is_empty(),
        "merging `interface Error` into the lib interface must not drop the \
         RegExpMatchArray -> Array<string> heritage fold (m[1]/m.map/m.length); \
         got: {offenders:?}",
    );
}

/// Same structural rule, different merged binder name: `interface String`
/// merges into the lib `String`. The unrelated RegExpMatchArray fold must stay
/// intact. Varying the name proves the fix is not keyed to a specific lib
/// interface.
#[test]
fn different_lib_interface_merge_keeps_heritage() {
    let diags = check(
        r#"
export {};
declare global { interface String { customFlag?: boolean } }
const m = "y".match(/y/);
if (m) {
    const first: string = m[0];
    const mapped: string[] = m.map(s => s);
    const len: number = m.length;
}
"#,
    );
    let offenders = fold_loss_offenders(&diags);
    assert!(
        offenders.is_empty(),
        "merging `interface String` into the lib interface must not drop the \
         RegExpMatchArray heritage fold; got: {offenders:?}",
    );
}

/// Control: augmenting a *fresh* (non-lib) interface never triggers the merge
/// hoist, so it was always clean. Keeping it green guards against a fix that
/// only suppresses the symptom for known lib names.
#[test]
fn fresh_non_lib_interface_stays_clean() {
    let diags = check(
        r#"
export {};
declare global { interface MyOwnFreshThing { extra?: number } }
const m = "z".match(/z/);
if (m) {
    const first: string = m[1];
    const mapped: string[] = m.map(x => x);
    const len: number = m.length;
}
"#,
    );
    let offenders = fold_loss_offenders(&diags);
    assert!(
        offenders.is_empty(),
        "a fresh non-lib interface must keep the RegExpMatchArray fold intact; \
         got: {offenders:?}",
    );
}
