//! Tests for TS2677 on assertion functions whose predicate type is an
//! intersection containing the parameter type (#6082).
//!
//! An intersection `A & B` is structurally a subtype of `A` — TypeScript
//! accepts `asserts d is Data & { ... }` against a parameter typed `Data`.
//! tsz's assignability check sometimes failed to recognize this when the
//! intersection reduced into a plain object form, dropping the alias
//! linkage. The check now accepts the predicate whenever any member of the
//! intersection is assignable to the parameter type — capturing the
//! `A & B <: A` rule without depending on the reduced form's structural
//! shape.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn diags(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn intersection_predicate_with_parameter_type_member_no_ts2677() {
    let source = r#"
interface Data {
  status: "pending" | "complete";
  value?: string;
}

function assertComplete(d: Data): asserts d is Data & { status: "complete"; value: string } {
  if (d.status !== "complete" || !d.value) throw new Error();
}
"#;
    let ds = diags(source);
    let ts2677: Vec<_> = ds.iter().filter(|d| d.0 == 2677).collect();
    assert!(
        ts2677.is_empty(),
        "Expected no TS2677; `Data & {{ ... }}` is structurally assignable to `Data`: {ts2677:?}",
    );
}

#[test]
fn intersection_predicate_with_alternate_names() {
    // Anti-hardcoding (.claude/CLAUDE.md §25): rule is structural, not name-based.
    let source = r#"
interface Box {
  kind: "A" | "B";
  payload?: number;
}

function assertA(b: Box): asserts b is Box & { kind: "A"; payload: number } {
  if (b.kind !== "A" || !b.payload) throw new Error();
}
"#;
    let ds = diags(source);
    let ts2677: Vec<_> = ds.iter().filter(|d| d.0 == 2677).collect();
    assert!(
        ts2677.is_empty(),
        "Expected no TS2677 for alternate names: {ts2677:?}",
    );
}

#[test]
fn predicate_not_assignable_still_flags_ts2677() {
    // Regression guard: when the predicate is genuinely incompatible with
    // the parameter type, TS2677 must still fire.
    let source = r#"
function bad(x: number): x is string & { foo: number } { return true as any; }
"#;
    let ds = diags(source);
    let ts2677: Vec<_> = ds.iter().filter(|d| d.0 == 2677).collect();
    assert_eq!(
        ts2677.len(),
        1,
        "Expected one TS2677 for genuinely incompatible predicate: {ts2677:?}",
    );
}
