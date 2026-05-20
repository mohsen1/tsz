//! Coverage for `super.<member>` accessibility checks (TS2340 and TS2855).
//!
//! Structural rules (match `tsc`):
//!
//! **Visibility-driven rule (all targets):** When the receiver of a property access
//! is the `super` keyword:
//! > - If the base-class member is **public** or **protected** (method, accessor,
//! >   or static), the access is valid — no TS2340.
//! > - If the base-class member is **private** (method, accessor, or static),
//! >   emit TS2340: "Only public and protected methods of the base class are
//! >   accessible via the 'super' keyword."  (NOT TS2341, which is the error for
//! >   ordinary `instance.privateMember` accesses.)
//!
//! Note: tsc does NOT emit TS2340 for public/protected super accessor access in
//! any ES target. TS2340 is exclusively a private-member visibility diagnostic.
//!
//! `super.<field>` reads are a separate target-sensitive path: ES5 emits
//! TS2340, while ES2015+ emits TS2855. Those cases are exercised below.

use tsz_checker::test_utils::{
    check_source, check_source_code_messages, diagnostic_code_messages, has_diagnostic_code,
};
use tsz_common::common::ScriptTarget;
use tsz_common::options::checker::CheckerOptions;

const TS2340: u32 = 2340;
const TS2341: u32 = 2341;
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

fn assert_ts2340(source: &str) {
    let d = check_source_code_messages(source);
    assert!(
        has_diagnostic_code(&d, TS2340),
        "expected TS2340 but got: {d:?}"
    );
    // TS2340 and TS2341 must not both fire for the same `super.x` access.
    assert!(
        !has_diagnostic_code(&d, TS2341),
        "TS2341 must not fire alongside TS2340, got: {d:?}"
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
fn super_public_members_in_nested_arrows_no_ts2340() {
    let source = r#"
class User {
    sayHello(): void {}
    get label(): string {
        return "user";
    }
}

class RegisteredUser extends User {
    constructor() {
        super();
        var direct = () => super.sayHello();
        var nested = () => () => () => super.sayHello();
        var superLabel = () => () => () => super.label;
    }
    sayHello(): void {
        var direct = () => super.sayHello();
        var nested = () => () => () => super.sayHello();
        var superLabel = () => () => () => super.label;
    }
}
"#;

    for diagnostics in [check_es5(source), check_es2015(source)] {
        assert!(
            !has_diagnostic_code(&diagnostics, TS2340),
            "public super method/accessor access in nested arrows must not emit TS2340, got: {diagnostics:?}",
        );
        assert!(
            !has_diagnostic_code(&diagnostics, TS2855),
            "public super method/accessor access in nested arrows must not emit TS2855, got: {diagnostics:?}",
        );
    }
}

#[test]
fn super_field_in_nested_arrows_reports_target_specific_primary_diagnostic() {
    let source = r#"
class User {
    name: string = "Bob";
}

class RegisteredUser extends User {
    name: string = "Frank";
    constructor() {
        super();
        var superName = () => () => () => super.name;
    }
    readName(): string {
        var superName = () => () => () => super.name;
        return superName()()();
    }
}
"#;

    let es5 = check_es5(source);
    assert!(
        has_diagnostic_code(&es5, TS2340),
        "ES5 super field access should emit TS2340, got: {es5:?}",
    );
    assert!(
        !has_diagnostic_code(&es5, TS2855),
        "ES5 super field access should not also emit TS2855, got: {es5:?}",
    );

    let es2015 = check_es2015(source);
    assert!(
        !has_diagnostic_code(&es2015, TS2340),
        "ES2015 super field access should not emit TS2340, got: {es2015:?}",
    );
    assert!(
        has_diagnostic_code(&es2015, TS2855),
        "ES2015 super field access should emit TS2855, got: {es2015:?}",
    );
}

#[test]
fn super_in_lambdas_parse_error_does_not_cascade_to_ts2340() {
    let source = r#"
class User {
    sayHello(): void {}
}

class RegisteredUser extends User {
    constructor() {
        super();
        super.sayHello();
        var x = () => super.sayHello();
    }
    sayHello(): void {
        super.sayHello();
        var x = () => super.sayHello();
    }
}

class RegisteredUser2 extends User {
    constructor() {
        super();
        var x = () => () => () => super.sayHello();
    }
    sayHello(): void {
        var x = () => () => () => super.sayHello();
    }
}

class RegisteredUser4 extends User {
    constructor() {
        super();
        var x = () => () => super;
    }
    sayHello(): void {
        var x = () => () => super;
    }
}
"#;

    for diagnostics in [check_es5(source), check_es2015(source)] {
        assert!(
            !has_diagnostic_code(&diagnostics, TS2340),
            "superInLambdas should not cascade to TS2340, got: {diagnostics:?}",
        );
    }
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

// --- ES5 target tests ---
// tsc never emits TS2340 for public/protected super accessor or field access,
// regardless of ES target. Only private member access via super causes TS2340.

#[test]
fn es5_super_method_call_no_ts2340() {
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
fn es5_super_accessor_read_no_ts2340() {
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
        !has_diagnostic_code(&d, TS2340),
        "ES5 public super accessor read must not emit TS2340, got: {d:?}",
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
        "ES5 public super field read should emit TS2340, got: {d:?}",
    );
    assert!(
        !has_diagnostic_code(&d, TS2855),
        "ES5 public super field read should not also emit TS2855, got: {d:?}",
    );
}

#[test]
fn es2015_super_accessor_read_no_ts2340() {
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

// --- Visibility-driven tests (private members via super emit TS2340) ---

#[test]
fn super_private_method_emits_ts2340() {
    assert_ts2340(
        r#"
class Base {
  private greet(): string {
    return "hello";
  }
}

class Derived extends Base {
  greet(): string {
    return super.greet();
  }
}
"#,
    );
}

#[test]
fn super_private_method_renamed_emits_ts2340() {
    assert_ts2340(
        r#"
class Animal {
  private speak(): string {
    return "...";
  }
}

class Dog extends Animal {
  speak(): string {
    return super.speak();
  }
}
"#,
    );
}

#[test]
fn super_private_get_accessor_emits_ts2340() {
    assert_ts2340(
        r#"
class Base {
  private get value(): number {
    return 0;
  }
}

class Derived extends Base {
  get value(): number {
    return super.value + 1;
  }
}
"#,
    );
}

#[test]
fn super_private_set_accessor_emits_ts2340() {
    assert_ts2340(
        r#"
class Base {
  private set count(_v: number) {}
}

class Derived extends Base {
  set count(v: number) {
    super.count = v;
  }
}
"#,
    );
}

#[test]
fn super_private_static_method_emits_ts2340() {
    assert_ts2340(
        r#"
class Base {
  private static factory(): Base {
    return new Base();
  }
}

class Derived extends Base {
  static create(): Derived {
    super.factory();
    return new Derived();
  }
}
"#,
    );
}

#[test]
fn regular_instance_private_access_still_ts2341_not_ts2340() {
    // When the receiver is NOT `super` (e.g. an instance), tsz must emit TS2341,
    // not TS2340. This guards against over-applying the super rule.
    let source = r#"
class Base {
  private secret: number = 42;
}

class Derived extends Base {
  read(b: Base): number {
    return b.secret;
  }
}
"#;
    let d = check_source_code_messages(source);
    assert!(
        !has_diagnostic_code(&d, TS2340),
        "instance access must not emit TS2340, got: {d:?}"
    );
    assert!(
        has_diagnostic_code(&d, TS2341),
        "instance private access must emit TS2341, got: {d:?}"
    );
}
