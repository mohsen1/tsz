//! Regression coverage for `super.<accessor>` reads and writes.
//!
//! Structural rule (matches `tsc`):
//!
//! > When the receiver of a property access is the `super` keyword, target
//! > ES5 only permits base-class methods. Accessors remain valid in ES2015+
//! > targets, but emit TS2340 under ES5.
//!
//! `super.<field>` reads still emit TS2855 via the separate field path,
//! which is exercised below.

use tsz_checker::test_utils::{
    check_source, check_source_code_messages, diagnostic_code_messages, has_diagnostic_code,
};
use tsz_common::common::ScriptTarget;
use tsz_common::options::checker::CheckerOptions;

const TS2340: u32 = 2340;
const TS2855: u32 = 2855;

fn assert_no_ts2340(source: &str) {
    let d = check_source_code_messages(source);
    assert!(!has_diagnostic_code(&d, TS2340), "got: {d:?}");
}

fn check_es5(source: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES5,
            ..CheckerOptions::default()
        },
    ))
}

fn check_es2015(source: &str) -> Vec<(u32, String)> {
    diagnostic_code_messages(check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    ))
}

#[test]
fn super_public_get_accessor_read_no_ts2340() {
    assert_no_ts2340(
        r#"
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
"#,
    );
}

#[test]
fn super_public_get_accessor_read_renamed_no_ts2340() {
    assert_no_ts2340(
        r#"
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
"#,
    );
}

#[test]
fn super_protected_get_accessor_read_no_ts2340() {
    assert_no_ts2340(
        r#"
class Base {
  protected get value(): number {
    return 0;
  }
}

class Derived extends Base {
  protected override get value(): number {
    return super.value + 1;
  }
}
"#,
    );
}

#[test]
fn super_set_accessor_write_no_ts2340() {
    assert_no_ts2340(
        r#"
class Base {
  set value(_v: number) {}
}

class Derived extends Base {
  override set value(v: number) {
    super.value = v / 2;
  }
}
"#,
    );
}

#[test]
fn super_get_accessor_read_inside_method_no_ts2340() {
    assert_no_ts2340(
        r#"
class Base {
  get x(): number {
    return 1;
  }
}

class Derived extends Base {
  read(): number {
    return super.x + 1;
  }
}
"#,
    );
}

#[test]
fn super_get_accessor_inherited_from_grandparent_no_ts2340() {
    // Transitive inheritance: the chain walk must reach grandparent accessors.
    assert_no_ts2340(
        r#"
class Grand {
  get gp(): number {
    return 1;
  }
}
class Mid extends Grand {}
class Leaf extends Mid {
  override get gp(): number {
    return super.gp + 1;
  }
}
"#,
    );
}

#[test]
fn super_get_accessor_in_arrow_inside_accessor_no_ts2340() {
    // Lexical `super` inside an arrow body binds to the enclosing accessor's
    // home object; tsc accepts this, and so must we.
    assert_no_ts2340(
        r#"
class Base {
  get x(): number {
    return 1;
  }
}

class Derived extends Base {
  override get x(): number {
    const f = (): number => super.x + 1;
    return f();
  }
}
"#,
    );
}

#[test]
fn super_static_get_accessor_read_no_ts2340() {
    assert_no_ts2340(
        r#"
class Base {
  static get s(): number {
    return 1;
  }
}

class Derived extends Base {
  static override get s(): number {
    return super.s + 1;
  }
}
"#,
    );
}

#[test]
fn super_method_call_no_ts2340() {
    assert_no_ts2340(
        r#"
class Base {
  greet(): string {
    return "hello";
  }
}

class Derived extends Base {
  override greet(): string {
    return super.greet() + " world";
  }
}
"#,
    );
}

#[test]
fn super_field_read_still_emits_ts2855_when_es2022() {
    let source = r#"
class Base {
  field: number = 0;
}
class Derived extends Base {
  read(): number {
    return super.field;
  }
}
"#;
    let d = check_source_code_messages(source);
    assert!(
        !has_diagnostic_code(&d, TS2340),
        "super.<field> must not emit TS2340, got: {d:?}",
    );
    assert!(
        has_diagnostic_code(&d, TS2855),
        "super.<field> read should emit TS2855 in default ES2022 mode, got: {d:?}",
    );
}

#[test]
fn es5_super_get_accessor_read_emits_ts2340() {
    let d = check_es5(
        r#"
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
"#,
    );
    assert!(
        has_diagnostic_code(&d, TS2340),
        "ES5 super accessor read should emit TS2340, got: {d:?}",
    );
}

#[test]
fn es5_super_set_accessor_write_emits_ts2340() {
    let d = check_es5(
        r#"
class Base {
  set value(_v: number) {}
}

class Derived extends Base {
  override set value(v: number) {
    super.value = v / 2;
  }
}
"#,
    );
    assert!(
        has_diagnostic_code(&d, TS2340),
        "ES5 super accessor write should emit TS2340, got: {d:?}",
    );
}

#[test]
fn es5_super_accessor_read_inside_method_emits_ts2340() {
    let d = check_es5(
        r#"
class Base {
  get x(): number {
    return 1;
  }
}

class Derived extends Base {
  read(): number {
    return super.x + 1;
  }
}
"#,
    );
    assert!(
        has_diagnostic_code(&d, TS2340),
        "ES5 super accessor read inside method should emit TS2340, got: {d:?}",
    );
}

#[test]
fn es5_super_static_accessor_read_emits_ts2340() {
    let d = check_es5(
        r#"
class Base {
  static get s(): number {
    return 1;
  }
}

class Derived extends Base {
  static read(): number {
    return super.s + 1;
  }
}
"#,
    );
    assert!(
        has_diagnostic_code(&d, TS2340),
        "ES5 static super accessor read should emit TS2340, got: {d:?}",
    );
}

#[test]
fn es5_super_method_call_still_no_ts2340() {
    let d = check_es5(
        r#"
class Base {
  greet(): string {
    return "hello";
  }
}

class Derived extends Base {
  override greet(): string {
    return super.greet() + " world";
  }
}
"#,
    );
    assert!(
        !has_diagnostic_code(&d, TS2340),
        "ES5 super method call should not emit TS2340, got: {d:?}",
    );
}

#[test]
fn es5_super_field_read_emits_ts2340() {
    let d = check_es5(
        r#"
class Base {
  field: number = 0;
}
class Derived extends Base {
  read(): number {
    return super.field;
  }
}
"#,
    );
    assert!(
        has_diagnostic_code(&d, TS2340),
        "ES5 super field read should emit TS2340, got: {d:?}",
    );
}

#[test]
fn es2015_super_accessor_read_still_no_ts2340() {
    let d = check_es2015(
        r#"
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
"#,
    );
    assert!(
        !has_diagnostic_code(&d, TS2340),
        "ES2015 super accessor read should not emit TS2340, got: {d:?}",
    );
}
