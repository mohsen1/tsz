//! Regression net for the nested-expando write suppression family behind
//! #17493 → #17495 → #17496.
//!
//! #17493 removed a callable-ness gate on nested expando member writes
//! (`a.d.member = e` where `d` is a binder-tracked chain-key on `a`). Making
//! the checker accept *more* writes silently dropped three real diagnostics
//! where the write target is NOT an open expando host — `tsc` still reports
//! the member absent:
//!
//! - `salsa/typeFromPropertyAssignment21.ts` — a `.prototype` write onto a
//!   built-in **DOM lib interface** (`Event`) → `TS2339`.
//! - `jsdoc/declarations/jsDeclarationsClassMethod.ts` — a `.prototype` write
//!   onto a **class instance** whose real member is a near miss (`method1`)
//!   → `TS2551` with the "Did you mean" suggestion.
//! - `salsa/typeFromPropertyAssignment23.ts` — a `.prototype` write onto a
//!   **user class** (`Module`) that declares no such member → `TS2339`.
//!
//! #17496 restored all three by dropping the vacuous `member_name ==
//! "prototype"` grant from the write fallback. The structural rule, stated
//! in one direction: a nested `base.member = e` write is accepted only when
//! `base` is itself a declared, OPEN expando host (empty-literal / function /
//! class-expression RHS) — a real class instance, a real class's
//! `.prototype`, and a lib interface's `.prototype` are all closed shapes and
//! keep their absent-member diagnostic.
//!
//! This file pins the first two rows, which had **no** in-process coverage:
//! the DOM lib-interface receiver and the class-instance near-miss `TS2551`
//! both need message-level assertions (and `dom.d.ts`), so they route through
//! `check_source_with_libs_code_messages` rather than the codes-only helpers
//! the adjacent expando files use. The user-class `Module.prototype` row is
//! already pinned next door by
//! `js_expando_nested_open_host_write_tests::nested_open_host_write_does_not_bypass_real_class_prototype_member_check`,
//! so it is not duplicated here.
//!
//! These are unit tests because the regression was invisible to the per-merge
//! CI lane (`clippy` + `arch-size`, which runs no checker test) and only
//! surfaced through three lucky scored conformance rows. A suppression of this
//! class is invisible to conformance whenever no scored row happens to cover
//! it, so the rule needs coverage that does not depend on a corpus row
//! existing — including renamed-binder variants so it can never be keyed on
//! the `Event` / `C2` spellings.

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs_code_messages, load_lib_files};

/// `(code, message)` diagnostics for a single checked-JS file (under
/// `noImplicitAny`), with `es5` plus the requested extra libs (e.g.
/// `dom.d.ts`) wired in so built-in receivers like `Event` resolve to their
/// real closed shape.
fn checkjs_diagnostics(source: &str, extra_libs: &[&str]) -> Vec<(u32, String)> {
    let mut names = vec!["es5.d.ts"];
    names.extend_from_slice(extra_libs);
    let libs = load_lib_files(&names);
    let options = CheckerOptions {
        no_implicit_any: true,
        check_js: true,
        allow_js: true,
        ..CheckerOptions::default()
    };
    check_source_with_libs_code_messages(source, "writer.js", options, &libs)
}

/// Whether some diagnostic has `code` and a message mentioning both
/// `subject` (the absent member) and `receiver` (its type). Substring matches
/// keep each assertion robust to the surrounding message template while still
/// pinning the code + the two identifiers `tsc` names.
fn has(diags: &[(u32, String)], code: u32, subject: &str, receiver: &str) -> bool {
    diags
        .iter()
        .any(|(c, m)| *c == code && m.contains(subject) && m.contains(receiver))
}

// --- DOM lib interface receiver (typeFromPropertyAssignment21 family) --------

/// `Event.prototype.removeChildren = function () { ... }`: `Event` is a
/// built-in DOM interface with a closed instance shape, so both the new
/// `.prototype` member and the `this.textContent` read inside the assigned
/// function are absent — two `TS2339`s, exactly `tsc`'s output for
/// `salsa/typeFromPropertyAssignment21.ts`.
#[test]
fn dom_lib_interface_prototype_write_of_absent_member_reports_ts2339() {
    let diags = checkjs_diagnostics(
        "Event.prototype.removeChildren = function () {\n    this.textContent = 'nope'\n}\n",
        &["dom.d.ts"],
    );
    assert!(
        has(&diags, 2339, "'removeChildren'", "'Event'"),
        "a `.prototype` write of an absent member onto a DOM lib interface must report TS2339, not be accepted as an expando. Got: {diags:?}"
    );
    assert!(
        has(&diags, 2339, "'textContent'", "'Event'"),
        "the `this` inside the function assigned to `Event.prototype.x` is the closed `Event` instance, so `this.textContent` is also TS2339. Got: {diags:?}"
    );
}

/// Anti-hardcoding: the DOM-receiver rule is structural (closed lib
/// interface), not keyed on the `Event` / `removeChildren` spellings — a
/// different DOM interface with a different absent member behaves identically.
#[test]
fn dom_lib_interface_prototype_write_rule_is_not_keyed_on_the_event_spelling() {
    let diags = checkjs_diagnostics(
        "Document.prototype.frobnicate = function () {}\n",
        &["dom.d.ts"],
    );
    assert!(
        has(&diags, 2339, "'frobnicate'", "'Document'"),
        "the closed-lib-interface rule must hold for any interface/member pair, not just Event.removeChildren. Got: {diags:?}"
    );
}

// --- class instance receiver, near-miss name (jsDeclarationsClassMethod) -----

/// `C2.prototype.method2 = function () {}` where the class really declares
/// `method1`: a class instance is a closed shape, so the write is absent —
/// and because a near-miss member exists, `tsc` upgrades TS2339 to `TS2551`
/// with a "Did you mean 'method1'?" suggestion, exactly as in
/// `jsdoc/declarations/jsDeclarationsClassMethod.ts`.
#[test]
fn class_instance_prototype_write_of_near_miss_member_reports_ts2551_with_suggestion() {
    let diags = checkjs_diagnostics(
        "class C2 {\n    method1() {}\n}\nC2.prototype.method2 = function () {}\n",
        &[],
    );
    assert!(
        diags.iter().any(|(c, m)| *c == 2551
            && m.contains("'method2'")
            && m.contains("'C2'")
            && m.contains("Did you mean 'method1'")),
        "a `.prototype` write of a near-miss absent member onto a class instance must report TS2551 with the suggestion. Got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == 2339),
        "the near-miss case is TS2551 (which subsumes the absent-member report), never a bare TS2339 alongside it. Got: {diags:?}"
    );
}

/// Anti-hardcoding: the closed-class-instance rejection is structural, not
/// keyed on the `C2` / `method1` / `method2` spellings — a renamed class whose
/// `.prototype` receives an absent member is still rejected with an
/// absent-member diagnostic (TS2339, or TS2551 when the name is close enough
/// to trigger `tsc`'s suggestion heuristic), never silently accepted. Whether
/// a given rename crosses the suggestion threshold is a separate concern from
/// the write-rejection this file guards, so this asserts only the rejection.
#[test]
fn class_instance_prototype_write_rejection_is_not_keyed_on_binder_spelling() {
    let diags = checkjs_diagnostics(
        "class Shape {\n    area() {}\n}\nShape.prototype.perimeter = function () {}\n",
        &[],
    );
    assert!(
        diags.iter().any(|(c, m)| (*c == 2339 || *c == 2551)
            && m.contains("'perimeter'")
            && m.contains("'Shape'")),
        "a `.prototype` write of an absent member onto a renamed class instance must still be rejected, not silently accepted. Got: {diags:?}"
    );
}
