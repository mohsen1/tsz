//! Tests for TS2320: Interface inherits conflicting declarations from base types.

use crate::test_utils::check_source_codes;

fn has_error(source: &str, code: u32) -> bool {
    check_source_codes(source).contains(&code)
}

#[test]
fn ts2320_different_optionality() {
    // interface C has x?: number, interface C2 has x: number
    // interface A extends C, C2 should get TS2320
    let source = r#"
interface C {
    x?: number;
}
interface C2 {
    x: number;
}
interface A extends C, C2 {
    y: string;
}
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for different optionality"
    );
}

#[test]
fn ts2320_incompatible_types() {
    // Classic TS2320: same property name, incompatible types
    let source = r#"
interface Mover {
    move(): void;
    getStatus(): { speed: number; };
}
interface Shaker {
    shake(): void;
    getStatus(): { frequency: number; };
}
interface MoverShaker extends Mover, Shaker {
}
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for incompatible types"
    );
}

#[test]
fn ts2320_compatible_override_no_error() {
    // When the derived interface provides a compatible override, no TS2320
    let source = r#"
interface Mover {
    getStatus(): { speed: number; };
}
interface Shaker {
    getStatus(): { frequency: number; };
}
interface MoverShaker extends Mover, Shaker {
    getStatus(): { speed: number; frequency: number; };
}
"#;
    // TS2320 should still fire because the *inherited* members conflict,
    // even though the override resolves it. tsc reports TS2320 here.
    // Actually tsc does NOT report TS2320 when an override is provided.
    assert!(
        !has_error(source, 2320),
        "Should not get TS2320 when override resolves conflict"
    );
}

#[test]
fn ts2320_same_optionality_no_error() {
    // Both optional — no conflict
    let source = r#"
interface A {
    x?: number;
}
interface B {
    x?: number;
}
interface C extends A, B {}
"#;
    assert!(
        !has_error(source, 2320),
        "Should not get TS2320 when both are optional with same type"
    );
}

#[test]
fn ts2320_same_name_generic_base() {
    // extends A<string>, A<number> — same base name "A" but different type args
    // must still detect the conflict on property "x"
    let source = r#"
interface A<T> {
    x: T;
}
interface C extends A<string>, A<number> { }
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for extends A<string>, A<number> with conflicting 'x'"
    );
}

#[test]
fn ts2320_same_name_generic_base_compatible() {
    // extends A<string>, A<string> — same base, same type arg, no conflict
    let source = r#"
interface A<T> {
    x: T;
}
interface C extends A<string>, A<string> { }
"#;
    assert!(
        !has_error(source, 2320),
        "Should not get TS2320 when same generic base with identical type args"
    );
}

#[test]
fn ts2320_inherited_members_through_chain() {
    // B inherits m:string from A, D inherits m:number from C.
    // E extends B and D — conflict on inherited member "m".
    let source = r#"
interface A {
    m: string;
}
interface B extends A { }
interface C {
    m: number;
}
interface D extends C { }
interface E extends B, D { }
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for inherited member conflict through interface chain"
    );
}

#[test]
fn ts2320_merged_interface_cross_declaration() {
    // Merged interface: two declarations of E, each extending a different base
    // with conflicting inherited members.
    let source = r#"
interface A {
    m: string;
}
interface B extends A { }
interface C {
    m: number;
}
interface D extends C { }
interface E extends B { }
interface E extends D { }
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for cross-declaration heritage with conflicting inherited members"
    );
}

#[test]
fn ts2320_inherited_compatible_no_error() {
    // B inherits x:number from A, D inherits x:number from C — same type, no conflict
    let source = r#"
interface A {
    x: number;
}
interface B extends A { }
interface C {
    x: number;
}
interface D extends C { }
interface E extends B, D { }
"#;
    assert!(
        !has_error(source, 2320),
        "Should not get TS2320 when inherited members have compatible types"
    );
}

#[test]
fn ts2320_derived_member_shadows_inherited() {
    // When E declares "m" itself, it shadows the inherited members — no TS2320
    let source = r#"
interface A {
    m: string;
}
interface B extends A { }
interface C {
    m: number;
}
interface D extends C { }
interface E extends B, D {
    m: string;
}
"#;
    assert!(
        !has_error(source, 2320),
        "Should not get TS2320 when derived interface overrides the conflicting member"
    );
}

#[test]
fn ts2320_class_bases_public_member_conflict() {
    // Two class bases with incompatible public properties
    let source = r#"
class D2 { a!: number; }
class E2 { a!: string; }
interface F2 extends E2, D2 { }
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for class bases with conflicting public member types"
    );
}

#[test]
fn ts2320_class_bases_compatible_no_error() {
    // Two class bases with compatible public properties — no error
    let source = r#"
class A { x!: number; }
class B { x!: number; }
interface C extends A, B { }
"#;
    assert!(
        !has_error(source, 2320),
        "Should not get TS2320 when class bases have compatible member types"
    );
}

#[test]
fn ts2320_class_bases_visibility_conflict() {
    // One class has public x, another has private x — visibility conflict
    let source = r#"
class C {
    public x!: number;
}
class C2 {
    private x!: number;
}
interface A extends C, C2 {
    y: string;
}
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for visibility conflict (public vs private)"
    );
}

#[test]
fn ts2320_generic_class_bases_conflict() {
    // Generic class bases with different type args causing member conflict
    let source = r#"
class C<T> { a!: T; }
class C3<T> { c!: T; }
class C4<T> { d!: T; }
interface A<T> extends C<string>, C3<string> {
    y: T;
}
interface A<T> extends C<number>, C4<string> {
    z: T;
}
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for generic class bases C<string> vs C<number>"
    );
}

#[test]
fn ts2320_class_and_interface_base_conflict() {
    // One class base and one interface base with conflicting members
    let source = r#"
class Mover {
    getStatus(): { speed: number; } { return { speed: 0 }; }
}
interface Shaker {
    getStatus(): { frequency: number; };
}
interface MoverShaker extends Mover, Shaker { }
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for class + interface bases with conflicting members"
    );
}

/// Reproduces `conformance/es6/Symbols/symbolProperty35.ts`.
///
/// `[Symbol.toStringTag]()` in two different bases with different return
/// types is a TS2320 conflict. The cross-base comparison has to canonicalize
/// the computed-property key (`[Symbol.toStringTag]`) so keys from both
/// bases match; previously we bailed on computed keys and missed the
/// conflict entirely.
#[test]
fn ts2320_well_known_symbol_method_conflict_across_bases() {
    let source = r#"
interface I1 {
    [Symbol.toStringTag](): { x: string }
}
interface I2 {
    [Symbol.toStringTag](): { x: number }
}

interface I3 extends I1, I2 { }
"#;
    assert!(
        has_error(source, 2320),
        "Expected TS2320 for `[Symbol.toStringTag]` methods with conflicting return types across bases"
    );
}

/// Regression: two bases declaring `[Symbol.iterator]` with the *same*
/// return type must NOT produce TS2320 (no conflict).
#[test]
fn ts2320_no_false_positive_for_matching_well_known_symbol_method() {
    let source = r#"
interface I1 {
    [Symbol.iterator](): { next(): { value: number; done: boolean } }
}
interface I2 {
    [Symbol.iterator](): { next(): { value: number; done: boolean } }
}

interface I3 extends I1, I2 { }
"#;
    assert!(
        !has_error(source, 2320),
        "Did not expect TS2320 when two bases declare matching `[Symbol.iterator]` methods"
    );
}
