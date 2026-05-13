//! Tests for TS2340 on `super.<accessor>` reads.
//!
//! Closes #5995. The structural rule:
//!
//! > `super.X` is only legal when X is a method on the base class. When
//! > X is a `get` accessor (or `set` accessor read as a value), tsc
//! > emits TS2340: "Only public and protected methods of the base
//! > class are accessible via the 'super' keyword."
//!
//! The fix lives in `check_property_accessibility_super_method`-style
//! logic in `checkers/property_checker.rs`. It adds a dedicated
//! `class_chain_member_is_accessor` lookup (in `classes/class_summary.rs`)
//! because the existing `class_chain_member_kind_name_only` folds
//! accessors and methods together as `MethodLike`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn diags(source: &str) -> Vec<(u32, String)> {
    check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn has_ts2340(diags: &[(u32, String)]) -> bool {
    diags
        .iter()
        .any(|(c, m)| *c == 2340 && m.contains("methods") && m.contains("'super'"))
}

#[test]
fn super_get_accessor_read_emits_ts2340() {
    // Direct repro from #5995.
    let source = r#"
class Base {
  get value(): number {
    return 0;
  }
}

class Derived extends Base {
  override get value(): number {
    return super.value + 1;
  }
}
"#;
    let d = diags(source);
    assert!(
        has_ts2340(&d),
        "expected TS2340 for super.<getter>, got: {d:?}",
    );
}

#[test]
fn super_get_accessor_read_different_name_emits_ts2340() {
    // Anti-hardcoding: the property name varies. Same rule applies.
    let source = r#"
class A {
  get size(): number {
    return 0;
  }
}

class B extends A {
  override get size(): number {
    return super.size * 2;
  }
}
"#;
    let d = diags(source);
    assert!(
        has_ts2340(&d),
        "expected TS2340 for super.<getter> (name 'size'), got: {d:?}",
    );
}

#[test]
fn super_method_call_no_ts2340() {
    // Regression guard: regular method access via super must remain OK.
    let source = r#"
class Base {
  greet(): string {
    return "hello";
  }
}

class Derived extends Base {
  greet(): string {
    return super.greet() + " world";
  }
}
"#;
    let d = diags(source);
    assert!(
        !d.iter().any(|(c, _)| *c == 2340),
        "super.method() must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_get_accessor_write_no_ts2340_on_read_path() {
    // Regression guard: assignment via super (write context) should not
    // emit the read-context TS2340. Other diagnostics may still fire
    // depending on whether the base has a setter, but not TS2340 for
    // the read path.
    let source = r#"
class Base {
  set value(v: number) {}
}

class Derived extends Base {
  override set value(v: number) {
    super.value = v;
  }
}
"#;
    let d = diags(source);
    // The write context bypasses the new read-only check we added.
    // (tsc actually still emits TS2340 here in some cases; we keep the
    //  conservative read-only gate for this slice. If tsc parity needs
    //  the write case too, a follow-up can widen the gate.)
    assert!(
        !has_ts2340(&d),
        "write-context super accessor should not hit the read-only TS2340 path, got: {d:?}",
    );
}
