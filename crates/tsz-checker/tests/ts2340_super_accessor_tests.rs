//! Regression coverage for `super.<accessor>` reads and writes.
//!
//! Structural rule (matches `tsc`):
//!
//! > When the receiver of a property access is the `super` keyword and the
//! > resolved base-class member is a `get` / `set` accessor, the access is
//! > valid. TypeScript never emits TS2340 in that shape — TS2340 is for
//! > unrelated invalid `super` access shapes that no longer apply to
//! > accessors.
//!
//! tsz previously emitted TS2340 for `super.<getter>` reads inside an
//! overriding accessor (introduced by #6170 to "close" #5995, which
//! misreported tsc's behavior). That false positive is reported by #6481
//! and #6665 and is removed by deleting the accessor-only TS2340 gate.
//!
//! TS2855 ("Class field … is not accessible … via super") and TS2540
//! ("Cannot assign to … because it is a read-only property") still cover
//! the `super.<field>` and `super.<readonly-accessor> = …` shapes via the
//! existing property-checker paths and are exercised below to prove this
//! change does not unmask either of them incorrectly.

fn diags(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn has_code(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _)| *c == code)
}

#[test]
fn super_public_get_accessor_read_no_ts2340() {
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
        !has_code(&d, 2340),
        "public super.<getter> must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_public_get_accessor_read_renamed_no_ts2340() {
    // Anti-hardcoding: same rule under a different property name.
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
        !has_code(&d, 2340),
        "public super.<getter> (name 'size') must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_protected_get_accessor_read_no_ts2340() {
    let source = r#"
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
"#;
    let d = diags(source);
    assert!(
        !has_code(&d, 2340),
        "protected super.<getter> must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_set_accessor_write_no_ts2340() {
    // super.<setter> = x via an overriding setter — tsc accepts this.
    let source = r#"
class Base {
  set value(_v: number) {}
}

class Derived extends Base {
  override set value(v: number) {
    super.value = v / 2;
  }
}
"#;
    let d = diags(source);
    assert!(
        !has_code(&d, 2340),
        "super.<setter> write must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_get_accessor_read_inside_method_no_ts2340() {
    // The receiver context is a regular method (not an accessor body) —
    // tsc still allows the access. The previous gate required the access
    // to live inside an accessor body, so this also guards against the
    // gate accidentally returning.
    let source = r#"
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
"#;
    let d = diags(source);
    assert!(
        !has_code(&d, 2340),
        "super.<getter> inside a method must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_get_accessor_inherited_from_grandparent_no_ts2340() {
    // The accessor is on a transitively-inherited class. The chain walk
    // must still treat it as a valid super target.
    let source = r#"
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
"#;
    let d = diags(source);
    assert!(
        !has_code(&d, 2340),
        "inherited super.<getter> must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_get_accessor_in_arrow_inside_accessor_no_ts2340() {
    // Lexical `super` inside an arrow body still binds to the enclosing
    // accessor's home object. tsc accepts this, and so must we.
    let source = r#"
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
"#;
    let d = diags(source);
    assert!(
        !has_code(&d, 2340),
        "super.<getter> in arrow inside accessor must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_static_get_accessor_read_no_ts2340() {
    // Static super accessor — also accepted by tsc.
    let source = r#"
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
"#;
    let d = diags(source);
    assert!(
        !has_code(&d, 2340),
        "static super.<getter> must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_method_call_no_ts2340() {
    // Regression guard: regular method access via super remains OK.
    let source = r#"
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
"#;
    let d = diags(source);
    assert!(
        !has_code(&d, 2340),
        "super.method() must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_field_read_still_emits_ts2855_when_es2022() {
    // The TS2855 path lives in `checkers/property_checker.rs` and is
    // unaffected by removing the accessor gate. Keep it covered.
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
    let d = diags(source);
    assert!(
        !has_code(&d, 2340),
        "super.<field> must not emit TS2340 (TS2855 is its diagnostic), got: {d:?}",
    );
    assert!(
        has_code(&d, 2855),
        "super.<field> read should emit TS2855 in default ES2022 mode, got: {d:?}",
    );
}
