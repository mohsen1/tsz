//! A property read on the right-hand side of a variable's own assignment must
//! observe the variable's *pre-write*, flow-narrowed type — even inside a loop.
//!
//! For `x = x.p` the binder must bind the RHS with the flow that reaches the
//! assignment (so any loop-body guard narrows `x` at the read), and only then
//! create the ASSIGNMENT flow node. Previously tsz created the assignment node
//! first, so the RHS `x` resolved through it and was treated as the *post-write*
//! value; the receiver widened back to the declared union and a spurious TS2339
//! ("Property 'p' does not exist on type 'number | { p: number }'") was reported
//! for code `tsc` accepts.
//!
//! Parity anchor (`tsc` 6.0):
//!   - `while (typeof x === "object") { x = x.p; }`   -> clean (guard re-narrows)
//!   - `while (cond) { x = x.length; }` on `string | number` -> TS2339 (widened,
//!     no guard) — the genuine error must still fire.
//!
//! Binder names are varied across cases so the behavior is driven by the
//! structural shape (loop + self-referential property read + guard), not by any
//! identifier spelling.

use crate::diagnostics::Diagnostic;
use crate::test_utils::{check_source_with_libs, load_default_lib_files, strict_checker_options};

fn codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    let diags: Vec<Diagnostic> =
        check_source_with_libs(source, "test.ts", strict_checker_options(), &libs);
    diags.iter().map(|d| d.code).collect()
}

fn assert_no_2339(source: &str, label: &str) {
    let got = codes(source);
    assert!(
        !got.contains(&2339),
        "{label}: expected no TS2339 for a guarded self-referential loop read, got: {got:?}"
    );
}

/// The reported witness: a `typeof`-object guard narrows `x` to the object
/// constituent at the RHS read, so `x = x.p` type-checks and reassigns `number`.
#[test]
fn while_typeof_object_guard_self_property_read_is_clean() {
    let source = r#"
function walk(node: number | { p: number }) {
    while (typeof node === "object") {
        node = node.p;
    }
}
"#;
    assert_no_2339(source, "while typeof-object guard");
}

/// The original `.length` witness (lib-backed apparent member): a `typeof`-string
/// guard keeps `x` a `string` at the read every iteration.
#[test]
fn while_typeof_string_guard_self_length_read_is_clean() {
    let source = r#"
function shrink(value: number | string) {
    while (typeof value === "string") {
        value = value.length;
    }
}
"#;
    assert_no_2339(source, "while typeof-string guard length");
}

/// A `for` loop with the guard in the condition behaves identically to `while`.
#[test]
fn for_loop_guard_self_property_read_is_clean() {
    let source = r#"
function step(cursor: number | { next: number }) {
    for (; typeof cursor === "object"; ) {
        cursor = cursor.next;
    }
}
"#;
    assert_no_2339(source, "for-loop guard");
}

/// A `do`/`while` whose guard is an early-`break` inside the body still narrows
/// the read that follows it.
#[test]
fn do_while_break_guard_self_property_read_is_clean() {
    let source = r#"
function drain(item: number | { head: number }) {
    do {
        if (typeof item !== "object") break;
        item = item.head;
    } while (true);
}
"#;
    assert_no_2339(source, "do-while break guard");
}

/// Anti-hardcoding: renamed binders and property spelling, same structural shape.
#[test]
fn renamed_binders_guard_self_property_read_is_clean() {
    let source = r#"
function traverse(cell: number | { forward: number }) {
    while (typeof cell === "object") {
        cell = cell.forward;
    }
}
"#;
    assert_no_2339(source, "renamed binders");
}

/// The reassigned value keeps flowing after the loop: a later read sees the
/// declared union, not a leaked narrowed member (sanity that narrowing is
/// scoped to the guarded body).
#[test]
fn post_loop_read_keeps_declared_union() {
    let source = r#"
function after(x: number | { p: number }) {
    while (typeof x === "object") {
        x = x.p;
    }
    const n: number = x;
}
"#;
    // `x` is `number` after the loop (the object arm always reassigns to
    // `number`), so the `const n: number = x` is accepted with no diagnostics.
    let got = codes(source);
    assert!(
        !got.contains(&2339) && !got.contains(&2322),
        "post-loop read parity, got: {got:?}"
    );
}

/// Negative parity: an *unguarded* loop that widens the variable back to a union
/// lacking the property must still report TS2339 — matching `tsc`. The fix must
/// not blanket-suppress the diagnostic; the normal flow-typed receiver already
/// carries the widened union at the read.
#[test]
fn unguarded_widening_loop_still_reports_ts2339() {
    let source = r#"
declare const cond: boolean;
function widen(x: string | number) {
    while (cond) {
        x = x.length;
    }
}
"#;
    let got = codes(source);
    assert!(
        got.contains(&2339),
        "unguarded widening must still emit TS2339 (parity with tsc), got: {got:?}"
    );
}

/// A subexpression read (`x.p + 0`) and an element-access read (`x[k]`) already
/// worked; keep them green so the binder reorder does not regress non-reference
/// right-hand sides.
#[test]
fn subexpression_and_element_reads_stay_clean() {
    let subexpr = r#"
function a(x: number | { p: number }) {
    while (typeof x === "object") {
        x = x.p + 0;
    }
}
"#;
    assert_no_2339(subexpr, "subexpression RHS");

    let element = r#"
function b(x: number | number[]) {
    while (typeof x === "object") {
        x = x[0];
    }
}
"#;
    assert_no_2339(element, "element-access RHS");
}
