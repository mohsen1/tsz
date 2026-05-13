//! Tests for `super.<accessor>` reads.
//!
//! Regression coverage for #6481: public and protected accessors are valid
//! `super` property targets and must not be rejected with TS2340.

fn diags(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn has_ts2340(diags: &[(u32, String)]) -> bool {
    diags
        .iter()
        .any(|(c, m)| *c == 2340 && m.contains("methods") && m.contains("'super'"))
}

#[test]
fn super_public_get_accessor_read_no_ts2340() {
    // Direct repro from #6481.
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
        !has_ts2340(&d),
        "public super.<getter> must not emit TS2340, got: {d:?}",
    );
}

#[test]
fn super_public_get_accessor_read_different_name_no_ts2340() {
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
        !has_ts2340(&d),
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
        !has_ts2340(&d),
        "protected super.<getter> must not emit TS2340, got: {d:?}",
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
