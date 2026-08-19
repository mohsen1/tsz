//! Merged-interface overload resolution order (tsc `reorderCandidates`).
//!
//! When an interface symbol has multiple declarations (same-file re-opens,
//! lib + user augmentation), tsc resolves calls against the merged overload
//! set with the LATER declaration group's signatures first (each group keeps
//! its internal source order), and hoists specialized signatures — those with
//! literal parameter types — above non-specialized ones regardless of group
//! (GH#1133). Type DISPLAY keeps plain source order; only the candidate
//! order at a call site changes.
//!
//! tsz mirrors this by stamping `CallSignature::declaration_group` in the
//! lowering merge passes and reordering transiently in the solver's
//! `reorder_overload_candidates` — the stored shape order stays as-declared.
//!
//! Every expectation here is oracle-verified against tsc (see #17646; the
//! witness there is `document.getElementById` resolving to a user
//! re-declaration's non-null return, which a false TS18047 previously hid).

use crate::test_utils::check_source_diagnostics;

fn error_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Later declaration group wins for identical parameter types.
#[test]
fn later_declaration_group_signature_wins() {
    let src = r#"
interface Alpha { run(v: string): 1; }
interface Alpha { run(v: string): 2; }
declare var alpha: Alpha;
const picked: 2 = alpha.run("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Negative form of the same witness: the EARLIER group's return type no
/// longer satisfies the annotation, so the assignment must fail.
#[test]
fn earlier_declaration_group_signature_no_longer_matches() {
    let src = r#"
interface Alpha { run(v: string): 1; }
interface Alpha { run(v: string): 2; }
declare var alpha: Alpha;
const picked: 1 = alpha.run("x");
"#;
    assert_eq!(error_codes(src), vec![2322]);
}

/// The later group keeps its internal source order: its first member wins.
#[test]
fn later_group_internal_order_preserved() {
    let src = r#"
interface Beta { go(v: number): "a"; }
interface Beta { go(v: number): "b"; go(v: number): "c"; }
declare var beta: Beta;
const picked: "b" = beta.go(1);
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// A specialized (literal-param) signature from the EARLIER group is hoisted
/// above the later group's catch-all, while the catch-all case still resolves
/// to the later group.
#[test]
fn specialized_signature_hoists_above_later_group() {
    let src = r#"
interface Gamma { pick(v: string): 1; pick(v: "lit"): 2; }
interface Gamma { pick(v: string): 3; }
declare var gamma: Gamma;
declare var someString: string;
const literalPick: 2 = gamma.pick("lit");
const stringPick: 3 = gamma.pick(someString);
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// A single declaration keeps plain first-overload-wins — no reordering.
#[test]
fn single_declaration_keeps_first_overload_wins() {
    let ok = r#"
interface Delta { one(v: string): 1; one(v: string): 2; }
declare var delta: Delta;
const picked: 1 = delta.one("x");
"#;
    assert_eq!(error_codes(ok), Vec::<u32>::new());
    let bad = r#"
interface Delta { one(v: string): 1; one(v: string): 2; }
declare var delta: Delta;
const picked: 2 = delta.one("x");
"#;
    assert_eq!(error_codes(bad), vec![2322]);
}

/// The reorder holds when the merged interface is reached through a type
/// alias wrapper.
#[test]
fn later_group_wins_through_type_alias_wrapper() {
    let src = r#"
interface Eps { m(v: string): 1; }
interface Eps { m(v: string): 2; }
type EpsAlias = Eps;
declare var eps: EpsAlias;
const picked: 2 = eps.m("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// The group stamp survives generic instantiation.
#[test]
fn later_group_wins_on_instantiated_generic_interface() {
    let src = r#"
interface Box<T> { get(v: string): 1; }
interface Box<T> { get(v: string): 2; }
declare var box: Box<number>;
const picked: 2 = box.get("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Interface-level CALL signatures merge with the same rule as methods.
#[test]
fn interface_call_signatures_follow_group_order() {
    let src = r#"
interface Callee { (v: string): 1; }
interface Callee { (v: string): 2; }
declare var callee: Callee;
const picked: 2 = callee("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Display order is untouched: an error that renders the merged method type
/// still lists the overloads in source order, exactly as tsc renders them.
#[test]
fn merged_overload_display_keeps_source_order() {
    let src = r#"
interface Delt { m(x: string): 1; }
interface Delt { m(x: number): 2; }
declare var delt: Delt;
const bad: never = delt.m;
"#;
    let diags = check_source_diagnostics(src);
    assert_eq!(diags.len(), 1);
    assert!(
        diags[0]
            .message_text
            .contains("{ (x: string): 1; (x: number): 2; }"),
        "expected source-order overload rendering, got: {}",
        diags[0].message_text
    );
}
