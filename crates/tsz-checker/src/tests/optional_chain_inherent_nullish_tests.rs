//! Chain-introduced vs inherent nullishness in optional chains (#15682).
//!
//! Structural rule: when a call or member access continues an optional chain
//! but the `?.` token does not directly guard the consumed value (`o?.f()` as
//! opposed to `o.f?.()`), tsc strips only the optionality introduced by the
//! chain short-circuit (the optional-type marker in
//! `getOptionalExpressionType` / `removeOptionalTypeMarker`) and then runs the
//! normal possibly-nullish checks, so a member whose own type includes
//! `undefined`/`null` still reports TS2721/TS2722 (calls) or TS18047/TS18048
//! (member accesses). tsz mirrors this through the per-node optional-chain
//! marker recorded by the chain producers and consumed by
//! `remove_optional_chain_marker`.

use tsz_common::options::checker::CheckerOptions;

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    }
}

fn codes(source: &str) -> Vec<u32> {
    crate::test_utils::check_source(source, "test.ts", opts())
        .iter()
        .map(|diag| diag.code)
        .collect()
}

const CANNOT_INVOKE_POSSIBLY_NULL: u32 = 2721;
const CANNOT_INVOKE_POSSIBLY_UNDEFINED: u32 = 2722;
const IS_POSSIBLY_UNDEFINED: u32 = 18048;
const OBJECT_IS_POSSIBLY_UNDEFINED: u32 = 2532;

// ---------------------------------------------------------------------------
// Reported repro: `box?.fn()` where `fn` itself is optional keeps the
// member's inherent `undefined` and reports TS2722.
// ---------------------------------------------------------------------------

#[test]
fn chain_call_with_inherent_optional_member_reports_ts2722() {
    let diags = codes(
        r#"
declare const bag: { pick?: () => void } | undefined;
const out = bag?.pick();
"#,
    );
    assert_eq!(
        diags,
        vec![CANNOT_INVOKE_POSSIBLY_UNDEFINED],
        "o?.f() with f?: must report TS2722 like tsc; got {diags:?}"
    );
}

#[test]
fn chain_call_with_null_member_reports_ts2721() {
    let diags = codes(
        r#"
declare const crate_: { open: (() => void) | null } | undefined;
const out = crate_?.open();
"#,
    );
    assert_eq!(
        diags,
        vec![CANNOT_INVOKE_POSSIBLY_NULL],
        "o?.f() with f: T | null must report TS2721 like tsc; got {diags:?}"
    );
}

#[test]
fn chain_call_with_entirely_undefined_member_reports_ts2722() {
    let diags = codes(
        r#"
declare const husk: { start: undefined } | undefined;
const out = husk?.start();
"#,
    );
    assert_eq!(
        diags,
        vec![CANNOT_INVOKE_POSSIBLY_UNDEFINED],
        "o?.f() with f: undefined must report TS2722 like tsc; got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / fallback cases: `?.` directly guarding the invoked value, or a
// required member, stay silent.
// ---------------------------------------------------------------------------

#[test]
fn direct_optional_call_on_optional_member_is_clean() {
    let diags = codes(
        r#"
declare const kiosk: { vend?: () => void } | undefined;
kiosk?.vend?.();
declare const stall: { vend?: () => void };
stall.vend?.();
declare const fnv: (() => void) | undefined;
fnv?.();
"#,
    );
    assert_eq!(
        diags,
        Vec::<u32>::new(),
        "?.() guards the callee: {diags:?}"
    );
}

#[test]
fn chain_call_with_required_member_is_clean_and_returns_undefined_union() {
    let diags = codes(
        r#"
declare const cart: { total: () => number } | undefined;
const t: number | undefined = cart?.total();
"#,
    );
    assert_eq!(diags, Vec::<u32>::new(), "required member: {diags:?}");
}

// ---------------------------------------------------------------------------
// Alias/wrapper/nesting variants: the inherent optionality can sit deeper in
// the chain, come from an element access, or come from a chained call result.
// ---------------------------------------------------------------------------

#[test]
fn nested_chain_call_with_inherent_optional_leaf_reports_ts2722() {
    let diags = codes(
        r#"
declare const shelf: { row: { grab?: () => void } } | undefined;
shelf?.row.grab();
"#,
    );
    assert_eq!(diags, vec![CANNOT_INVOKE_POSSIBLY_UNDEFINED], "{diags:?}");
}

#[test]
fn element_access_chain_call_with_inherent_optional_member_reports_ts2722() {
    let diags = codes(
        r#"
declare const drawer: { ["knob"]?: () => void } | undefined;
drawer?.["knob"]();
"#,
    );
    assert_eq!(diags, vec![CANNOT_INVOKE_POSSIBLY_UNDEFINED], "{diags:?}");
}

#[test]
fn chain_member_read_with_inherent_optional_receiver_reports_ts18048() {
    let diags = codes(
        r#"
declare const attic: { box?: { lid: string } } | undefined;
const lid = attic?.box.lid;
"#,
    );
    assert_eq!(
        diags,
        vec![IS_POSSIBLY_UNDEFINED],
        "o?.f.g with f?: must report TS18048 like tsc; got {diags:?}"
    );
}

#[test]
fn chain_element_read_with_inherent_optional_receiver_reports_ts18048() {
    let diags = codes(
        r#"
declare const vault: { bin?: { [slot: string]: number } } | undefined;
const coin = vault?.bin["gold"];
"#,
    );
    assert_eq!(diags, vec![IS_POSSIBLY_UNDEFINED], "{diags:?}");
}

#[test]
fn chained_call_result_with_inherent_optional_member_reports_ts2532() {
    let diags = codes(
        r#"
declare const forge: { cast?: () => ({ heat?: number } | undefined) } | undefined;
const heat = forge?.cast?.().heat;
"#,
    );
    assert_eq!(
        diags,
        vec![OBJECT_IS_POSSIBLY_UNDEFINED],
        "call-result continuation keeps the return type's inherent undefined; got {diags:?}"
    );
}

#[test]
fn middle_inherent_optionality_reports_ts18048_not_ts2722() {
    let diags = codes(
        r#"
declare const yard: { gate?: { open: () => void } } | undefined;
yard?.gate.open();
"#,
    );
    assert_eq!(
        diags,
        vec![IS_POSSIBLY_UNDEFINED],
        "the receiver's inherent undefined reports TS18048 and the call stays clean; got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Guards: a narrowed chain must not report — the marker model may not
// resurrect nullishness that control flow already removed.
// ---------------------------------------------------------------------------

#[test]
fn guarded_chain_call_is_clean() {
    let diags = codes(
        r#"
declare const relay: { send?: () => void } | undefined;
if (relay?.send) {
  relay.send();
  relay?.send();
}
declare const modem: { dial?: () => void } | undefined;
if (modem !== undefined && modem.dial !== undefined) {
  modem?.dial();
}
"#,
    );
    assert_eq!(diags, Vec::<u32>::new(), "guarded chain calls: {diags:?}");
}

#[test]
fn guarded_chain_member_reads_are_clean() {
    let diags = codes(
        r#"
declare const nest: { egg?: { crack: () => void } } | undefined;
if (nest?.egg) {
  nest?.egg.crack();
}
declare const hive: { comb?: { [cell: string]: number } } | undefined;
if (hive?.comb) {
  const honey = hive?.comb["a1"];
}
"#,
    );
    assert_eq!(diags, Vec::<u32>::new(), "guarded continuations: {diags:?}");
}

// ---------------------------------------------------------------------------
// Repeated identical chains share type-level caches; the marker must replay
// per node so the second occurrence behaves like the first.
// ---------------------------------------------------------------------------

#[test]
fn repeated_chain_calls_report_per_occurrence() {
    let diags = codes(
        r#"
declare const pump: { prime?: () => number } | undefined;
const first = pump?.prime();
const second = pump?.prime();
"#,
    );
    assert_eq!(
        diags,
        vec![
            CANNOT_INVOKE_POSSIBLY_UNDEFINED,
            CANNOT_INVOKE_POSSIBLY_UNDEFINED
        ],
        "cache-hit occurrences must keep the marker bit; got {diags:?}"
    );
}

#[test]
fn repeated_clean_chain_calls_stay_clean() {
    let diags = codes(
        r#"
declare const meter: { read: () => number } | undefined;
const a = meter?.read();
const b = meter?.read();
"#,
    );
    assert_eq!(diags, Vec::<u32>::new(), "{diags:?}");
}
