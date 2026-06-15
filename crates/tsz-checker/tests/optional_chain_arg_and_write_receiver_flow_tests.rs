//! Regression tests for flow narrowing dropped on a *non-target sub-reference*
//! (issue #13649). Two co-tracked faces share the "a reference that is not the
//! narrowing/write target itself must still read its flow-narrowed type" rule:
//!
//! * Face 1 (immer) — an optional-chain call argument (`p?.m(prop)`) must be
//!   typed against the narrowing in scope at the chain, not against a base that
//!   stripped the enclosing `typeof prop === "string"` guard. The defect lived
//!   in the binder's `optional_chain_branch_base`, which unwound *any* enclosing
//!   `TRUE_CONDITION` when forking an optional chain's branches — including a
//!   guard the chain merely begins inside of.
//! * Face 2 (xstate) — the dotted/element *base receiver* of a write target
//!   (`this._c.next = null`, `o.c.next = null`, `arr[0].next = null`) must keep
//!   its truthiness narrowing; only the outermost write target keeps declared
//!   semantics. The defect lived in `write_receiver_type_for_property_access`,
//!   which discarded the already-computed flow-narrowed receiver whenever the
//!   declared receiver still resolved the written property.
//!
//! Binder names are varied across cases so no fix can latch onto an identifier,
//! and each family pairs the now-fixed positive case with the negative control
//! that must still report.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    check_source(source, "test.ts", strict())
        .iter()
        .map(|d| d.code)
        .collect()
}

// ---------------------------------------------------------------------------
// Face 1 — optional-chain call argument keeps the enclosing typeof narrowing.
// ---------------------------------------------------------------------------

#[test]
fn optional_call_arg_if_condition_keeps_typeof_narrowing() {
    let diags = codes(
        r#"
        function run(prop: string | symbol, plug?: { m(s: string): boolean }) {
          if (typeof prop === "string") {
            if (plug?.m(prop)) return 1;
          }
        }
    "#,
    );
    assert!(
        !diags.contains(&2345),
        "optional-chain call arg in an if-condition must narrow to string: {diags:?}"
    );
}

#[test]
fn optional_call_arg_ternary_bare_and_reassign_keep_narrowing() {
    // The consumption context (ternary condition, bare statement, reassignment)
    // is irrelevant; the optional chain is the only trigger.
    let diags = codes(
        r#"
        function ternary(key: string | symbol, api?: { use(s: string): boolean }) {
          if (typeof key === "string") {
            return api?.use(key) ? 1 : 2;
          }
        }
        function bare(field: string | symbol, host?: { take(s: string): boolean }) {
          if (typeof field === "string") {
            host?.take(field);
          }
        }
        function reassign(name: string | symbol, svc?: { call(s: string): boolean }) {
          let out: unknown;
          if (typeof name === "string") {
            out = svc?.call(name);
          }
          return out;
        }
    "#,
    );
    assert!(
        !diags.contains(&2345),
        "ternary/bare/reassign optional-chain call args must narrow: {diags:?}"
    );
}

#[test]
fn optional_call_arg_chained_and_call_form_keep_narrowing() {
    let diags = codes(
        r#"
        function chained(
          k: string | symbol,
          a?: { b?: { c(s: string): boolean } },
        ) {
          if (typeof k === "string") {
            if (a?.b?.c(k)) return 1;
          }
        }
        function callForm(k: string | symbol, f?: (s: string) => boolean) {
          if (typeof k === "string") {
            if (f?.(k)) return 1;
          }
        }
    "#,
    );
    assert!(
        !diags.contains(&2345),
        "chained / call-form optional chains must narrow the argument: {diags:?}"
    );
}

#[test]
fn optional_call_arg_already_passing_contexts_stay_clean() {
    // These never regressed; keep them green so the fix does not over-correct.
    let diags = codes(
        r#"
        function decl(p: string | symbol, h?: { m(s: string): boolean }) {
          if (typeof p === "string") { const r = h?.m(p); return r; }
        }
        function ret(p: string | symbol, h?: { m(s: string): boolean }) {
          if (typeof p === "string") { return h?.m(p); }
        }
        function loop(p: string | symbol, h?: { m(s: string): boolean }) {
          if (typeof p === "string") { while (h?.m(p)) break; }
        }
        function andGuard(p: string | symbol, h?: { m(s: string): boolean }) {
          if (typeof p === "string") { if (true && h?.m(p)) return 1; }
        }
        function nonOptional(p: string | symbol, h: { m(s: string): boolean }) {
          if (typeof p === "string") { if (h.m(p)) return 1; }
        }
        function elementIndex(p: string | symbol, h?: { o: Record<string, number> }) {
          if (typeof p === "string") { return h?.o[p]; }
        }
    "#,
    );
    assert!(
        !diags.contains(&2345),
        "previously-clean optional-chain arg contexts must stay clean: {diags:?}"
    );
}

#[test]
fn optional_call_arg_without_guard_still_reports() {
    // Negative control: with no narrowing in scope the `symbol` half is a real
    // mismatch and must still surface TS2345.
    let diags = codes(
        r#"
        function run(prop: string | symbol, plug?: { m(s: string): boolean }) {
          if (plug?.m(prop)) return 1;
        }
    "#,
    );
    assert!(
        diags.contains(&2345),
        "unnarrowed string|symbol argument must still report TS2345: {diags:?}"
    );
}

#[test]
fn optional_call_arg_narrowing_does_not_leak_past_chain() {
    // The chain's present-condition must not narrow an unrelated later use.
    let diags = codes(
        r#"
        function run(prop: string | symbol, plug?: { m(s: string): boolean }) {
          if (plug?.m(typeof prop === "string" ? prop : "")) {
          }
          const bad: string = prop;
        }
    "#,
    );
    assert!(
        diags.contains(&2322),
        "string|symbol must not silently narrow outside the chain: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Face 2 — write-target base receiver keeps truthiness narrowing.
// ---------------------------------------------------------------------------

#[test]
fn write_target_this_property_base_narrows() {
    let diags = codes(
        r#"
        class Mailbox<T> {
          current: { value: T; next: unknown } | null = null;
          clear() {
            if (this.current) {
              this.current.next = null;
            }
          }
        }
    "#,
    );
    assert!(
        !diags.contains(&2531) && !diags.contains(&18047),
        "this.current base of a write target must narrow to non-null: {diags:?}"
    );
}

#[test]
fn write_target_dotted_and_element_bases_narrow() {
    let diags = codes(
        r#"
        function dotted(box: { slot: { next: unknown } | null }) {
          if (box.slot) { box.slot.next = null; }
        }
        function nested(box: { mid: { slot: { next: unknown } | null } }) {
          if (box.mid.slot) { box.mid.slot.next = null; }
        }
        function element(slots: ({ next: unknown } | null)[]) {
          if (slots[0]) { slots[0].next = null; }
        }
    "#,
    );
    assert!(
        !diags.contains(&2531) && !diags.contains(&18047),
        "dotted/nested/element write-target bases must narrow to non-null: {diags:?}"
    );
}

#[test]
fn write_target_identifier_base_stays_clean() {
    let diags = codes(
        r#"
        function ident(node: { next: unknown } | null) {
          if (node) { node.next = null; }
        }
    "#,
    );
    assert!(
        !diags.contains(&2531) && !diags.contains(&18047),
        "identifier write-target base must keep narrowing: {diags:?}"
    );
}

#[test]
fn write_target_base_without_guard_still_reports() {
    // Negative control: with no guard the receiver is genuinely nullable.
    let diags = codes(
        r#"
        function dotted(box: { slot: { next: unknown } | null }) {
          box.slot.next = null;
        }
        function ident(node: { next: unknown } | null) {
          node.next = null;
        }
    "#,
    );
    assert!(
        diags.iter().filter(|&&c| c == 2531 || c == 18047).count() >= 2,
        "unguarded nullable write-target bases must still report possibly-null: {diags:?}"
    );
}

#[test]
fn write_target_itself_keeps_declared_type() {
    // Negative control: narrowing the *receiver* must not narrow the actual
    // write target. After `foo[x] === undefined`, the target `foo[x]` keeps its
    // declared `number | undefined`, so writing `1` stays legal (no error) and
    // the declared-type semantics are preserved.
    let diags = codes(
        r#"
        function widen(foo: Record<string, number | undefined>, x: string) {
          if (foo[x] === undefined) {
            foo[x] = 1;
          }
        }
    "#,
    );
    assert!(
        diags.is_empty(),
        "declared-type write target must remain assignable: {diags:?}"
    );
}
