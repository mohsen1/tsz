//! Merged-interface construct-signature resolution order (tsc
//! `reorderCandidates`, `new`-expression half).
//!
//! tsc's `reorderCandidates` is shared between call and construct
//! resolution: when an interface symbol has multiple declarations, `new` on
//! the merged type tries the LATER declaration group's construct signatures
//! first (each group keeps its internal source order), and hoists
//! specialized construct signatures — those with literal parameter types —
//! above non-specialized ones regardless of group (GH#1133). Type DISPLAY
//! keeps plain source order; only the candidate order at the `new` site
//! changes.
//!
//! tsz stamps `CallSignature::declaration_group` on construct signatures in
//! the lowering merge passes (same stamp as call signatures); the solver's
//! `resolve_callable_new` applies the same transient
//! `reorder_overload_candidates` the call path uses — the stored shape order
//! stays as-declared.
//!
//! Every expectation here is oracle-verified against tsc 7.0.2 (see #17646
//! follow-ups; the call-signature half is fenced by
//! `merged_interface_overload_order_tests`).

use crate::test_utils::check_source_diagnostics;

fn error_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Later declaration group wins for identical parameter types.
#[test]
fn later_declaration_group_construct_signature_wins() {
    let src = r#"
interface MakerCtor { new (v: string): 1; }
interface MakerCtor { new (v: string): 2; }
declare var Maker: MakerCtor;
const picked: 2 = new Maker("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Negative form of the same witness: the EARLIER group's return type no
/// longer satisfies the annotation, so the assignment must fail.
#[test]
fn earlier_declaration_group_construct_signature_no_longer_matches() {
    let src = r#"
interface MakerCtor { new (v: string): 1; }
interface MakerCtor { new (v: string): 2; }
declare var Maker: MakerCtor;
const picked: 1 = new Maker("x");
"#;
    assert_eq!(error_codes(src), vec![2322]);
}

/// The later group keeps its internal source order: its first member wins.
#[test]
fn later_group_internal_construct_order_preserved() {
    let src = r#"
interface BetaCtor { new (v: number): "a"; }
interface BetaCtor { new (v: number): "b"; new (v: number): "c"; }
declare var Beta: BetaCtor;
const picked: "b" = new Beta(1);
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// A specialized (literal-param) construct signature from the EARLIER group
/// is hoisted above the later group's catch-all, while the catch-all case
/// still resolves to the later group.
#[test]
fn specialized_construct_signature_hoists_above_later_group() {
    let src = r#"
interface GammaCtor { new (v: string): 1; new (v: "lit"): 2; }
interface GammaCtor { new (v: string): 3; }
declare var Gamma: GammaCtor;
declare var someString: string;
const literalPick: 2 = new Gamma("lit");
const stringPick: 3 = new Gamma(someString);
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Specialized hoisting applies within a single declaration group too: the
/// literal-param signature declared AFTER the catch-all still wins a literal
/// argument, and the catch-all keeps non-literal arguments.
#[test]
fn single_group_specialized_construct_signature_hoists() {
    let src = r#"
interface SoloCtor { new (v: string): 1; new (v: "lit"): 2; }
declare var Solo: SoloCtor;
declare var anyOldString: string;
const soloLit: 2 = new Solo("lit");
const soloStr: 1 = new Solo(anyOldString);
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// A generic construct signature in the later group outranks the earlier
/// group's concrete signature (candidate order, then inference).
#[test]
fn generic_construct_signature_in_later_group_wins() {
    let src = r#"
interface Box<T> { value: T }
interface GenCtor { new (v: string): 1; }
interface GenCtor { new <T>(v: T): Box<T>; }
declare var Gen: GenCtor;
const gnum: Box<number> = new Gen(42);
const gstr: Box<string> = new Gen("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// A later group that cannot accept the arguments falls through to the
/// earlier group (reorder must not drop candidates).
#[test]
fn non_matching_later_construct_group_falls_through_to_earlier() {
    let src = r#"
interface PortCtor { new (v: string): 1; }
interface PortCtor { new (v: number): 9; }
declare var Port: PortCtor;
const fallback: 1 = new Port("s");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// No overload in any group matches: still exactly one TS2769.
#[test]
fn no_matching_construct_overload_still_reports_ts2769() {
    let src = r#"
interface MissCtor { new (v: string): 1; }
interface MissCtor { new (v: number): 2; }
declare var Miss: MissCtor;
const bad = new Miss(true);
"#;
    assert_eq!(error_codes(src), vec![2769]);
}

/// Construct and call signatures reorder independently: the merged
/// interface's `new` picks the later construct group while a plain call
/// picks the later call group.
#[test]
fn construct_and_call_signatures_reorder_independently() {
    let src = r#"
interface DualCtor { (v: string): "call1"; new (v: string): 1; }
interface DualCtor { (v: string): "call2"; new (v: string): 2; }
declare var Dual: DualCtor;
const built: 2 = new Dual("x");
const called: "call2" = Dual("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Single construct signature is untouched by the reorder plumbing.
#[test]
fn single_construct_signature_unaffected() {
    let src = r#"
interface SingleCtor { new (v: string): 1; }
declare var Single: SingleCtor;
const single: 1 = new Single("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}
