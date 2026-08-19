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

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file, check_source_diagnostics};
use tsz_common::common::ModuleKind;

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

/// The reorder holds when the merged interface is reached through a type
/// alias wrapper, not just a direct `declare var` of the interface itself.
#[test]
fn later_group_wins_through_type_alias_wrapper() {
    let src = r#"
interface AliasCtor { new (v: string): 1; }
interface AliasCtor { new (v: string): 2; }
type AliasCtorAlias = AliasCtor;
declare var AliasCtor: AliasCtorAlias;
const picked: 2 = new AliasCtor("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// The `declaration_group` stamp survives generic interface instantiation:
/// reordering a merged generic interface's plain (non-generic-signature)
/// construct overloads still picks the later group after instantiating the
/// interface itself with a concrete type argument.
#[test]
fn later_group_wins_on_instantiated_generic_interface() {
    let src = r#"
interface BoxCtor<T> { new (v: string): 1; }
interface BoxCtor<T> { new (v: string): 2; }
declare var BoxCtor: BoxCtor<number>;
const picked: 2 = new BoxCtor("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Cross-file merge: two program files each re-open the same global
/// interface with a `new` overload. The later program file's
/// construct-signature group must win, mirroring the call-signature
/// cross-file fix in `merge_interface_types_cross_file_declaration`
/// (#17658) — not whichever file happened to lower the symbol first.
#[test]
fn later_file_wins_for_cross_file_construct_signature_merge() {
    let earlier = r#"
interface FileCtor { new (v: string): 1; }
"#;
    let later = r#"
interface FileCtor { new (v: string): 2; }
declare var FileCtor: FileCtor;
const picked: 2 = new FileCtor("x");
"#;
    let diags = check_multi_file(
        &[("./a.ts", earlier), ("./b.ts", later)],
        "./b.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diags.into_iter().map(|d| d.code).collect();
    assert_eq!(codes, Vec::<u32>::new());
}
