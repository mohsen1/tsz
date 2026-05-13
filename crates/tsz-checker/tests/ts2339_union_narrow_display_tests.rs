//! Locks in TS2339 display for union receivers narrowed by control flow.
//!
//! Structural rule: when a property lookup fails on a union receiver that
//! flow analysis has narrowed to a strict subset of its members, the
//! diagnostic must name the narrowed type — that is the type the lookup
//! actually ran against. The earlier behavior emitted the original
//! pre-narrowing union, which prints the unrelated alternative members
//! and obscures what the type-checker actually saw.
//!
//! Type parameters keep their existing apparent-type display (the
//! constraint), and non-union receivers keep their literal-preserving
//! display (so primitive-literal receivers like `""` still print as
//! `""` rather than the widened `string`).

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostic_messages(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn constrained_interface_type_parameter_property_access_no_ts2339() {
    let diagnostics = diagnostic_messages(
        r#"
interface TreeNode2<T> {
  value: T;
  children: TreeNode2<T>[];
}

function traverse<T extends TreeNode2<any>>(node: T): T['value'][] {
  const result: T['value'][] = [node.value];
  return result;
}
"#,
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected constrained interface property access to avoid TS2339, got: {diagnostics:#?}"
    );
}

#[test]
fn instanceof_narrowed_union_receiver_displays_picked_member() {
    let src = r#"
class A { a: string = ""; }
class B { b: string = ""; }
function f(x: A | B) {
    if (x instanceof A) {
        x.b;
    }
}
"#;
    let diags = diagnostic_messages(src);
    let ts2339 = diags
        .iter()
        .find(|(code, _)| *code == 2339)
        .expect("expected TS2339 for missing 'b' on narrowed receiver");
    assert!(
        ts2339.1.contains("type 'A'"),
        "TS2339 should name the narrowed receiver 'A', got: {}",
        ts2339.1
    );
    assert!(
        !ts2339.1.contains("type 'A | B'"),
        "TS2339 should not display the un-narrowed union 'A | B', got: {}",
        ts2339.1
    );
}

#[test]
fn shadowed_object_and_function_instanceof_use_local_constructor_instances() {
    let src = r#"
function testObject() {
    class Object {
        private _brand!: void;
        obj = 1;
    }

    class Date {
        date = 1;
    }

    let value: string | Object = "" as string | Object;
    if (value instanceof Object) {
        const asLocalObject: Object = value;
        const asString: string = value;

        asLocalObject;
        asString;
    }

    let other: string | Date = "" as string | Date;
    if (other instanceof Object) {
        const impossible: never = other;
        const asDate: Date = other;

        impossible;
        asDate;
    }
}

function testFunction() {
    class Function {
        fn = 1;
    }

    let value: string | Function = "" as string | Function;
    if (value instanceof Function) {
        const asLocalFunction: Function = value;
        const asString: string = value;

        asLocalFunction;
        asString;
    }
}
"#;
    let diags = diagnostic_messages(src);
    let ts2322_messages: Vec<_> = diags
        .iter()
        .filter_map(|(code, msg)| (*code == 2322).then_some(msg.as_str()))
        .collect();

    assert!(
        ts2322_messages
            .iter()
            .any(|msg| msg.contains("Type 'Object' is not assignable to type 'string'.")),
        "expected local Object to narrow to the shadowing class, got: {diags:?}"
    );
    assert!(
        ts2322_messages.iter().any(|msg| {
            msg.contains("not assignable to type 'Date'")
                && !msg.contains("Type 'Date' is not assignable")
        }),
        "expected shadowed Object to preserve the intersection with Date instead of built-in \
         Object narrowing, got: {diags:?}"
    );
    assert!(
        !ts2322_messages.iter().any(|msg| {
            msg.contains("(Function & string)") || msg.contains("(Function & Function)")
        }),
        "local Function assignment should not use the built-in Function fast path, got: {diags:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|msg| msg.contains("Type 'Function' is not assignable to type 'string'.")),
        "expected local Function to narrow to the shadowing class, got: {diags:?}"
    );
}

/// When narrowing exhausts a union to `never`, property access against the
/// resulting `never` value must emit TS2339 — the access is unreachable and
/// the property genuinely doesn't exist on the value at this point. The
/// previous behavior suppressed TS2339 whenever the property happened to
/// exist on the un-narrowed declared receiver, which masked errors in
/// type-predicate chains over structurally-identical classes (issue #7271,
/// `instanceofWithStructurallyIdenticalTypes`).
#[test]
fn never_receiver_emits_ts2339_even_when_property_exists_on_declared_union() {
    let src = r#"
class C1 { item: string = ""; }
class C2 { item: string[] = []; }
class C3 { item: string = ""; }

function isC1(c: C1 | C2 | C3): c is C1 { return c instanceof C1 }
function isC2(c: C1 | C2 | C3): c is C2 { return c instanceof C2 }
function isC3(c: C1 | C2 | C3): c is C3 { return c instanceof C3 }

function foo2(x: C1 | C2 | C3): string {
    if (isC1(x)) {
        return x.item;
    }
    else if (isC2(x)) {
        return x.item[0];
    }
    else if (isC3(x)) {
        return x.item;
    }
    return "error";
}
"#;
    let diags = diagnostic_messages(src);
    let ts2339_on_never = diags.iter().find(|(code, msg)| {
        *code == 2339 && msg.contains("'item'") && msg.contains("type 'never'")
    });
    assert!(
        ts2339_on_never.is_some(),
        "expected TS2339 'Property item does not exist on type never' inside the unreachable \
         isC3 branch (C1 ≡ C3 structurally so isC1's else already filters C3, and isC2's else \
         exhausts the union to never), got: {diags:?}"
    );
}

#[test]
fn never_receiver_from_declared_discriminated_intersection_suppresses_ts2339() {
    let src = r#"
type RuntimeValue =
    | { type: 'number', value: number }
    | { type: 'string', value: string }
    | { type: 'boolean', value: boolean };

function foo1(x: RuntimeValue & { type: 'number' }) {
    if (x.type === 'number') {
        x.value;
    }
    else {
        x.value;
    }
}
"#;
    let diags = diagnostic_messages(src);
    let ts2339_on_value = diags.iter().find(|(code, msg)| {
        *code == 2339 && msg.contains("'value'") && msg.contains("type 'never'")
    });
    assert!(
        ts2339_on_value.is_none(),
        "declared discriminated intersections should preserve their property surface in \
         unreachable branches, got: {diags:?}"
    );
}

/// Literal-typed receivers must keep their literal display in TS2339 — the
/// helper must not collapse `''` to `'string'` just because the lookup
/// resolves through a primitive apparent type.
#[test]
fn literal_receiver_preserves_literal_in_ts2339_display() {
    let src = r#"
class C extends "".bogus {}
"#;
    let diags = diagnostic_messages(src);
    let ts2339 = diags
        .iter()
        .find(|(code, _)| *code == 2339)
        .expect("expected TS2339 for missing 'bogus' on string literal receiver");
    assert!(
        ts2339.1.contains("\"\"") || ts2339.1.contains("''"),
        "TS2339 should preserve the empty-string literal type in the message, got: {}",
        ts2339.1
    );
}
