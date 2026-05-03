//! Tests for TS2430: Interface incorrectly extends interface
//!
//! Verifies correct behavior for interface extension compatibility,
//! including scope-aware type resolution in ambient modules.

use crate::test_utils::check_source_code_messages;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

// =========================================================================
// Basic TS2430: interface incorrectly extends interface
// =========================================================================

#[test]
fn test_basic_incompatible_property_type() {
    // Derived interface has incompatible property type
    let source = r#"
interface Base {
    x: number;
}
interface Derived extends Base {
    x: string;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when derived property type is incompatible"
    );
}

#[test]
fn test_compatible_extension_no_error() {
    // Derived interface has compatible (same) property type
    let source = r#"
interface Base {
    x: number;
}
interface Derived extends Base {
    x: number;
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 when derived property type is compatible"
    );
}

// =========================================================================
// Module scope: same-name interface should not cause false TS2430
// =========================================================================

#[test]
fn test_module_namespace_same_name_interface_no_false_positive() {
    // This is the pattern from react16.d.ts that caused false TS2430.
    // In the react16.d.ts structure:
    //   declare module "react" {
    //     type NativeClipboardEvent = ClipboardEvent;  // at module level
    //     namespace React {
    //       interface ClipboardEvent<T> extends SyntheticEvent<T> { ... } // in namespace
    //     }
    //   }
    // The type alias at module level should resolve `ClipboardEvent` to the
    // global one, NOT the namespace-scoped one. The flat file_locals lookup
    // incorrectly shadowed the global with the namespace-local symbol.
    let source = r#"
interface Event {
    type: string;
}
interface ClipboardEvent extends Event {
    clipboardData: any;
}

declare module "mylib" {
    type NativeClipboardEvent = ClipboardEvent;

    namespace MyLib {
        interface BaseSyntheticEvent {
            nativeEvent: Event;
        }

        interface ClipboardEvent<T> extends BaseSyntheticEvent {
            nativeEvent: NativeClipboardEvent;
        }
    }
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 for namespace-scoped interface with same name as global; \
         type alias at module level should resolve to global ClipboardEvent. Got errors: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_module_scoped_genuinely_incompatible() {
    // Inside a module, a genuinely incompatible extension should still error.
    let source = r#"
declare module "mylib" {
    interface Base {
        x: number;
    }
    interface Derived extends Base {
        x: string;
    }
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 for genuinely incompatible extension inside module"
    );
}

// =========================================================================
// Type alias base: interface extends type alias
// =========================================================================

#[test]
fn test_interface_extends_type_alias_incompatible() {
    // Interface extends a type alias with incompatible property type
    let source = r#"
type T1 = { a: number };
interface I1 extends T1 {
    a: string;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when interface extends type alias with incompatible property. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_interface_extends_type_alias_compatible() {
    // Interface extends a type alias with compatible property type — no error
    let source = r#"
type T1 = { a: number };
interface I1 extends T1 {
    a: number;
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 when interface extends type alias with compatible property. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_interface_extends_intersection_type_alias_incompatible() {
    // Interface extends an intersection type alias with incompatible property
    let source = r#"
type T1 = { a: number };
type T2 = T1 & { b: number };
interface I2 extends T2 {
    b: string;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when interface extends intersection type alias with incompatible property. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_interface_extends_mapped_type_alias_incompatible() {
    // Interface extends a mapped type alias with incompatible property
    let source = r#"
type T5 = { [P in 'a' | 'b' | 'c']: string };
interface I5 extends T5 {
    c: number;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when interface extends mapped type alias with incompatible property. Got: {:?}",
        get_diagnostics(source)
    );
}

#[test]
fn test_multi_base_both_incompatible_emits_two_ts2430() {
    // When interface extends multiple bases and BOTH are incompatible,
    // we must emit TS2430 for each incompatible base (not just the first).
    let source = r#"
interface Base1 {
    x: { a: string; }
}
interface Base2 {
    x: { b: string; }
}
interface Derived<T> extends Base1, Base2 {
    x: { a: T; b: T; }
}
"#;
    let diags = get_diagnostics(source);
    let ts2430_count = diags.iter().filter(|d| d.0 == 2430).count();
    assert_eq!(
        ts2430_count, 2,
        "Should emit TS2430 for BOTH incompatible bases, not just the first. Got: {diags:?}"
    );
}

#[test]
fn test_multi_base_one_compatible_emits_one_ts2430() {
    // When only one of multiple bases is incompatible, emit exactly one TS2430.
    let source = r#"
interface Base1 {
    x: { a: string; }
}
interface Base2 {
    x: { b: string; }
}
interface Derived extends Base1, Base2 {
    x: { a: string; b: number; }
}
"#;
    let diags = get_diagnostics(source);
    let ts2430_msgs: Vec<_> = diags
        .iter()
        .filter(|d| d.0 == 2430)
        .map(|d| d.1.clone())
        .collect();
    assert_eq!(
        ts2430_msgs.len(),
        1,
        "Should emit exactly one TS2430 (only Base2 is incompatible). Got: {ts2430_msgs:?}"
    );
    assert!(
        ts2430_msgs[0].contains("Base2"),
        "Error should mention Base2. Got: {:?}",
        ts2430_msgs[0]
    );
}

/// TODO: This test should emit TS2430 when interface extends array type alias with
/// incompatible 'length' property, but array type resolution requires lib.d.ts
/// (Array<T> interface) which is not available in the unit test environment.
/// Once lib.d.ts is available in unit tests, update the assertion to expect TS2430.
#[test]
fn test_interface_extends_array_type_alias_incompatible() {
    // Interface extends array type alias with incompatible length property
    let source = r#"
type T3 = number[];
interface I3 extends T3 {
    length: string;
}
"#;
    // TODO: Should emit TS2430 but array type resolution requires lib.d.ts (Array<T>)
    // which is not available in unit test env. Currently only TS2318 errors are produced.
    let diags = get_diagnostics(source);
    let ts2430_errors: Vec<_> = diags.iter().filter(|d| d.0 == 2430).collect();
    assert!(
        ts2430_errors.is_empty(),
        "Expected no TS2430 (lib.d.ts not available in unit test env). Got: {ts2430_errors:?}"
    );
    // TS2318 is suppressed when no lib files are loaded and --noLib is not
    // explicitly set (unit test environment without lib context).
    let ts2318_errors: Vec<_> = diags.iter().filter(|d| d.0 == 2318).collect();
    assert!(
        ts2318_errors.is_empty(),
        "Expected no TS2318 when lib files are not loaded and --noLib is not set. Got: {ts2318_errors:?}"
    );
}

#[test]
fn test_interface_extends_class_with_private_emits_at_name() {
    // TS2430 for private member conflict should be reported at the interface name,
    // not at the member that conflicts
    let source = r#"
class Base {
    private x: number;
}
interface Foo extends Base {
    x: number;
}
"#;
    let diags = get_diagnostics(source);
    let ts2430 = diags.iter().filter(|d| d.0 == 2430).collect::<Vec<_>>();
    assert!(
        !ts2430.is_empty(),
        "Should emit TS2430 for interface extending class with private member. Got: {diags:?}"
    );
}

// =========================================================================
// TS2430: index signature incompatibility
// =========================================================================

#[test]
fn test_index_signature_string_incompatible() {
    // Derived interface has string index signature incompatible with base
    let source = r#"
interface E {
    [a: string]: string;
}
interface F extends E {
    [a: string]: number;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when derived string index type is not assignable to base"
    );
}

#[test]
fn test_index_signature_number_incompatible() {
    // Derived interface has number index signature incompatible with base
    let source = r#"
interface G {
    [a: number]: string;
}
interface H extends G {
    [a: number]: number;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when derived number index type is not assignable to base"
    );
}

#[test]
fn test_index_signature_compatible_no_error() {
    // Derived index signature is a subtype of base — no error
    let source = r#"
interface A {
    [a: string]: number;
}
interface B extends A {
    [a: string]: number;
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 when derived index type matches base"
    );
}

#[test]
fn test_inherited_index_signatures_conflict_across_bases() {
    // Interface extends two bases with conflicting index signatures
    // tsc emits TS2430 against the second base
    let source = r#"
interface A {
    [s: string]: number;
}
interface D {
    [s: string]: string;
}
interface E extends A, D { }
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when bases have conflicting index signatures"
    );
}

#[test]
fn test_inherited_index_signatures_compatible_across_bases() {
    // Interface extends two bases with compatible index signatures — no error
    let source = r#"
interface A {
    [s: string]: number;
}
interface B {
    [s: string]: number;
}
interface C extends A, B { }
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 when bases have compatible index signatures"
    );
}

// =========================================================================
// Overloaded generic callable property: tsc N×M erasure path
// =========================================================================

#[test]
fn test_overloaded_generic_callable_property_no_false_ts2430() {
    // When a property type is a callable object with overloaded generic signatures,
    // the child interface may override it with a single generic function type.
    // tsc's N×M signaturesRelatedTo path erases type params to `any`, making
    // `{ <T extends Derived>(a: T): T; <T extends Base>(a: T): T; }` assignable
    // to `<T>(a: T) => T`. This must not produce a false TS2430.
    let source = r#"
class Base { foo: string = ""; }
class Derived extends Base { bar: string = ""; }

interface Parent {
    method: {
        (x: {
            <T extends Derived>(a: T): T;
            <T extends Base>(a: T): T;
        }): any[];
    };
}

interface Child extends Parent {
    method: (x: <T>(a: T) => T) => any[];
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "Should NOT emit TS2430 when overloaded generic callable property is \
         overridden by a single generic function. tsc's N×M erasure path accepts this."
    );
}

#[test]
fn test_generic_member_call_signature_with_extra_type_param_no_false_ts2430() {
    // Contextual generic signature instantiation can relate a derived member with
    // extra type params to a base member after erasing both signatures.
    let source = r#"
type Base = { foo: string };

interface A {
    a11: <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
}

interface I extends A {
    a11: <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
}
"#;
    let diags = get_diagnostics(source);
    let ts2430 = diags.iter().filter(|d| d.0 == 2430).collect::<Vec<_>>();
    assert!(
        ts2430.is_empty(),
        "Should NOT emit TS2430 when the derived generic member call signature \
         is accepted by erased contextual comparison. Got: {diags:?}"
    );
}

#[test]
fn test_generic_member_construct_signature_with_extra_type_param_no_false_ts2430() {
    // The same erased contextual comparison is needed for construct signatures.
    let source = r#"
type Base = { foo: string };

interface A {
    a11: new <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
}

interface I extends A {
    a11: new <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
}
"#;
    let diags = get_diagnostics(source);
    let ts2430 = diags.iter().filter(|d| d.0 == 2430).collect::<Vec<_>>();
    assert!(
        ts2430.is_empty(),
        "Should NOT emit TS2430 when the derived generic member construct signature \
         is accepted by erased contextual comparison. Got: {diags:?}"
    );
}

#[test]
fn test_overloaded_generic_callable_property_incompatible_still_errors() {
    // The erasure path must NOT suppress genuine incompatibilities.
    // Here the return type is wrong (number[] vs string[]).
    let source = r#"
interface Parent {
    method: {
        (x: number): string[];
        (x: string): string[];
    };
}

interface Child extends Parent {
    method: (x: number) => number[];
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "Should still emit TS2430 when overloaded property has genuinely incompatible types"
    );
}

#[test]
fn test_constructor_typed_property_with_outer_type_param_errors() {
    // Mirrors conformance cases like subtypingWithConstructSignatures6.ts where
    // the derived property uses an outer interface type parameter and the base
    // property uses its own generic constructor signature.
    let source = r#"
interface Base {
    a: new <T>(x: T) => T[];
}

interface Derived<T> extends Base {
    a: new (x: T) => T[];
}
"#;

    assert!(
        has_error_with_code(source, 2430),
        "Should emit TS2430 when a derived constructor-typed property narrows a base generic constructor property. Got: {:?}",
        get_diagnostics(source)
    );
}

// =========================================================================
// Regression: `this` type substitution in whole-type assignability check
// =========================================================================

#[test]
fn test_class_implements_interface_with_this_type_no_false_positive() {
    // Interfaces using `this` type (e.g. `Vnode<A, this>`) must have `this`
    // substituted with the class instance type in the whole-type assignability
    // check, not just in per-property checks. Without this, the abstract `this`
    // causes a false TS2420/TS2430.
    //
    // Reduced from TypeScript's subclassThisTypeAssignable02.ts
    let source = r#"
interface Lifecycle<Attrs, State extends Lifecycle<Attrs, State>> {
    oninit?(vnode: Vnode<Attrs, State>): number;
    [_: number]: any;
}

interface Vnode<Attrs, State extends Lifecycle<Attrs, State>> {
    tag: Component<Attrs, State>;
}

interface Component<Attrs, State extends Lifecycle<Attrs, State>> {
    view(this: State, vnode: Vnode<Attrs, State>): number;
}

interface ClassComponent<A> extends Lifecycle<A, ClassComponent<A>> {
    oninit?(vnode: Vnode<A, this>): number;
    view(vnode: Vnode<A, this>): number;
}

interface MyAttrs { id: number }
class C implements ClassComponent<MyAttrs> {
    view(v: Vnode<MyAttrs, C>) { return 0; }
    [_: number]: unknown;
}
"#;
    let diags = get_diagnostics(source);
    let ts2430 = diags.iter().filter(|d| d.0 == 2430).collect::<Vec<_>>();
    assert!(
        ts2430.is_empty(),
        "Should NOT emit TS2430 for class implementing interface with `this` type references. Got: {ts2430:?}"
    );
}

#[test]
fn test_this_in_callback_parameter_with_recursive_return_no_false_ts2430() {
    // Reduced from immutable-style collection interfaces. A callback parameter
    // using `this` must not make a derived method incompatible when the return
    // type specializes the current interface family.
    let source = r#"
interface Boxed<T> {
    value: T;
}

interface Collection<K, V> {
    map<M>(mapper: (value: V, key: K, iter: this) => M): Collection<K, M>;
    flatMap<M>(mapper: (value: V, key: K, iter: this) => Boxed<M>): Collection<K, M>;
}

interface Indexed<T> extends Collection<number, T> {
    map<M>(mapper: (value: T, key: number, iter: this) => M): Indexed<M>;
    flatMap<M>(mapper: (value: T, key: number, iter: this) => Boxed<M>): Indexed<M>;
}
"#;
    let diags = get_diagnostics(source);
    let ts2430 = diags.iter().filter(|d| d.0 == 2430).collect::<Vec<_>>();
    assert!(
        ts2430.is_empty(),
        "Should NOT emit TS2430 for callback-parameter `this` with recursive return specialization. Got: {diags:?}"
    );
}

#[test]
fn test_direct_this_return_mismatch_still_emits_ts2430() {
    // A base method returning direct `this` cannot be replaced by an arbitrary
    // related interface return. This keeps the recursive-return suppression
    // narrow enough to preserve tsc's real TS2430 cases.
    let source = r#"
interface Collection<K, V> {
    toSeq(): this;
}

interface SeqKeyed<K, V> {
    get(key: K): V;
}

interface Keyed<K, V> extends Collection<K, V> {
    toSeq(): SeqKeyed<K, V>;
}
"#;
    let diags = get_diagnostics(source);
    let ts2430 = diags.iter().filter(|d| d.0 == 2430).collect::<Vec<_>>();
    assert!(
        !ts2430.is_empty(),
        "Should still emit TS2430 when a direct `this` return is narrowed to a named interface. Got: {diags:?}"
    );
}

#[test]
fn test_ancestor_primitive_return_still_emits_ts2430() {
    // Regression for PR #2571 review: the recursive-base suppression must NOT
    // hide genuine TS2430 errors where an ancestor (grandparent) member's
    // return type is an unrelated primitive like `string`. Only the worklist
    // path inspects ancestor members, so without the target-side guard this
    // false-suppression silently drops the error.
    let source = r#"
interface GrandBase {
    method(): string;
}
interface Base extends GrandBase {}
interface Derived extends Base {
    method(): Derived;
}
"#;
    let diags = get_diagnostics(source);
    let ts2430 = diags.iter().filter(|d| d.0 == 2430).collect::<Vec<_>>();
    assert!(
        !ts2430.is_empty(),
        "Should emit TS2430 when an ancestor member returns an unrelated primitive but the derived method narrows to the current interface. Got: {diags:?}"
    );
}
