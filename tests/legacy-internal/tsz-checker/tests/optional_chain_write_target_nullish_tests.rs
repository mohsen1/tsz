//! A write-only optional-chain target (`=`, a `for...in`/`for...of` head) whose
//! *receiver* carries genuine optionality reports the possibly-nullish family
//! (`TS18047`/`TS18048`/`TS18049`) on that receiver, alongside the existing
//! `TS2779`/`TS2781`/`TS2405` grammar error.
//!
//! This is the write-only companion of `optional_chain_read_before_write_tests`.
//! The discriminator is *not* "write-only forms never report" — it is **which
//! `undefined` the receiver carries**:
//!
//! - `a.b?.c.d = 1` (`c` required): the receiver's `undefined` is the chain's
//!   own short-circuit marker. `tsc` strips the marker before checking the
//!   continuation, so nothing is reported. `TS2779` alone.
//! - `q?.w.e = 1` (`w?` optional): the receiver's `undefined` is real
//!   optionality and survives the strip. `TS18048 'q.w'` + `TS2779`.
//! - `z.y?.x.v = 1` (`y?` and `x?` both optional): the receiver carries both a
//!   marker *and* real optionality. The real one survives.
//!   `TS18048 'z.y.x'` + `TS2779`.
//!
//! Structural rule: the write-target short-circuits in
//! `types/property_access_type/resolve.rs` and `types/computation/access.rs`
//! compute the receiver's type (so its own diagnostics still fire) and then
//! discard it before returning `TypeId::ANY`. Routing that already-computed
//! type through `report_write_target_chain_nullish_receiver`
//! (`types/property_access_type/nullish_access.rs`), which applies
//! `remove_optional_chain_marker` first, is the whole fix. Owner:
//! `types/property_access_type/nullish_access.rs`.
//!
//! The marker strip is load-bearing, not defensive: reporting without it makes
//! every marker-only chain (`a.b?.c.d = 1`) report a `TS18048` `tsc` does not.
//!
//! Every expectation below is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --lib es2022 --module esnext`). The
//! message is asserted, not just the code: this diagnostic and the
//! read-before-write one from #16671 fire at the same anchor with the same
//! code and differ only by the name they carry, so a code-only assertion
//! cannot tell a correct fix from a wrong one. Where a row's point is that a
//! diagnostic fires exactly *once*, the offsets are asserted too.
//!
//! Note on ordering: both diagnostics on a reported row share one anchor, and
//! at a shared anchor tsz's emission order is not stable across the paths
//! involved (the increment path yields the nullish error first, the assignment
//! path yields the grammar error first). The helpers therefore sort by
//! `(offset, code)`, which puts the lower-numbered grammar code first — the
//! reverse of how `tsc` prints the pair. That tie order is a presentation
//! artifact of the unit harness, not a parity claim; the parity claims here
//! are the code set, the anchors, the counts, and the messages.

use tsz_common::options::checker::CheckerOptions;

fn options(strict: bool) -> CheckerOptions {
    CheckerOptions {
        strict,
        strict_null_checks: strict,
        ..CheckerOptions::default()
    }
}

/// Diagnostic codes ordered by source offset, then by code — the same order
/// `tsc` prints them in, so an expected vector here can be read straight off
/// the oracle output. The unit harness itself yields diagnostics in emission
/// order, which puts the grammar error before the nullish one; sorting keeps
/// these assertions comparable to the oracle instead of frozen to whichever
/// checker pass happens to run first.
fn codes_at(source: &str, strict: bool) -> Vec<u32> {
    let mut diags = crate::test_utils::check_source(source, "test.ts", options(strict));
    diags.sort_by_key(|diag| (diag.start, diag.code));
    diags.iter().map(|diag| diag.code).collect()
}

fn strict_codes(source: &str) -> Vec<u32> {
    codes_at(source, true)
}

fn non_strict_null_codes(source: &str) -> Vec<u32> {
    codes_at(source, false)
}

fn messages_for(source: &str, code: u32) -> Vec<String> {
    let mut diags = crate::test_utils::check_source(source, "test.ts", options(true));
    diags.sort_by_key(|diag| (diag.start, diag.code));
    diags
        .into_iter()
        .filter(|diag| diag.code == code)
        .map(|diag| diag.message_text)
        .collect()
}

/// `(start offset, code)` pairs, sorted. Used where the point of the row is
/// that two diagnostics share an anchor, or that a diagnostic fires exactly
/// once — a code-only assertion cannot express either.
fn sites_for(source: &str, code: u32) -> Vec<u32> {
    let mut diags = crate::test_utils::check_source(source, "test.ts", options(true));
    diags.sort_by_key(|diag| (diag.start, diag.code));
    diags
        .into_iter()
        .filter(|diag| diag.code == code)
        .map(|diag| diag.start)
        .collect()
}

const POSSIBLY_UNDEFINED: u32 = 18048;
const POSSIBLY_NULL: u32 = 18047;
const POSSIBLY_NULL_OR_UNDEFINED: u32 = 18049;
const ASSIGNMENT_TARGET: u32 = 2779;
const FOR_OF_TARGET: u32 = 2781;
const INCREMENT_TARGET: u32 = 2777;

// ---------------------------------------------------------------------------
// Genuine receiver optionality, plain assignment.
// ---------------------------------------------------------------------------

#[test]
fn assignment_genuine_receiver_optionality_reports_the_receiver() {
    let source = r#"
declare const q: { w?: { e: number } };
q?.w.e = 1;
"#;
    // tsc 7.0.2: TS18048 'q.w' is possibly 'undefined'. + TS2779
    assert_eq!(
        strict_codes(source),
        vec![ASSIGNMENT_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'q.w' is possibly 'undefined'.".to_string()]
    );
}

#[test]
fn assignment_genuine_receiver_optionality_survives_renamed_binders() {
    // Same shape, every binder renamed: the rule is structural, not a name.
    let source = r#"
declare const holder: { slot?: { leaf: number } };
holder?.slot.leaf = 1;
"#;
    assert_eq!(
        strict_codes(source),
        vec![ASSIGNMENT_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'holder.slot' is possibly 'undefined'.".to_string()]
    );
}

#[test]
fn assignment_marker_and_genuine_optionality_reports_the_genuine_one() {
    // `y?` makes the chain short-circuit (marker) and `x?` is genuinely
    // optional. The genuine `undefined` survives `remove_optional_chain_marker`.
    let source = r#"
declare const z: { y?: { x?: { v: number } } };
z.y?.x.v = 1;
"#;
    assert_eq!(
        strict_codes(source),
        vec![ASSIGNMENT_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'z.y.x' is possibly 'undefined'.".to_string()]
    );
}

#[test]
fn element_access_write_target_genuine_receiver_optionality_reports_the_receiver() {
    // The element-access path is a *separate* short-circuit site
    // (`types/computation/access.rs`), so it needs its own row.
    let source = r#"
declare const idx: { list?: number[] };
idx?.list[0] = 1;
"#;
    assert_eq!(
        strict_codes(source),
        vec![ASSIGNMENT_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'idx.list' is possibly 'undefined'.".to_string()]
    );
}

#[test]
fn assignment_null_only_receiver_reports_possibly_null() {
    let source = r#"
declare const q: { w: { e: number } | null };
q?.w.e = 1;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET, POSSIBLY_NULL]);
    assert_eq!(
        messages_for(source, POSSIBLY_NULL),
        vec!["'q.w' is possibly 'null'.".to_string()]
    );
}

#[test]
fn assignment_nullish_receiver_reports_possibly_null_or_undefined() {
    let source = r#"
declare const q: { w: { e: number } | null | undefined };
q?.w.e = 1;
"#;
    assert_eq!(
        strict_codes(source),
        vec![ASSIGNMENT_TARGET, POSSIBLY_NULL_OR_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_NULL_OR_UNDEFINED),
        vec!["'q.w' is possibly 'null' or 'undefined'.".to_string()]
    );
}

#[test]
fn assignment_generic_receiver_constrained_to_optional_member_reports() {
    // The receiver's optionality arrives through a type parameter's
    // constraint rather than a written object literal type.
    let source = r#"
function g<T extends { w?: { e: number } }>(t: T) {
  t?.w.e = 1;
}
"#;
    assert_eq!(
        strict_codes(source),
        vec![ASSIGNMENT_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'t.w' is possibly 'undefined'.".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Marker-only receiver: silent, exactly as in a read position.
// ---------------------------------------------------------------------------

#[test]
fn assignment_marker_only_receiver_reports_nothing_extra() {
    // `c` is required, so the receiver `a.b?.c` is undefined *only* because
    // the chain may short-circuit. tsc reports TS2779 alone.
    let source = r#"
declare const a: { b?: { c: { d: number } } };
a.b?.c.d = 1;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET]);
}

#[test]
fn element_access_marker_only_receiver_reports_nothing_extra() {
    let source = r#"
declare const a: { b?: { c: number[] } };
a.b?.c[0] = 1;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET]);
}

#[test]
fn assignment_marker_only_receiver_survives_renamed_binders() {
    let source = r#"
declare const root: { mid?: { inner: { leaf: number } } };
root.mid?.inner.leaf = 1;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET]);
}

// ---------------------------------------------------------------------------
// `for...of` / `for...in` heads — the other write-only forms.
// ---------------------------------------------------------------------------

#[test]
fn for_of_head_genuine_receiver_optionality_reports_the_receiver() {
    let source = r#"
declare const q: { w?: { e: number } };
declare const items: number[];
for (q?.w.e of items);
"#;
    assert_eq!(
        strict_codes(source),
        vec![FOR_OF_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'q.w' is possibly 'undefined'.".to_string()]
    );
}

#[test]
fn for_of_head_marker_only_receiver_reports_nothing_extra() {
    let source = r#"
declare const a: { b?: { c: { d: number } } };
declare const items: number[];
for (a.b?.c.d of items);
"#;
    assert_eq!(strict_codes(source), vec![FOR_OF_TARGET]);
}

#[test]
fn for_in_head_genuine_receiver_optionality_reports_the_receiver() {
    // Deliberately asserts only the possibly-nullish half. tsc pairs this row
    // with TS2405 (`the left-hand side must be of type 'string' or 'any'`);
    // tsz still reports TS2780 (`may not be an optional property access`).
    // That divergence is a *separate* defect on the same construct, tracked by
    // #16655, and this fix neither causes nor cures it — so pinning the
    // grammar code here either way would freeze someone else's open bug.
    let source = r#"
declare const q: { w?: { e: number } };
declare const keys: string[];
for (q?.w.e in keys);
"#;
    let codes = strict_codes(source);
    assert!(
        codes.contains(&POSSIBLY_UNDEFINED),
        "expected TS18048 on a for-in head receiver, got {codes:?}"
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'q.w' is possibly 'undefined'.".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Overlap with the read-before-write half (#16671): exactly one TS18048.
// ---------------------------------------------------------------------------

#[test]
fn compound_assignment_genuine_receiver_reports_the_receiver_exactly_once() {
    // `+=` reads before writing, so #16671's machinery also runs here. tsc
    // reports a single TS18048 naming the *receiver*; the two reports must
    // not stack.
    let source = r#"
declare const q: { w?: { e: number } };
q?.w.e += 1;
"#;
    assert_eq!(
        strict_codes(source),
        vec![ASSIGNMENT_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'q.w' is possibly 'undefined'.".to_string()]
    );
    // The count is the point: #16671's read-before-write report and this
    // receiver report both have a claim on this row, and they must not stack.
    assert_eq!(sites_for(source, POSSIBLY_UNDEFINED).len(), 1);
}

#[test]
fn increment_genuine_receiver_reports_the_receiver_exactly_once() {
    let source = r#"
declare const q: { w?: { e: number } };
q?.w.e++;
"#;
    assert_eq!(
        strict_codes(source),
        vec![INCREMENT_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'q.w' is possibly 'undefined'.".to_string()]
    );
    assert_eq!(sites_for(source, POSSIBLY_UNDEFINED).len(), 1);
}

#[test]
fn compound_assignment_marker_only_still_names_the_whole_target() {
    // #16671's row, unchanged by this fix: a marker-only read-before-write
    // target names the whole target, not the receiver.
    let source = r#"
declare const a: { b?: { c: { d: number } } };
a.b?.c.d += 1;
"#;
    assert_eq!(
        strict_codes(source),
        vec![ASSIGNMENT_TARGET, POSSIBLY_UNDEFINED]
    );
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'a.b.c.d' is possibly 'undefined'.".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Negative controls.
// ---------------------------------------------------------------------------

#[test]
fn guarded_link_reports_nothing_extra() {
    // The `.e` link carries its own `?.`, so the chain guards the receiver.
    let source = r#"
declare const q: { w?: { e: number } };
q?.w?.e = 1;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET]);
}

#[test]
fn non_null_asserted_receiver_reports_nothing_extra() {
    let source = r#"
declare const q: { w?: { e: number } };
q?.w!.e = 1;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET]);
}

#[test]
fn non_optional_receiver_reports_nothing_extra() {
    // No optionality anywhere below the target: the chain's own `?.` on a
    // non-nullable root cannot short-circuit.
    let source = r#"
declare const q: { w: { e: number } };
q?.w.e = 1;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET]);
}

#[test]
fn non_strict_null_checks_reports_nothing_extra() {
    let source = r#"
declare const q: { w?: { e: number } };
q?.w.e = 1;
"#;
    assert_eq!(non_strict_null_codes(source), vec![ASSIGNMENT_TARGET]);
}

#[test]
fn ordinary_read_of_the_same_chain_is_unchanged() {
    // The read position already reported this receiver before the fix; it
    // must still report it exactly once and name the same node.
    let source = r#"
declare const q: { w?: { e: number } };
const v = q?.w.e;
"#;
    assert_eq!(strict_codes(source), vec![POSSIBLY_UNDEFINED]);
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'q.w' is possibly 'undefined'.".to_string()]
    );
}

#[test]
fn plain_non_chain_write_to_an_optional_member_is_unchanged() {
    // No optional chain at all: the write-target short-circuit never runs,
    // and tsc reports the receiver through the ordinary access path.
    let source = r#"
declare const q: { w?: { e: number } };
q.w.e = 1;
"#;
    assert_eq!(strict_codes(source), vec![POSSIBLY_UNDEFINED]);
    assert_eq!(
        messages_for(source, POSSIBLY_UNDEFINED),
        vec!["'q.w' is possibly 'undefined'.".to_string()]
    );
}

#[test]
fn ts16710_optional_chain_write_target_does_not_seed_expando_type() {
    // #16710: `obj?.a = 1` is an invalid write target (TS2779, optional
    // chains can never be assignment targets) and must not be read back as
    // an expando-property declaration for later reads of `obj?.a` — tsc
    // keeps the receiver's `any` type. Oracle-verified against
    // `typescript@7.0.2`: TS2779 only, no TS2322.
    let source = r#"
declare const obj: any;
obj?.a = 1;
let x: string = obj?.a;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET]);
}

#[test]
fn ts16710_nonoptional_write_target_control_is_unchanged() {
    // Regression guard: the already-correct non-optional case (`obj.a = 1`
    // never went through the expando fast path in the first place) must
    // stay clean.
    let source = r#"
declare const obj: any;
obj.a = 1;
let x: string = obj.a;
"#;
    assert_eq!(strict_codes(source), Vec::<u32>::new());
}

#[test]
fn ts16710_plain_identifier_any_control_is_unchanged() {
    // Regression guard: plain-identifier `any` narrowing (unrelated to
    // property access) must stay clean.
    let source = r#"
let x: any;
x = 1;
let y: string = x;
"#;
    assert_eq!(strict_codes(source), Vec::<u32>::new());
}

#[test]
fn ts16710_renamed_binder_does_not_change_the_outcome() {
    let source = r#"
declare const holder: any;
holder?.value = 42;
let out: string = holder?.value;
"#;
    assert_eq!(strict_codes(source), vec![ASSIGNMENT_TARGET]);
}
