//! Regression tests: the false branch of a user-defined type-guard call over a
//! **non-union** source must keep the source unchanged when no part of it can
//! satisfy the guard — matching tsc's `getNarrowedType`.
//!
//! Witnessed by kysely's `default-query-compiler.ts` `appendImmediateValue`:
//! `value: unknown` walked through `isString` / `isNumber` / `isNull` guards
//! and reached an `else if (isDate(value))` arm where `value` had been
//! collapsed to `never`, tripping a false TS2339 on `value.toISOString()`.
//!
//! Root cause: `narrow_call_predicate_guard`'s union-oriented false-branch
//! exclusion fallback ran on non-union sources and excluded members via a
//! `strict_null_checks = false` assignability probe, under which a non-nullish
//! source (`Date`, `unknown`, `{ x }`) is spuriously "related" to a nullish
//! predicate (`null` / `undefined`) and gets dropped to `never`. The fallback
//! is now gated to union sources only; a non-union source is authoritatively
//! narrowed by the primary path.
//!
//! Binder names are varied across cases so no fix can key on a specific
//! identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};
use tsz_common::common::ModuleKind;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(
        source,
        "case.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .filter(|diag| diag.code != 2318) // ignore "no global lib" noise
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

#[test]
fn is_null_false_branch_on_date_keeps_date() {
    // `!isNil(d)` where `d: Date` must stay `Date`, so `.toISOString()` is fine.
    let src = r#"
declare function isNil(o: unknown): o is null;
function run(d: Date): string {
  if (isNil(d)) { return ""; }
  return d.toISOString();
}
"#;
    assert!(
        diagnostics(src).is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics(src)
    );
}

#[test]
fn is_undefined_false_branch_on_date_keeps_date() {
    let src = r#"
declare function absent(v: unknown): v is undefined;
function run(when: Date): string {
  if (absent(when)) { return ""; }
  return when.toISOString();
}
"#;
    assert!(
        diagnostics(src).is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics(src)
    );
}

#[test]
fn is_null_then_second_guard_on_unknown_keeps_source() {
    // The kysely shape: `value: unknown`, a nullish guard, then `else if` a
    // second user guard. The second arm must see the guarded type, not `never`.
    let src = r#"
declare function nullish(x: unknown): x is null;
declare function dateLike(x: unknown): x is Date;
function pick(value: unknown): string {
  if (nullish(value)) {
    return "";
  } else if (dateLike(value)) {
    return value.toISOString();
  }
  return "";
}
"#;
    assert!(
        diagnostics(src).is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics(src)
    );
}

#[test]
fn is_null_false_branch_on_object_literal_keeps_shape() {
    let src = r#"
declare function empty(o: unknown): o is null;
function run(rec: { readonly count: number }): number {
  if (empty(rec)) { return 0; }
  return rec.count;
}
"#;
    assert!(
        diagnostics(src).is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics(src)
    );
}

#[test]
fn full_immediate_value_guard_chain_reaches_date_arm() {
    // Faithful reduction of kysely's appendImmediateValue guard ladder.
    let src = r#"
declare function isText(o: unknown): o is string;
declare function isNum(o: unknown): o is number;
declare function isBool(o: unknown): o is boolean;
declare function isBig(o: unknown): o is bigint;
declare function isNothing(o: unknown): o is null;
declare function isWhen(o: unknown): o is Date;
function emit(value: unknown): string {
  if (isText(value)) {
    return value;
  } else if (isNum(value) || isBool(value) || isBig(value)) {
    return String(value);
  } else if (isNothing(value)) {
    return "null";
  } else if (isWhen(value)) {
    return value.toISOString();
  } else {
    throw new Error("invalid");
  }
}
"#;
    assert!(
        diagnostics(src).is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics(src)
    );
}

#[test]
fn union_source_null_guard_still_excludes_null_member() {
    // Guard rail: the union path must keep working — `Date | null` under the
    // false branch of a `null` predicate narrows to `Date` (member excluded).
    let src = r#"
declare function isNil(o: unknown): o is null;
function run(d: Date | null): string {
  if (isNil(d)) { return ""; }
  return d.toISOString();
}
"#;
    assert!(
        diagnostics(src).is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics(src)
    );
}
