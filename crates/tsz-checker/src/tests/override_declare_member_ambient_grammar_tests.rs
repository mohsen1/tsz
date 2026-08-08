//! Regression tests: a class member that combines `override` with `declare`
//! (member-level ambient, independent of whether the *class* is ambient) must
//! report `tsc`'s TS1040 ("'override' modifier cannot be used in an ambient
//! context.") alone — never also one of the semantic override-legality
//! diagnostics (TS4112 "does not extend another class", TS4113 "not declared
//! in the base class", TS4117 "did you mean", TS4127 "name is dynamic").
//!
//! Verified against `typescript@7.0.2`: `checkGrammarModifiers` reports TS1040
//! for the `override`+`declare` combination on a single member — in *either*
//! source order — and once that grammar error has fired for the member, `tsc`
//! never runs its separate semantic override-validity checks on it. This is
//! true regardless of whether the class extends a base, whether the base
//! declares the member by that name, whether the member's name is dynamic, or
//! whether the *enclosing class* is itself ambient (`declare class`) — the
//! class-level ambient gate in `no_implicit_override_ambient_context_tests.rs`
//! is a different, narrower mechanism (it only suppresses the *implicit*
//! TS4114 requirement) and does not by itself explain this suppression: a
//! `declare class` whose member has `override` but NOT `declare` still
//! reports TS4113 normally (see that file's negative controls).
//!
//! tsz previously computed TS1040 in the parser and TS4112/TS4113 in the
//! checker as two independent diagnostic sources with no shared "already
//! flagged" state, so both fired for the same member.

use crate::test_utils::check_source_with_parse_health;

const TS1031_DECLARE_MODIFIER_WRONG_KIND: u32 = 1031;
const TS1040_MODIFIER_CANNOT_BE_USED_IN_AMBIENT_CONTEXT: u32 = 1040;
const TS4112_CANNOT_HAVE_OVERRIDE_NO_BASE: u32 = 4112;
const TS4113_CANNOT_HAVE_OVERRIDE_NOT_IN_BASE: u32 = 4113;

// ---------------------------------------------------------------------------
// Positive: `override declare` on a member suppresses every override-legality
// checker diagnostic, leaving only the parser's TS1040.
// ---------------------------------------------------------------------------

#[test]
fn override_declare_member_no_base_reports_only_ts1040() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
class Solo {
  override declare m(): void;
}
",
    );
    assert_eq!(
        parse_codes,
        vec![TS1040_MODIFIER_CANNOT_BE_USED_IN_AMBIENT_CONTEXT],
        "parser side: got {parse_codes:?}"
    );
    assert!(
        checker_codes.is_empty(),
        "TS4112 must not also fire once the member's own `declare` has already \
         been flagged with TS1040, got: {checker_codes:?}"
    );
}

#[test]
fn override_declare_member_matching_base_reports_only_ts1040() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
class Base {
  m(): void {}
}
class Derived extends Base {
  override declare m(): void;
}
",
    );
    assert_eq!(
        parse_codes,
        vec![TS1040_MODIFIER_CANNOT_BE_USED_IN_AMBIENT_CONTEXT],
        "parser side: got {parse_codes:?}"
    );
    assert!(
        checker_codes.is_empty(),
        "a `declare`d member is exempt from the override-legality checks even \
         when it happens to match a base member, got: {checker_codes:?}"
    );
}

#[test]
fn override_declare_member_not_in_base_reports_only_ts1040() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
class Base {
  m(): void {}
}
class Derived extends Base {
  override declare notInBase(): void;
}
",
    );
    assert_eq!(
        parse_codes,
        vec![TS1040_MODIFIER_CANNOT_BE_USED_IN_AMBIENT_CONTEXT],
        "parser side: got {parse_codes:?}"
    );
    assert!(
        !checker_codes.contains(&TS4113_CANNOT_HAVE_OVERRIDE_NOT_IN_BASE),
        "TS4113 must not also fire once the member's own `declare` has already \
         been flagged with TS1040, got: {checker_codes:?}"
    );
}

#[test]
fn override_declare_member_dynamic_name_reports_only_ts1040() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
declare const key: unique symbol;
class Solo {
  override declare [key](): void;
}
",
    );
    assert_eq!(
        parse_codes,
        vec![TS1040_MODIFIER_CANNOT_BE_USED_IN_AMBIENT_CONTEXT],
        "parser side: got {parse_codes:?}"
    );
    assert!(
        checker_codes.is_empty(),
        "TS4127 (dynamic name) must not also fire once the member's own \
         `declare` has already been flagged with TS1040, got: {checker_codes:?}"
    );
}

/// Reversed modifier order: `tsc`'s grammar walk hits `declare` first (an
/// illegal modifier on a bodyless, non-abstract, non-ambient-class method) and
/// reports TS1031 alone (oracle-verified against `typescript@7.0.2`) — a
/// different code than the `override`-first order, but the same suppression
/// of the semantic override checks applies. tsz's parser currently reports
/// TS1031 *and* TS1040 together for this ordering (a separate, pre-existing
/// parser-grammar gap unrelated to this fix — real `tsc` stops at the first
/// grammar error per member, tsz's `state_statements_class_members.rs` walk
/// does not for this specific pair); only the checker side is asserted here
/// since that is this fix's contract.
#[test]
fn declare_override_member_reversed_order_no_base_checker_side_is_clean() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
class Solo {
  declare override m(): void;
}
",
    );
    assert!(
        parse_codes.contains(&TS1031_DECLARE_MODIFIER_WRONG_KIND),
        "parser side: got {parse_codes:?}"
    );
    assert!(
        checker_codes.is_empty(),
        "TS4112 must not also fire regardless of which grammar diagnostic the \
         member's `declare` produced, got: {checker_codes:?}"
    );
}

/// Ambient class control: the suppression is driven by the *member's own*
/// `declare`, not derived from the class already being ambient — a `declare
/// class` reports the same lone TS1040 for an `override declare` member.
#[test]
fn override_declare_member_in_declare_class_reports_only_ts1040() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
declare class Base {
  m(): void;
}
declare class Derived extends Base {
  override declare notInBase(): void;
}
",
    );
    assert_eq!(
        parse_codes,
        vec![TS1040_MODIFIER_CANNOT_BE_USED_IN_AMBIENT_CONTEXT],
        "parser side: got {parse_codes:?}"
    );
    assert!(
        !checker_codes.contains(&TS4113_CANNOT_HAVE_OVERRIDE_NOT_IN_BASE),
        "got: {checker_codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: `override` WITHOUT `declare` must keep reporting the
// semantic checks normally — this is the boundary the fix must not cross.
// ---------------------------------------------------------------------------

#[test]
fn override_without_declare_no_base_still_reports_ts4112() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
class Solo {
  override m(): void {}
}
",
    );
    assert!(parse_codes.is_empty(), "got: {parse_codes:?}");
    assert_eq!(
        checker_codes,
        vec![TS4112_CANNOT_HAVE_OVERRIDE_NO_BASE],
        "a plain `override` (no `declare`) with no base class must still \
         report TS4112, got: {checker_codes:?}"
    );
}

#[test]
fn override_without_declare_not_in_base_still_reports_ts4113() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
class Base {
  m(): void {}
}
class Derived extends Base {
  override notInBase(): void {}
}
",
    );
    assert!(parse_codes.is_empty(), "got: {parse_codes:?}");
    assert_eq!(
        checker_codes,
        vec![TS4113_CANNOT_HAVE_OVERRIDE_NOT_IN_BASE],
        "a plain `override` (no `declare`) not declared in the base must \
         still report TS4113, got: {checker_codes:?}"
    );
}

/// A renamed-binder control: the rule is structural (member modifier
/// combination), not name-driven.
#[test]
fn override_declare_member_renamed_binders_reports_only_ts1040() {
    let (parse_codes, checker_codes) = check_source_with_parse_health(
        r"
class Wombat {
  flurble(): void {}
}
class Grommet extends Wombat {
  override declare flurble(): void;
}
",
    );
    assert_eq!(
        parse_codes,
        vec![TS1040_MODIFIER_CANNOT_BE_USED_IN_AMBIENT_CONTEXT],
        "parser side: got {parse_codes:?}"
    );
    assert!(checker_codes.is_empty(), "got: {checker_codes:?}");
}
