//! Manual tests for enum equality narrowing (`===`/`!==`).
//!
//! Issue #9684: a value typed as a whole enum (`e: E`) must narrow to the
//! matching member literal type after a successful `e === E.A` check, and
//! must narrow to the union of remaining members after a successful
//! `e !== E.A` check. The structural rule is that for control-flow purposes
//! a whole-enum value is treated as the union of its member-typed values
//! (matching tsc's `getBaseTypeOfEnumType`), so equality narrowing remaps the
//! surviving inner literals back to their corresponding member-typed enums.
//!
//! The tests intentionally cover renamed members, heterogeneous enums,
//! negative numeric enums, and the no-op narrowing case so a fix that only
//! special-cases one spelling fails the suite.

use tsz_checker::test_utils::{check_source_code_messages, check_source_diagnostics};

fn assert_clean(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {diagnostics:#?}"
    );
}

fn ts2845_messages(source: &str) -> Vec<String> {
    check_source_code_messages(source)
        .into_iter()
        .filter_map(|(code, message)| (code == 2845).then_some(message))
        .collect()
}

#[test]
fn equality_narrow_two_member_numeric_enum_if_else() {
    let source = r"
enum E { A, B }
declare const e: E;
if (e === E.A) {
  const x: E.A = e;
} else {
  const y: E.B = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_three_member_enum_exclusion_else_branch() {
    let source = r"
enum E { A, B, C }
declare const e: E;
if (e === E.A) {
  const x: E.A = e;
} else {
  const y: E.B | E.C = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_heterogeneous_enum() {
    let source = r#"
enum H { A = 0, B = "b" }
declare const h: H;
if (h === H.A) {
  const ha: H.A = h;
} else {
  const hb: H.B = h;
}
"#;
    assert_clean(source);
}

#[test]
fn equality_narrow_negative_value_enum() {
    let source = r"
enum N { A = -1, B = -2 }
declare const n: N;
if (n === N.A) {
  const na: N.A = n;
} else {
  const nb: N.B = n;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_renamed_members_is_position_independent() {
    // Rename the enum and members; behavior must not depend on the spellings.
    let source = r"
enum Renamed { Foo, Bar }
declare const r: Renamed;
if (r === Renamed.Foo) {
  const rf: Renamed.Foo = r;
} else {
  const rb: Renamed.Bar = r;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_already_member_is_no_op() {
    let source = r"
enum E { A, B }
declare const ea: E.A;
if (ea === E.A) {
  const x: E.A = ea;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_inequality_two_member_else() {
    let source = r"
enum E { A, B }
declare const e: E;
if (e !== E.A) {
  const y: E.B = e;
} else {
  const x: E.A = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_union_predicate_multiple_members() {
    let source = r"
enum E { A, B, C }
declare const m: E;
if (m === E.A || m === E.B) {
  const x: E.A | E.B = m;
} else {
  const y: E.C = m;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_switch_case_collects_remaining_members_in_default() {
    let source = r"
enum E { A, B, C }
function f(e: E) {
  switch (e) {
    case E.A: {
      const a: E.A = e;
      return;
    }
    case E.B: {
      const b: E.B = e;
      return;
    }
    default: {
      const c: E.C = e;
    }
  }
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_with_literal_value_maps_to_corresponding_member() {
    // `e === 0` for `e: E` narrows `e` to the member whose value is `0`.
    let source = r"
enum E { A, B, C }
declare const e: E;
if (e === 0) {
  const x: E.A = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_duplicate_value_enum_preserves_all_member_identities() {
    let source = r"
enum E { A = 0, B = 0, C = 1 }
declare const e: E;
declare const ab: E.A | E.B;
const directA: E.A = ab;
const directB: E.B = ab;
if (e === E.A) {
  const a: E.A = e;
  const b: E.B = e;
  const u: E.A | E.B = e;
}
if (e !== E.A) {
  const c: E.C = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_preserves_whole_enum_assignment_after_narrow() {
    // After narrowing to a member, the value is still assignable to the
    // whole enum (member <: parent).
    let source = r"
enum E { A, B }
declare const e: E;
if (e === E.A) {
  const x: E = e;
}
";
    assert_clean(source);
}

#[test]
fn equality_narrow_does_not_leak_across_unrelated_enums() {
    // The narrowing domain check must use parent enum identity, so
    // narrowing `e: E` by a member of an unrelated enum `F` must not refine
    // `e` (and the assignment to `E.A` must fail).
    let source = r"
enum E { A, B }
enum F { A, B }
declare const e: E;
declare const ok: boolean;
if (ok) {
  const x: E.A = e;
}
";
    let diagnostics = tsz_checker::test_utils::check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322, 1,
        "Expected one TS2322 for unrelated-enum assignment: {diagnostics:?}",
    );
}

#[test]
fn enum_member_truthiness_uses_materialized_literal_values() {
    let messages = ts2845_messages(
        r#"
enum Numeric { Zero = 0, One = 1, Two = 2 }
enum Text { Empty = "", Filled = "filled" }

if (Numeric.Zero) {}
if (Numeric.One) {}
if (Numeric.Two) {}
if (Text.Empty) {}
if (Text.Filled) {}
"#,
    );

    assert_eq!(
        messages.len(),
        5,
        "expected one TS2845 per enum-member condition, got: {messages:#?}"
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.contains("'false'"))
            .count(),
        2,
        "zero and empty-string enum members should be always false, got: {messages:#?}"
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.contains("'true'"))
            .count(),
        3,
        "non-zero and non-empty enum members should be always true, got: {messages:#?}"
    );
}

#[test]
fn renamed_enum_member_truthiness_is_not_name_keyed() {
    let messages = ts2845_messages(
        r#"
enum Renamed { Blank = "", Enabled = 10 }

if (Renamed.Blank) {}
if (Renamed.Enabled) {}
"#,
    );

    assert_eq!(
        messages.len(),
        2,
        "expected TS2845 for both renamed enum member conditions, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|message| message.contains("'false'")),
        "empty-string member should be always false independent of names, got: {messages:#?}"
    );
    assert!(
        messages.iter().any(|message| message.contains("'true'")),
        "numeric non-zero member should be always true independent of names, got: {messages:#?}"
    );
}

#[test]
fn enum_member_truthiness_uses_declared_auto_values() {
    let messages = ts2845_messages(
        r#"
enum Auto { Zero, One, Five = 5, Six }
if (Auto.Zero) {}
if (Auto.One) {}
if (Auto.Six) {}
"#,
    );

    assert_eq!(
        messages.len(),
        3,
        "expected TS2845 for auto-increment enum member conditions, got: {messages:#?}"
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.contains("'false'"))
            .count(),
        1,
        "zero member should be always false through declaration value recovery, got: {messages:#?}"
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.contains("'true'"))
            .count(),
        2,
        "non-zero auto members should be always true through declaration value recovery, got: {messages:#?}"
    );
}

#[test]
fn enum_member_truthiness_does_not_fabricate_value_after_string_initializer() {
    let messages = ts2845_messages(
        r#"
enum Mixed { Empty = "", Missing }
if (Mixed.Empty) {}
if (Mixed.Missing) {}
"#,
    );

    assert_eq!(
        messages.len(),
        1,
        "only the declared string member should get TS2845; do not fabricate a numeric value after a string initializer: {messages:#?}"
    );
    assert!(
        messages[0].contains("'false'"),
        "empty string member should be always false, got: {messages:#?}"
    );
}
