//! Coverage for `super.<member>` accessibility checks (TS2340/TS2341/TS2855).
//!
//! Structural rules (each shape byte-verified against `tsc` 6.0.2; the
//! diagnostics come from `checkPropertyAccessibilityAtLocation`, whose
//! `isSuper` gates run BEFORE the visibility check):
//!
//! **ES5 target:** a super member backed by any non-*method* declaration —
//! plain field, get/set accessor, auto-accessor field; instance or static;
//! ANY visibility (public, protected, private) — emits TS2340: "Only public
//! and protected methods of the base class are accessible via the 'super'
//! keyword." Private *methods* are the only private members that reach the
//! visibility check, and they emit TS2341 exactly like an ordinary
//! `instance.x` access.
//!
//! **ES2015 and later:** TS2340 never fires. A parent **instance** field via
//! super emits TS2855 (any visibility; statics and static contexts are
//! exempt — `super` there is the parent constructor object). A private
//! member that passes that gate (method, accessor, or static) emits TS2341.
//! Public/protected methods, accessors, statics, and auto-accessor fields
//! are allowed.

use tsz_binder::BinderState;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::{
    check_source, check_source_code_messages, diagnostic_code_messages, has_diagnostic_code,
};
use tsz_common::common::ScriptTarget;
use tsz_common::options::checker::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

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

fn check_es5_with_parse_flags(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let parse_diagnostics = parser.get_diagnostics().to_vec();
    assert!(
        !parse_diagnostics.is_empty(),
        "test fixture must contain syntax diagnostics",
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES5,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.has_parse_errors = true;
    checker.ctx.has_syntax_parse_errors = true;
    checker.ctx.all_parse_error_positions =
        parse_diagnostics.iter().map(|diag| diag.start).collect();
    checker.ctx.syntax_parse_error_positions = checker.ctx.all_parse_error_positions.clone();
    checker.check_source_file(root);
    diagnostic_code_messages(checker.ctx.diagnostics.clone())
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

/// Private member via super at a modern target: TS2341, never TS2340.
fn assert_ts2341_not_ts2340(d: &[(u32, String)]) {
    assert!(
        has_diagnostic_code(d, TS2341),
        "expected TS2341 but got: {d:?}"
    );
    assert!(
        !has_diagnostic_code(d, TS2340),
        "TS2340 must not fire for a private member at ES2015+, got: {d:?}"
    );
}

/// ES5 non-method member via super: TS2340 alone — the visibility check is
/// never reached, so TS2341 must not stack on top.
fn assert_ts2340_not_ts2341(d: &[(u32, String)]) {
    assert!(
        has_diagnostic_code(d, TS2340),
        "expected TS2340 but got: {d:?}"
    );
    assert!(
        !has_diagnostic_code(d, TS2341),
        "TS2341 must not fire alongside TS2340, got: {d:?}"
    );
}

// --- Public/protected accessor access via super is legal at modern targets ---

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
fn super_public_methods_in_nested_arrows_no_ts2340() {
    let source = r#"
class User {
    sayHello(): void {}
}

class RegisteredUser extends User {
    constructor() {
        super();
        var direct = () => super.sayHello();
        var nested = () => () => () => super.sayHello();
    }
    sayHello(): void {
        var direct = () => super.sayHello();
        var nested = () => () => () => super.sayHello();
    }
}
"#;

    for diagnostics in [check_es5(source), check_es2015(source)] {
        assert!(
            !has_diagnostic_code(&diagnostics, TS2340),
            "public super method access in nested arrows must not emit TS2340, got: {diagnostics:?}",
        );
        assert!(
            !has_diagnostic_code(&diagnostics, TS2855),
            "public super method access in nested arrows must not emit TS2855, got: {diagnostics:?}",
        );
    }
}

#[test]
fn super_public_accessor_in_nested_arrows_target_split() {
    // `super.label` (a public get accessor) through nested arrows: legal at
    // ES2015+, TS2340 at ES5 — the arrow nesting must not change the answer.
    let source = r#"
class User {
    get label(): string {
        return "user";
    }
}

class RegisteredUser extends User {
    describe(): string {
        var superLabel = () => () => super.label;
        return superLabel()();
    }
}
"#;

    let es5 = check_es5(source);
    assert_ts2340_not_ts2341(&es5);
    let es2015 = check_es2015(source);
    assert!(
        !has_diagnostic_code(&es2015, TS2340) && !has_diagnostic_code(&es2015, TS2855),
        "ES2015 public super accessor in nested arrows must be legal, got: {es2015:?}",
    );
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
    name: string = "Bob";
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

class RegisteredUser3 extends User {
    constructor() {
        super();
        var superName = () => () => () => super.name;
    }
    sayHello(): void {
        var superName = () => () => () => super.name;
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

    for diagnostics in [check_es5_with_parse_flags(source), check_es2015(source)] {
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
fn super_auto_accessor_read_allowed_at_modern_target() {
    // `accessor` fields are prototype accessors, not instance fields:
    // tsc's `isClassInstanceProperty` excludes them from TS2855, and they
    // are public here, so `super.a` is legal.
    let source = r#"
class Base {
  accessor a = 1;
}
class Derived extends Base {
  read(): number {
    return super.a;
  }
}
"#;
    let d = check_source_code_messages(source);
    for code in [TS2340, TS2341, TS2855] {
        assert!(
            !has_diagnostic_code(&d, code),
            "super.<auto-accessor> must be legal at ES2022, got: {d:?}",
        );
    }
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

// --- ES5 target: TS2340 for every non-method member via super ---

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
fn es5_super_public_accessor_read_emits_ts2340() {
    // Unlike ES2015+, ES5 rejects accessors via super regardless of
    // visibility: only *methods* can be dispatched through ES5 super emit.
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
    assert_ts2340_not_ts2341(&d);
    assert!(
        !has_diagnostic_code(&d, TS2855),
        "ES5 super accessor access must not emit TS2855, got: {d:?}",
    );
}

#[test]
fn es5_super_private_accessor_read_emits_ts2340_not_ts2341() {
    // The ES5 non-method gate fires before the visibility check, so a
    // private accessor gets TS2340, not TS2341.
    let d = check_es5(
        r#"
class Base {
  private get value(): number {
    return 0;
  }
}

class Derived extends Base {
  read(): number {
    return super.value + 1;
  }
}
"#,
    );
    assert_ts2340_not_ts2341(&d);
}

#[test]
fn es5_super_private_method_call_emits_ts2341_not_ts2340() {
    // Methods pass the ES5 non-method gate even when private; the ordinary
    // visibility check then reports TS2341.
    let d = check_es5(
        r#"
class Base {
  private greet(): string {
    return "hello";
  }
}

class Derived extends Base {
  call(): string {
    return super.greet();
  }
}
"#,
    );
    assert_ts2341_not_ts2340(&d);
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
fn es5_super_static_field_in_static_method_emits_ts2340() {
    // In a static member, `super` is the parent constructor object; a static
    // *field* reached through it is still a non-method declaration, so the
    // ES5 gate rejects it. (ES2015+ allows it — see the es2015 test below.)
    let source = r#"
class Base {
  static sf = 1;
}
class Derived extends Base {
  static m(): number {
    return super.sf;
  }
}
"#;
    let es5 = check_es5(source);
    assert_ts2340_not_ts2341(&es5);
    assert!(
        !has_diagnostic_code(&es5, TS2855),
        "static super field access must not emit TS2855, got: {es5:?}",
    );

    let es2015 = check_es2015(source);
    for code in [TS2340, TS2341, TS2855] {
        assert!(
            !has_diagnostic_code(&es2015, code),
            "ES2015 public static super field access must be legal, got: {es2015:?}",
        );
    }
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

// --- Private members via super at modern targets: TS2341 (or TS2855 for fields) ---

#[test]
fn super_private_method_emits_ts2341() {
    let d = check_source_code_messages(
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
    assert_ts2341_not_ts2340(&d);
}

#[test]
fn super_private_method_renamed_emits_ts2341() {
    let d = check_source_code_messages(
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
    assert_ts2341_not_ts2340(&d);
}

#[test]
fn super_private_get_accessor_emits_ts2341() {
    let d = check_source_code_messages(
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
    assert_ts2341_not_ts2340(&d);
}

#[test]
fn super_private_set_accessor_emits_ts2341() {
    let d = check_source_code_messages(
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
    assert_ts2341_not_ts2340(&d);
}

#[test]
fn super_private_static_method_emits_ts2341() {
    let d = check_source_code_messages(
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
    assert_ts2341_not_ts2340(&d);
}

#[test]
fn super_private_static_field_emits_ts2341() {
    // A private *static* field via super is exempt from TS2855 (not an
    // instance field), so the visibility check reports TS2341.
    let d = check_es2015(
        r#"
class Base {
  private static sf = 1;
}
class Derived extends Base {
  static m(): number {
    return super.sf;
  }
}
"#,
    );
    assert_ts2341_not_ts2340(&d);
    assert!(
        !has_diagnostic_code(&d, TS2855),
        "static super field access must not emit TS2855, got: {d:?}",
    );
}

#[test]
fn super_private_instance_field_emits_ts2855_not_ts2341() {
    // The instance-field-via-super gate runs before the visibility check, so
    // a private parent field gets TS2855 — never TS2341, never TS2340 — at
    // ES2015+.
    let d = check_es2015(
        r#"
class Base {
  private secret: number = 42;
}
class Derived extends Base {
  read(): number {
    return super.secret;
  }
}
"#,
    );
    assert!(
        has_diagnostic_code(&d, TS2855),
        "private super instance field should emit TS2855, got: {d:?}",
    );
    assert!(
        !has_diagnostic_code(&d, TS2341) && !has_diagnostic_code(&d, TS2340),
        "TS2341/TS2340 must not fire for a private instance field via super at ES2015+, got: {d:?}",
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
