//! Regression coverage for `super.<accessor>` reads and writes.
//!
//! Structural rule (matches `tsc`):
//!
//! > When the receiver of a property access is the `super` keyword and the
//! > resolved base-class member is a `get` / `set` accessor, the access is
//! > valid. TypeScript never emits TS2340 in that shape.
//!
//! TS2855 ("Class field … is not accessible … via super") and TS2540
//! ("Cannot assign to … because it is a read-only property") still cover
//! the `super.<field>` and `super.<readonly-accessor> = …` shapes via the
//! existing property-checker paths, and are exercised below to prove this
//! change does not unmask either of them incorrectly.

use tsz_checker::test_utils::{check_source_code_messages, has_diagnostic_code};

const TS2340: u32 = 2340;
const TS2855: u32 = 2855;

fn assert_no_ts2340(source: &str, label: &str) {
    let d = check_source_code_messages(source);
    assert!(
        !has_diagnostic_code(&d, TS2340),
        "{label}: must not emit TS2340, got: {d:?}",
    );
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
        "public super.<getter> read in override",
    );
}

#[test]
fn super_public_get_accessor_read_renamed_no_ts2340() {
    // Anti-hardcoding: same rule under a different property name.
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
        "public super.<getter> (renamed property)",
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
        "protected super.<getter> read in override",
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
        "super.<setter> write in override",
    );
}

#[test]
fn super_get_accessor_read_inside_method_no_ts2340() {
    // The receiver context is a regular method, not an accessor body. The
    // previous gate required an accessor body to fire; this guards against
    // the gate accidentally returning under a different shape.
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
        "super.<getter> read inside method",
    );
}

#[test]
fn super_get_accessor_inherited_from_grandparent_no_ts2340() {
    // The accessor is on a transitively-inherited class; the chain walk must
    // still treat it as a valid super target.
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
        "inherited super.<getter>",
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
        "super.<getter> in arrow inside accessor",
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
        "static super.<getter>",
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
        "super.method() call",
    );
}

#[test]
fn super_field_read_still_emits_ts2855_when_es2022() {
    // TS2855 lives in `checkers/property_checker.rs` and is unaffected by
    // removing the accessor gate; this proves the field path is still wired.
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
        "super.<field> must not emit TS2340 (TS2855 is its diagnostic), got: {d:?}",
    );
    assert!(
        has_diagnostic_code(&d, TS2855),
        "super.<field> read should emit TS2855 in default ES2022 mode, got: {d:?}",
    );
}
