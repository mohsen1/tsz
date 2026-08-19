//! Merged-interface CONSTRUCT signature overload resolution order (tsc
//! `reorderCandidates`), the `new`-expression twin of
//! `merged_interface_overload_order_tests.rs`.
//!
//! `resolve_callable_call` (the call-signature overload loop) already applies
//! `reordered_overload_candidates_if_needed` before trying candidates;
//! `resolve_callable_new` iterated `shape.construct_signatures` in raw stored
//! order with no reorder call, even though construct signatures get the same
//! `declaration_group` stamp as call signatures at lowering time (same-file
//! re-opens: `crates/tsz-lowering/src/lower/core/signature_members.rs`;
//! cross-file merges: `merge_interface_types_cross_file_declaration` in
//! `crates/tsz-checker/src/types/interface_type.rs`). So a merged
//! constructor's later declaration group never won `new` overload resolution.
//! Fixed by wiring the same reorder call into `resolve_callable_new`
//! (`crates/tsz-solver/src/operations/constructors.rs`).
//!
//! Every expectation here is oracle-verified against tsc 7.0.2: a same-file
//! interface re-open with two zero-arg `new` signatures returning different
//! literal types (`interface C { new(): 1; } interface C { new(): 2; }`)
//! resolves `new C()` to the LATER declaration's `2`, not the earlier `1`.

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
interface Alpha { new (v: string): 1; }
interface Alpha { new (v: string): 2; }
declare var Alpha: Alpha;
const picked: 2 = new Alpha("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Negative form of the same witness: the EARLIER group's return type no
/// longer satisfies the annotation, so the assignment must fail.
#[test]
fn earlier_declaration_group_construct_signature_no_longer_matches() {
    let src = r#"
interface Alpha { new (v: string): 1; }
interface Alpha { new (v: string): 2; }
declare var Alpha: Alpha;
const picked: 1 = new Alpha("x");
"#;
    assert_eq!(error_codes(src), vec![2322]);
}

/// The later group keeps its internal source order: its first member wins.
#[test]
fn later_group_internal_order_preserved_for_construct() {
    let src = r#"
interface Beta { new (v: number): "a"; }
interface Beta { new (v: number): "b"; new (v: number): "c"; }
declare var Beta: Beta;
const picked: "b" = new Beta(1);
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// A specialized (literal-param) construct signature from the EARLIER group
/// is hoisted above the later group's catch-all in the candidate order, so a
/// non-matching (non-literal) argument still falls through to the later
/// group's catch-all rather than the earlier group's.
///
/// This only exercises the non-literal-argument arm. The literal-argument
/// arm (`new Gamma("lit")` actually selecting the hoisted specialized
/// signature) hits a separate, pre-existing gap: non-generic `new` overload
/// argument collection never sets `preserve_literal_types` (contrast
/// `resolve_overloaded_call_with_signatures`, which sets it unconditionally
/// for every overloaded CALL), so a literal argument widens to `string`
/// before candidate matching and the specialized signature can never match.
/// That gap lives at the checker argument-collection layer
/// (`crates/tsz-checker/src/types/computation/complex.rs`, gated on
/// `is_generic_new`), not the solver candidate-order layer this fix touches;
/// tracked separately (see PR description).
#[test]
fn specialized_construct_signature_hoist_falls_through_to_later_group_for_non_literal_arg() {
    let src = r#"
interface Gamma { new (v: string): 1; new (v: "lit"): 2; }
interface Gamma { new (v: string): 3; }
declare var Gamma: Gamma;
declare var someString: string;
const stringPick: 3 = new Gamma(someString);
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// A single declaration keeps plain first-overload-wins — no reordering.
#[test]
fn single_declaration_keeps_first_construct_overload_wins() {
    let ok = r#"
interface Delta { new (v: string): 1; new (v: string): 2; }
declare var Delta: Delta;
const picked: 1 = new Delta("x");
"#;
    assert_eq!(error_codes(ok), Vec::<u32>::new());
    let bad = r#"
interface Delta { new (v: string): 1; new (v: string): 2; }
declare var Delta: Delta;
const picked: 2 = new Delta("x");
"#;
    assert_eq!(error_codes(bad), vec![2322]);
}

/// The reorder holds when the merged interface is reached through a type
/// alias wrapper.
#[test]
fn later_group_wins_through_type_alias_wrapper_for_construct() {
    let src = r#"
interface Eps { new (v: string): 1; }
interface Eps { new (v: string): 2; }
type EpsAlias = Eps;
declare var Eps: EpsAlias;
const picked: 2 = new Eps("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// The group stamp survives generic instantiation.
#[test]
fn later_group_wins_on_instantiated_generic_interface_for_construct() {
    let src = r#"
interface Box<T> { new (v: string): 1; }
interface Box<T> { new (v: string): 2; }
declare var Box: Box<number>;
const picked: 2 = new Box("x");
"#;
    assert_eq!(error_codes(src), Vec::<u32>::new());
}

/// Cross-file merge: two program files each re-open the same global
/// interface with a `new` overload. Mirrors the call-signature cross-file
/// fix in `merge_interface_types_cross_file_declaration` (#17658) — the
/// later program file's construct-signature group must win, not whichever
/// file happened to lower the symbol first.
#[test]
fn later_file_wins_for_cross_file_construct_signature_merge() {
    let earlier = r#"
interface Ctor { new (v: string): 1; }
"#;
    let later = r#"
interface Ctor { new (v: string): 2; }
declare var Ctor: Ctor;
const picked: 2 = new Ctor("x");
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
