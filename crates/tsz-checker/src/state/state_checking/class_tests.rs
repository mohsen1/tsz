use crate::context::CheckerOptions;
use crate::test_utils::{check_source, check_source_diagnostics};

fn check_with_declaration(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            emit_declarations: true,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

// TS4094 — property of exported anonymous class type may not be private or protected

#[test]
fn ts4094_export_default_anon_class_private_member() {
    // `export default class { private x() {} }` — tsc emits TS4094 for `x`.
    let codes = check_with_declaration(
        r#"
export default class {
    private x() {}
    protected y() {}
    public z() {}
}
"#,
    );
    assert!(
        codes.contains(&4094),
        "TS4094 expected for private/protected in exported anonymous class, got: {codes:?}"
    );
}

#[test]
fn ts4094_no_error_for_named_export_own_class() {
    // `export class Foo { private x() {} }` — tsc does NOT emit TS4094 because
    // the named class's private members are stripped in the .d.ts.
    let codes = check_with_declaration(
        r#"
export class Foo {
    private x() {}
}
"#,
    );
    assert!(
        !codes.contains(&4094),
        "TS4094 should NOT fire for named exported class with own private members, got: {codes:?}"
    );
}

#[test]
fn ts4094_export_default_mixin_call_anon_class() {
    // `export default mix(AnonClass)` where mix<T>(x:T):T returns the anonymous
    // constructor — tsc emits TS4094 for private/protected members.
    let codes = check_with_declaration(
        r#"
declare function mix<TMix>(mixin: TMix): TMix;
const AnonBase = class {
    protected _onDispose() {}
    private _assertIsStripped() {}
};
export default mix(AnonBase);
"#,
    );
    assert!(
        codes.contains(&4094),
        "TS4094 expected for `export default mix(AnonClass)`, got: {codes:?}"
    );
}

#[test]
fn ts4094_named_class_extending_mixin_anon_class() {
    // `export class Monitor extends mix(AnonBase)` — tsc emits TS4094 at Monitor's
    // name because the anonymous base's private/protected appear in the .d.ts.
    let codes = check_with_declaration(
        r#"
declare function mix<TMix>(mixin: TMix): TMix;
const AnonBase = class {
    protected _onDispose() {}
    private _assertIsStripped() {}
};
export class Monitor extends mix(AnonBase) {
    protected _onDispose() {}
}
"#,
    );
    assert!(
        codes.contains(&4094),
        "TS4094 expected for named exported class extending mixin of anonymous class, got: {codes:?}"
    );
}

#[test]
fn ts4094_exported_function_returning_anon_class_with_private_base() {
    // The inferred return type of an exported function can be an anonymous
    // class constructor whose declaration emit surface includes private
    // members inherited from the base.
    let codes = check_with_declaration(
        r#"
export class Base {
    private property = "";
}

export type Constructor<T> = new (...args: any[]) => T;
export function WithTags<T extends Constructor<Base>>(BaseCtor: T) {
    return class extends BaseCtor {
        tags(): void {}
    };
}

export class Test extends WithTags(Base) {}
"#,
    );
    let ts4094_count = codes.iter().filter(|&&code| code == 4094).count();
    assert!(
        ts4094_count >= 2,
        "TS4094 expected for exported function return and named class heritage, got: {codes:?}"
    );
}

#[test]
fn ts4094_no_error_without_declaration_flag() {
    // Without `declaration: true`, TS4094 should not be emitted.
    let codes: Vec<u32> = check_source_diagnostics(
        r#"
export default class {
    private x() {}
}
"#,
    )
    .iter()
    .map(|d| d.code)
    .collect();
    assert!(
        !codes.contains(&4094),
        "TS4094 should NOT fire without declaration flag, got: {codes:?}"
    );
}

#[test]
fn ts1267_abstract_property_with_initializer() {
    let diags = check_source_diagnostics(
        r#"
abstract class C {
    abstract x: number = 1;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 1267).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS1267, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert!(matching[0].message_text.contains("'x'"));
}

#[test]
fn ts1267_abstract_property_without_initializer_no_error() {
    let diags = check_source_diagnostics(
        r#"
abstract class C {
    abstract x: number;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 1267).collect();
    assert_eq!(matching.len(), 0, "Expected no TS1267, got: {matching:?}");
}

#[test]
fn ts18006_field_named_constructor() {
    let diags = check_source_diagnostics(
        r#"
class C {
    "constructor" = 3;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 18006).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS18006, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2699_static_property_named_prototype() {
    let diags = check_source_diagnostics(
        r#"
class C {
    static prototype: number;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2699).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS2699, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts2699_non_static_prototype_no_error() {
    let diags = check_source_diagnostics(
        r#"
class C {
    prototype: number;
}
"#,
    );
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2699).collect();
    assert_eq!(
        matching.len(),
        0,
        "Expected no TS2699 for non-static prototype, got: {matching:?}"
    );
}

#[test]
fn ts2797_mixin_extending_abstract_type_variable() {
    let diags = check_source_diagnostics(
        r#"
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass) {
    class MixinClass extends baseClass {
    }
    return MixinClass;
}
"#,
    );
    let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2797).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS2797 for mixin extending abstract type variable, got codes: {all_codes:?}"
    );
}

#[test]
fn ts2797_mixin_with_implements_clause() {
    // TS2797 should still fire when the mixin class has an implements clause
    // (previously broken due to name-merging between function Mixin and interface Mixin)
    let diags = check_source_diagnostics(
        r#"
interface Mixin {
    mixinMethod(): void;
}
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass): TBaseClass & (abstract new (...args: any) => Mixin) {
    class MixinClass extends baseClass implements Mixin {
        mixinMethod() {}
    }
    return MixinClass;
}
"#,
    );
    let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2797).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS2797 for mixin with implements clause, got codes: {all_codes:?}"
    );
}

#[test]
fn ts2515_expression_based_heritage() {
    // Non-abstract class extending expression-based heritage (mixin pattern)
    // should report TS2515 for unimplemented abstract members
    let diags = check_source_diagnostics(
        r#"
interface Mixin {
    mixinMethod(): void;
}
function Mixin<TBaseClass extends abstract new (...args: any) => any>(baseClass: TBaseClass): TBaseClass & (abstract new (...args: any) => Mixin) {
    class MixinClass extends baseClass implements Mixin {
        mixinMethod() {}
    }
    return MixinClass;
}

abstract class AbstractBase {
    abstract abstractBaseMethod(): void;
}

const MixedBase = Mixin(AbstractBase);

class DerivedFromAbstract extends MixedBase {
}
"#,
    );
    let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    let ts2515: Vec<_> = diags.iter().filter(|d| d.code == 2515).collect();
    assert_eq!(
        ts2515.len(),
        1,
        "Expected 1 TS2515 for missing abstract member, got codes: {all_codes:?}"
    );
    // Verify the message mentions the correct base class name
    let msg = &ts2515[0].message_text;
    assert!(
        msg.contains("AbstractBase & Mixin"),
        "TS2515 message should reference 'AbstractBase & Mixin', got: {msg}"
    );
}

/// Rule: a get/set accessor pair (two declarations, one member name) is a
/// single inherited abstract member. The missing-member set must dedup by
/// name so the count stays 1 (TS2515 singular), not 2 (TS2654 plural with a
/// duplicated name).
#[test]
fn abstract_get_set_pair_counted_as_one_member_ts2515() {
    let diags = check_source_diagnostics(
        r#"
abstract class A { abstract get x(): number; abstract set x(v: number); }
class B extends A {}
"#,
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2515),
        "Abstract get/set pair missing in subclass must emit singular TS2515, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2654),
        "Abstract get/set pair must not be counted as two members (TS2654), got: {codes:?}"
    );
    let msg = &diags
        .iter()
        .find(|d| d.code == 2515)
        .expect("expected a TS2515 diagnostic")
        .message_text;
    assert!(
        !msg.contains("'x', 'x'") && !msg.contains("x, x"),
        "TS2515 message must name member 'x' once, got: {msg}"
    );
}

/// Same rule, different identifiers — proves the dedup is structural, not
/// keyed to the spelling `x`.
#[test]
fn abstract_get_set_pair_renamed_counted_as_one_member_ts2515() {
    let diags = check_source_diagnostics(
        r#"
abstract class Shape { abstract get area(): number; abstract set area(v: number); }
class Square extends Shape {}
"#,
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2515) && !codes.contains(&2654),
        "Renamed abstract get/set pair must emit singular TS2515, got: {codes:?}"
    );
    let msg = &diags
        .iter()
        .find(|d| d.code == 2515)
        .expect("expected a TS2515 diagnostic")
        .message_text;
    assert!(
        !msg.contains("'area', 'area'") && !msg.contains("area, area"),
        "TS2515 message must name member 'area' once, got: {msg}"
    );
}

/// Generalization: abstract method overload signatures share one member
/// name across two declarations and likewise collapse to a single missing
/// member.
#[test]
fn abstract_method_overload_signatures_counted_as_one_member_ts2515() {
    let diags = check_source_diagnostics(
        r#"
abstract class A {
    abstract f(): void;
    abstract f(x: number): void;
}
class B extends A {}
"#,
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2515) && !codes.contains(&2654),
        "Abstract method overload signatures must collapse to one missing member (TS2515), got: {codes:?}"
    );
}

/// tsc accepts implementing only the getter of an abstract get/set pair.
#[test]
fn abstract_get_set_pair_satisfied_by_getter_only() {
    let diags = check_source_diagnostics(
        r#"
abstract class A { abstract get x(): number; abstract set x(v: number); }
class B extends A { get x(): number { return 0; } }
"#,
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2515) && !codes.contains(&2654),
        "Implementing the getter of an abstract get/set pair must satisfy it, got: {codes:?}"
    );
}

/// Negative control: two genuinely distinct abstract members stay TS2654
/// listing both names.
#[test]
fn two_distinct_abstract_members_stay_ts2654() {
    let diags = check_source_diagnostics(
        r#"
abstract class A { abstract a(): void; abstract b(): void; }
class B extends A {}
"#,
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2654) && !codes.contains(&2515),
        "Two distinct missing abstract members must emit TS2654, got: {codes:?}"
    );
    let msg = &diags
        .iter()
        .find(|d| d.code == 2654)
        .expect("expected a TS2654 diagnostic")
        .message_text;
    assert!(
        msg.contains("'a'") && msg.contains("'b'"),
        "TS2654 must list both 'a' and 'b', got: {msg}"
    );
}

/// Negative control: a single missing abstract method stays TS2515.
#[test]
fn single_abstract_method_stays_ts2515() {
    let diags = check_source_diagnostics(
        r#"
abstract class A { abstract a(): void; }
class B extends A {}
"#,
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2515) && !codes.contains(&2654),
        "A single missing abstract method must emit TS2515, got: {codes:?}"
    );
}

#[test]
fn double_mixin_conditional_type_base_class_has_no_extra_ts2345() {
    let diags = check_source_diagnostics(
        r#"
type Constructor = new (...args: any[]) => {};
declare const Object: Constructor;

const Mixin1 = <C extends Constructor>(Base: C) => class extends Base { private _fooPrivate!: {}; };

type FooConstructor = typeof Mixin1 extends (a: Constructor) => infer Cls ? Cls : never;
const Mixin2 = <C extends FooConstructor>(Base: C) => class extends Base {};

class C extends Mixin2(Mixin1(Object)) {}
"#,
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2345),
        "Expected no TS2345 for double mixin conditional base class, got codes: {codes:?}"
    );
}

#[test]
fn ts2545_mixin_with_optional_rest_parameter() {
    // TS2545: A mixin class must have a constructor with a single rest
    // parameter of type 'any[]'. Optional rest parameters are invalid.
    let diags = check_source_diagnostics(
        r#"
type Constructor<T = {}> = new (...args?: any[]) => T;

function Mixin<TBase extends Constructor>(Base: TBase) {
    return class extends Base {
    };
}
"#,
    );
    let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2545).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected 1 TS2545 for optional rest param in mixin constructor, got codes: {all_codes:?}"
    );
}

#[test]
fn ts2545_no_error_for_valid_mixin_constructor() {
    // Valid mixin pattern: `...args: any[]` without optional should NOT emit TS2545.
    let diags = check_source_diagnostics(
        r#"
type Constructor = new (...args: any[]) => {};

function Mixin<TBase extends Constructor>(Base: TBase) {
    return class extends Base {};
}
"#,
    );
    let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        !all_codes.contains(&2545),
        "Should NOT emit TS2545 for valid mixin constructor pattern, got codes: {all_codes:?}"
    );
}

#[test]
fn ts2545_no_error_for_bare_any_rest_parameter() {
    // `...args: any` (bare any, not any[]) is also valid for mixin constructors.
    let diags = check_source_diagnostics(
        r#"
function Mixin<TBase extends abstract new (...args: any) => any>(baseClass: TBase) {
    abstract class MixinClass extends baseClass {}
    return MixinClass;
}
"#,
    );
    let all_codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert!(
        !all_codes.contains(&2545),
        "Should NOT emit TS2545 for bare `any` rest param type, got codes: {all_codes:?}"
    );
}
