//! Regression coverage for `check_global_augmentation_const_enum_rebind_diagnostics`.
//!
//! This path walks the top-level statements of an external module, descends into
//! `declare global { ... }` blocks, and reports duplicate-identifier (TS2300) and
//! "only one declaration can omit an initializer" (TS2432) diagnostics for `const
//! enum` members. It previously cloned both the outer and inner statement lists on
//! every pass purely to release the arena borrow before emitting; the iteration is
//! now index-based. These tests lock the observable diagnostics so that
//! allocation-only refactors cannot drift the behavior, and vary the enum binder
//! names to prove nothing keys on a particular identifier.

use tsz_checker::test_utils::check_source_codes_named as codes;

fn count(source: &str, code: u32) -> usize {
    codes(source, "test.ts")
        .iter()
        .filter(|&&c| c == code)
        .count()
}

#[test]
fn const_enum_in_global_augmentation_reports_members() {
    // External module (has `export {}`) with a `const enum` whose first member
    // omits an initializer: each member is a duplicate-identifier and the
    // initializer-less first member trips TS2432.
    let source = "export {};\ndeclare global {\n  const enum E { A, B }\n}\n";
    assert_eq!(count(source, 2300), 2);
    assert_eq!(count(source, 2432), 1);
}

#[test]
fn const_enum_with_initializers_no_ts2432() {
    // Every member is initialized, so TS2432 does not fire, but the members are
    // still reported as duplicate identifiers.
    let source = "export {};\ndeclare global {\n  const enum E { A = 1, B = 2 }\n}\n";
    assert_eq!(count(source, 2300), 2);
    assert_eq!(count(source, 2432), 0);
}

#[test]
fn two_const_enums_in_one_global_block() {
    // Two const enums in the same `global` block are both visited via the
    // index-based inner walk. E is fully initialized (no TS2432); F's first
    // member is not (one TS2432).
    let source = "export {};\ndeclare global {\n  const enum E { A = 1, B = 2 }\n  const enum F { X, Y }\n}\n";
    assert_eq!(count(source, 2300), 4);
    assert_eq!(count(source, 2432), 1);
}

#[test]
fn non_const_enum_in_global_is_untouched() {
    // The path only inspects `const enum`; a plain enum (and an interface) inside
    // `global` produces none of these diagnostics.
    let source = "export {};\ndeclare global {\n  enum Plain { A, B }\n  interface I {}\n}\n";
    assert_eq!(count(source, 2300), 0);
    assert_eq!(count(source, 2432), 0);
}

#[test]
fn renamed_enum_binder_same_diagnostics() {
    // A differently spelled enum/member set yields the identical diagnostic
    // shape — the walk is structural, not name-based.
    let source = "export {};\ndeclare global {\n  const enum Palette { Crimson, Azure }\n}\n";
    assert_eq!(count(source, 2300), 2);
    assert_eq!(count(source, 2432), 1);
}

#[test]
fn const_enum_outside_global_block_untouched() {
    // A top-level `const enum` that is not inside a `global` augmentation is not
    // this path's concern and must not produce TS2300/TS2432 from it.
    let source = "export {};\nconst enum E { A, B }\n";
    assert_eq!(count(source, 2300), 0);
    assert_eq!(count(source, 2432), 0);
}
