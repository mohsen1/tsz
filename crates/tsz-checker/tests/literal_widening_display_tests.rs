//! Pins for diagnostic displays that distinguish fresh widening from declared
//! literal annotations.
//!
//! Inferred fresh members render through their canonical widened type, while
//! explicitly declared literal members stay literal. This is structural type
//! provenance; `TS2559` and `TS2560` use the same source display policy.

use tsz_checker::test_utils::{check_source_code_messages, check_source_diagnostics};

/// Object-literal method `this` circularity: the refined `this` display
/// (TS2339 receiver) widens literal annotations of sibling members, including
/// method return literals (`lit(): "tag"` renders as `lit(): string`).
#[test]
fn circular_method_this_display_widens_sibling_literals() {
    let source = r#"
const obj = {
    n: 1,
    s: "x",
    lit() { return "tag"; },
    m() { return this.missing; },
};
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![
            (
                2339_u32,
                "Property 'missing' does not exist on type \
                 '{ n: number; s: string; lit(): string; m(): any; }'."
                    .to_string()
            ),
            (
                7023_u32,
                "'m' implicitly has return type 'any' because it does not have a return type \
                 annotation and is referenced directly or indirectly in one of its return \
                 expressions."
                    .to_string()
            ),
        ],
    );
}

/// TS2322 source display: fresh object literal with a method widens the
/// method-return literal in the rendered source type.
#[test]
fn ts2322_source_method_literal_return_widens() {
    let source = r#"
const src = { m() { return "x"; }, k: 1 };
declare let dst: { m(): number; k: number };
dst = src;
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2322_u32,
            "Type '{ m(): string; k: number; }' is not assignable to type \
             '{ m(): number; k: number; }'."
                .to_string()
        )],
    );
}

/// TS2322 target display: a target with a method whose declared return type is
/// a literal keeps that literal (the target rewrite guards literal-sensitive
/// targets).
#[test]
fn ts2322_target_method_literal_return_display() {
    let source = r#"
const src = { m() { return 2; } };
declare let dst: { m(): "a" };
dst = src;
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2322_u32,
            "Type '{ m(): number; }' is not assignable to type '{ m(): \"a\"; }'.".to_string()
        )],
    );
}

/// TS2322 source display: literal annotations nested inside a generic
/// application argument in a property type widen in the rendered source.
#[test]
fn ts2322_source_nested_application_literal_widens() {
    let source = r#"
interface Box<T> { value: T }
declare let src: { p: Box<{ a: "x" }> };
declare let dst: { p: number };
dst = src;
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2322_u32,
            "Type '{ p: Box<{ a: string; }>; }' is not assignable to type '{ p: number; }'."
                .to_string()
        )],
    );
}

/// TS2560 ("did you mean to call it?") at an assignment site preserves the
/// literal member of the callable source display.
#[test]
fn ts2560_assignment_source_literal_member_preserved() {
    let source = r#"
declare const fn: (() => { opt: string }) & { a: 1 };
const x: { opt?: string } = fn;
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2560_u32,
            "Value of type '(() => { opt: string; }) & { a: 1; }' has no properties in common \
             with type '{ opt?: string | undefined; }'. Did you mean to call it?"
                .to_string()
        )],
    );
}

/// TS2559 (plain "no properties in common", assignment site) preserves
/// literal members of the source display: the widening rewrite is scoped to
/// call-site argument displays.
#[test]
fn ts2559_assignment_source_literal_member_preserved() {
    let source = r#"
declare const fn: (() => { a: number }) & { a: 1 };
const x: { opt?: string } = fn;
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2559_u32,
            "Type '(() => { a: number; }) & { a: 1; }' has no properties in common with type \
             '{ opt?: string | undefined; }'."
                .to_string()
        )],
    );
}

/// TS2560 at a call site preserves declared literal members.
#[test]
fn ts2560_call_argument_declared_literal_member_preserved() {
    let source = r#"
declare function take(x: { opt?: string }): void;
declare const fn: (() => { opt: string }) & { a: 1 };
take(fn);
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2560_u32,
            "Value of type '(() => { opt: string; }) & { a: 1; }' has no properties in \
             common with type '{ opt?: string | undefined; }'. Did you mean to call it?"
                .to_string()
        )],
    );
}

/// TS2559 at a call site preserves declared literal members through the same
/// source display policy as TS2560.
#[test]
fn ts2559_call_argument_declared_literal_member_preserved() {
    let source = r#"
declare function take(x: { opt?: string }): void;
declare const fn: (() => { a: number }) & { a: 1 };
take(fn);
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2559_u32,
            "Type '(() => { a: number; }) & { a: number; }' has no properties in common with \
             type '{ opt?: string | undefined; }'."
                .to_string()
        )],
    );
}

/// A genuinely inferred fresh callable member is already widened canonically
/// for TS2560.
#[test]
fn ts2560_call_argument_inferred_fresh_member_widens() {
    let source = r#"
declare function take(x: { opt?: string }): void;
declare function merge<A extends object, B extends object>(a: A, b: B): A & B;
const fn = merge(() => ({ opt: "ok" }), { a: 1 });
take(fn);
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2560_u32,
            "Value of type '(() => { opt: string; }) & { a: number; }' has no properties in \
             common with type '{ opt?: string | undefined; }'. Did you mean to call it?"
                .to_string()
        )],
    );
}

/// The same inferred-fresh rule applies to TS2559.
#[test]
fn ts2559_call_argument_inferred_fresh_member_widens() {
    let source = r#"
declare function take(x: { opt?: string }): void;
declare function merge<A extends object, B extends object>(a: A, b: B): A & B;
const fn = merge(() => ({ a: 0 }), { a: 1 });
take(fn);
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2559_u32,
            "Type '(() => { a: number; }) & { a: number; }' has no properties in common with \
             type '{ opt?: string | undefined; }'."
                .to_string()
        )],
    );
}

/// A const assertion turns one inferred member into a real literal annotation
/// that TS2560 preserves while widening its fresh sibling.
#[test]
fn ts2560_call_argument_inferred_const_member_preserved() {
    let source = r#"
declare function take(x: { opt?: string }): void;
declare function merge<A extends object, B extends object>(a: A, b: B): A & B;
const fn = merge(() => ({ opt: "ok" }), { fresh: 1, pinned: 2 as const });
take(fn);
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2560_u32,
            "Value of type '(() => { opt: string; }) & { fresh: number; pinned: 2; }' has no \
             properties in common with type '{ opt?: string | undefined; }'. Did you mean to \
             call it?"
                .to_string()
        )],
    );
}

/// The general failure renderer used by `satisfies` preserves declared
/// literals for TS2560.
#[test]
fn ts2560_satisfies_declared_literal_member_preserved() {
    let source = r#"
declare const fn: (() => { opt: string }) & { a: 1 };
fn satisfies { opt?: string };
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2560_u32,
            "Value of type '(() => { opt: string; }) & { a: 1; }' has no properties in common \
             with type '{ opt?: string | undefined; }'. Did you mean to call it?"
                .to_string()
        )],
    );
}

/// The general failure renderer uses the same declared-literal policy for
/// TS2559.
#[test]
fn ts2559_satisfies_declared_literal_member_preserved() {
    let source = r#"
declare const fn: (() => { a: number }) & { a: 1 };
fn satisfies { opt?: string };
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2559_u32,
            "Type '(() => { a: number; }) & { a: 1; }' has no properties in common with type \
             '{ opt?: string | undefined; }'."
                .to_string()
        )],
    );
}

/// TS2559 assignment site: a *fresh* object-literal source widens its property
/// literals (`{ c: 1 }` → `{ c: number }`), matching tsc and the rest of the
/// assignability diagnostics (TS2322/TS2739/TS2741). Regression for #14829.
#[test]
fn ts2559_assignment_fresh_object_literal_source_widens() {
    let source = r#"
interface Weak { a?: number; b?: number; }
const src = { c: 1 };
const w: Weak = src;
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2559_u32,
            "Type '{ c: number; }' has no properties in common with type 'Weak'.".to_string()
        )],
    );
}

/// TS2559 with a multi-property fresh source widens every property literal.
/// Binder names differ from the single-property case to prove the widening is
/// structural, not keyed on identifier text.
#[test]
fn ts2559_assignment_multi_property_fresh_source_widens() {
    let source = r#"
interface Sink { p?: string; q?: string; }
const payload = { first: 1, second: 2 };
const target: Sink = payload;
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2559_u32,
            "Type '{ first: number; second: number; }' has no properties in common with type \
             'Sink'."
                .to_string()
        )],
    );
}

/// TS2559 with the source coming from an inferred function return widens the
/// returned object literal's property literals.
#[test]
fn ts2559_assignment_function_return_fresh_source_widens() {
    let source = r#"
interface Slot { lo?: boolean; hi?: boolean; }
function build() { return { count: 1 }; }
const slot: Slot = build();
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2559_u32,
            "Type '{ count: number; }' has no properties in common with type 'Slot'.".to_string()
        )],
    );
}

/// Control from #14829: an explicitly annotated, already widened source is
/// unaffected and keeps its declared display.
#[test]
fn ts2559_annotated_widened_source_unchanged() {
    let source = r#"
interface Weak { a?: number; b?: number; }
let src: { c: number } = { c: 1 };
const w: Weak = src;
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2559_u32,
            "Type '{ c: number; }' has no properties in common with type 'Weak'.".to_string()
        )],
    );
}

/// TS2345 generic parameter display: string literal annotations inside the
/// inferred generic application's object argument widen, while the literal
/// key argument is preserved.
#[test]
fn ts2345_generic_parameter_object_literal_annotations_widen() {
    let source = r#"
interface ListProps<T, K extends keyof T> {
    items: T[];
    itemKey: K;
    prop: number;
}
declare const Component: <T, K extends keyof T>(x: ListProps<T, K>) => void;
Component({ items: [{ name: ' string' }], itemKey: 'name' });
"#;
    let msgs = check_source_code_messages(source);
    assert_eq!(
        msgs,
        vec![(
            2345_u32,
            "Argument of type '{ items: { name: string; }[]; itemKey: \"name\"; }' is not \
             assignable to parameter of type 'ListProps<{ name: string; }, \"name\">'."
                .to_string()
        )],
    );
}

/// Missing-property related information widens literal annotations of both
/// rendered object types (method return literals included).
#[test]
fn missing_property_related_info_widens_anonymous_object_literals() {
    let source = r#"
declare function g(x: { a: string; b: number; m(): string }): void;
const arg = { a: "x", m() { return "y"; } };
g(arg);
"#;
    let diags = check_source_diagnostics(source);
    let mut related: Vec<String> = Vec::new();
    for diag in &diags {
        for info in &diag.related_information {
            related.push(info.message_text.clone());
        }
    }
    assert_eq!(
        related,
        vec![
            "Property 'b' is missing in type '{ a: string; m(): string; }' but required in type \
             '{ a: string; b: number; m(): string; }'."
                .to_string()
        ],
    );
}
