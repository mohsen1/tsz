//! Probe tests for type guard narrowing of mutable/untyped fields.
//! Investigates the typeGuardNarrowByMutableUntypedField.ts false positive pattern.

use tsz_checker::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn type_guard_discriminant_union_no_error() {
    let c = codes(
        r#"
type A = { kind: 'a'; value: string };
type B = { kind: 'b'; count: number };
function isA(x: A | B): x is A {
    return x.kind === 'a';
}
function test(x: A | B) {
    if (isA(x)) {
        const v: string = x.value;
    }
}
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}

#[test]
fn type_guard_class_mutable_field_no_error() {
    let c = codes(
        r#"
class Widget {
    data = undefined as string | undefined;
    isReady(): this is Widget & { data: string } {
        return this.data !== undefined;
    }
}
function use(w: Widget) {
    if (w.isReady()) {
        const s: string = w.data;
    }
}
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}

#[test]
fn type_guard_narrows_optional_property_no_error() {
    // Type guard that intersects with a property type overrides a mutable optional field
    let c = codes(
        r#"
interface Box { value?: string }
interface FilledBox extends Box { value: string }
function isFilled(x: Box): x is FilledBox {
    return x.value !== undefined;
}
function test(x: Box) {
    if (isFilled(x)) {
        const v: string = x.value;
    }
}
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}

#[test]
fn type_guard_intersection_narrowing_no_error() {
    // Type guard that narrows by adding a property via intersection
    let c = codes(
        r#"
interface Base { kind: string }
interface Specific extends Base { kind: 'specific'; extra: number }
function isSpecific(x: Base): x is Specific {
    return x.kind === 'specific';
}
function test(x: Base) {
    if (isSpecific(x)) {
        const k: 'specific' = x.kind;
        const e: number = x.extra;
    }
}
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}

#[test]
fn type_guard_via_property_of_union_no_error() {
    // Type guard narrows via access to a property that could be mutable
    let c = codes(
        r#"
interface HasName { name: string }
interface NoName { id: number }
function hasName(x: HasName | NoName): x is HasName {
    return 'name' in x;
}
function test(x: HasName | NoName) {
    if (hasName(x)) {
        const n: string = x.name;
    }
}
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}

#[test]
fn type_guard_this_predicate_mutable_field_no_error() {
    // Type guard with `this is T` on a mutable property
    let c = codes(
        r#"
class Foo {
    value: string | null = null;
    isPopulated(): this is Foo & { value: string } {
        return this.value !== null;
    }
}
function use(f: Foo) {
    if (f.isPopulated()) {
        const s: string = f.value;
    }
}
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}

#[test]
fn type_guard_narrowing_to_never_on_false_branch() {
    // Type guard that should narrow to never in the else branch
    let c = codes(
        r#"
interface Foo { kind: 'foo'; x: number }
interface Bar { kind: 'bar'; y: string }
function isFoo(x: Foo | Bar): x is Foo {
    return x.kind === 'foo';
}
function test(x: Foo | Bar) {
    if (!isFoo(x)) {
        const y: string = x.y;
    }
}
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}

#[test]
fn type_guard_narrows_unknown_with_index_access_no_error() {
    // Type guard with index access to a field that has no explicit annotation on the object
    let c = codes(
        r#"
interface Container {
    data: string | undefined;
}
function hasData(x: Container): x is Container & { data: string } {
    return x.data !== undefined;
}
function process(c: Container) {
    if (hasData(c)) {
        const d: string = c.data;
    }
}
"#,
    );
    assert!(c.is_empty(), "expected no errors, got: {:?}", c);
}
