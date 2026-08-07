//! Regression tests for #16769 leg 1: the call-argument (`TS2345`) path
//! dropping the nominal private-member elaboration that the
//! assignment-statement path already attaches.
//!
//! Structural rule (pinned against `typescript@7.0.2`, `--noEmit --strict`):
//! when a call argument fails to satisfy a parameter solely because the two
//! types have distinct nominal private brands, tsc's `checkTypeRelatedTo`
//! attaches the same "separate declarations of a private property" (`TS2442`
//! wording, modifier-`private`) / "refers to a different member" (`TS18015`
//! wording, `#`-private) elaboration under the `TS2345` head that the
//! assignment-statement path already gets via
//! `error_reporter/assignability.rs`'s `private_brand_mismatch_error`
//! interception. tsz now routes the same detail into
//! `error_reporter/call_errors/error_emission.rs`'s
//! `error_argument_not_assignable_at_impl`.

use crate::diagnostics::Diagnostic;
use crate::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::diagnostic_codes;

const TS2345: u32 = diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE;

fn only(diags: &[Diagnostic], code: u32) -> Diagnostic {
    let matching: Vec<_> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one TS{code}; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0].clone()
}

/// Modifier-`private` members with the same name on unrelated classes:
/// `TS2345` elaborated with the `TS2442`-worded "separate declarations" line.
#[test]
fn modifier_private_argument_mismatch_carries_separate_declarations_elaboration() {
    let source = r#"
class M2 { private s = 1; }
class N2 { private s = 1; }
declare function g(n: N2): void;
g(new M2());
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2345);
    assert_eq!(
        diag.message_text,
        "Argument of type 'M2' is not assignable to parameter of type 'N2'."
    );
    assert_eq!(diag.related_information.len(), 1);
    assert_eq!(
        diag.related_information[0].message_text,
        "Types have separate declarations of a private property 's'."
    );
}

/// ES `#`-private members with the same spelling on unrelated classes:
/// `TS2345` elaborated with the `TS18015` wording instead.
#[test]
fn es_private_argument_mismatch_carries_refers_to_different_member_elaboration() {
    let source = r#"
class Alpha { #s = 1; }
class Beta { #s = 1; }
declare function takeBeta(n: Beta): void;
takeBeta(new Alpha());
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2345);
    assert_eq!(
        diag.message_text,
        "Argument of type 'Alpha' is not assignable to parameter of type 'Beta'."
    );
    assert_eq!(diag.related_information.len(), 1);
    assert_eq!(
        diag.related_information[0].message_text,
        "Property '#s' in type 'Alpha' refers to a different member that cannot be accessed from within type 'Beta'."
    );
}

// NOTE: a `protected`-only witness (`class M3 { protected s = 1; }` vs.
// `class N3 { protected s = 1; }`) is deliberately not covered here.
// `private_brand_mismatch_error`'s protected branch renders "Types have
// separate declarations of a protected property" for that shape on BOTH the
// pre-existing assignment path (`error_reporter/assignability.rs`) and this
// PR's new argument path, but the pinned `typescript@7.0.2` oracle instead
// emits "Property 's' is protected but type 'M3' is not a class derived from
// 'N3'." for both. That is a pre-existing divergence in
// `private_brand_mismatch_error` itself (not introduced by routing it into
// the argument path) and out of scope for #16769 leg 1, which is only about
// the `TS2345` path missing elaboration the assignment path already had —
// filed separately, see #16769 leg 1 PR discussion.

/// A `new`-expression argument (not just a plain identifier construction
/// pattern) still elaborates correctly.
#[test]
fn new_expression_argument_position_carries_elaboration() {
    let source = r#"
class M4 { private s = 1; }
class N4 { private s = 1; }
class Wrapper {
    constructor(n: N4) {}
}
new Wrapper(new M4());
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2345);
    assert_eq!(diag.related_information.len(), 1);
    assert_eq!(
        diag.related_information[0].message_text,
        "Types have separate declarations of a private property 's'."
    );
}

/// Negative control: a structurally-failing argument with no private/`#`
/// members keeps its existing property-incompatibility elaboration chain
/// unchanged — the new nominal-brand interception must not fire here.
#[test]
fn structural_mismatch_without_nominal_members_has_no_elaboration() {
    let source = r#"
class Plain1 { s = 1; }
class Plain2 { s = "x"; }
declare function take(n: Plain2): void;
take(new Plain1());
"#;
    let diags = check_source_diagnostics(source);
    let diag = only(&diags, TS2345);
    assert!(
        diag.related_information
            .iter()
            .all(|info| !info.message_text.contains("separate declarations")
                && !info.message_text.contains("refers to a different member")),
        "expected no nominal-brand elaboration for a purely structural mismatch, got {:?}",
        diag.related_information
    );
}
