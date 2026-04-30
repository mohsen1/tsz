//! Locks in deep-property elaboration through paren/comma/assignment wrappers
//! on variable initializers. tsc anchors a single TS2322 at the deepest
//! mismatching leaf even when the initializer object literal sits behind
//! `( ... )`, `(void 0, { ... })`, or `prop = { ... }` wrappers.
//!
//! Regression: `slightlyIndirectedDeepObjectLiteralElaborations.ts` —
//! `const x: Foo = (void 0, { a: q = { b: ({ c: { d: 42 } }) } })` was
//! emitting an outer-anchored TS2322 with a one-level type display instead
//! of drilling to `d: 42` and reporting `Type 'number' is not assignable to
//! type 'string'.`.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    diagnostics_with_pos(source)
        .into_iter()
        .map(|(code, _, msg)| (code, msg))
        .collect()
}

fn diagnostics_with_pos(source: &str) -> Vec<(u32, u32, String)> {
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
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

#[test]
fn comma_wrapped_deep_object_literal_drills_to_leaf() {
    let src = r#"
interface Foo {
    a: { b: { c: { d: string } } }
}
let q: Foo["a"] | undefined;
const x: Foo = (void 0, {
    a: q = {
        b: ({
            c: { d: 42 }
        })
    }
});
"#;
    let diags = diagnostics(src);
    let leaf = diags
        .iter()
        .find(|(code, msg)| *code == 2322 && msg.contains("'number'") && msg.contains("'string'"));
    assert!(
        leaf.is_some(),
        "expected TS2322 with deep `number → string` leaf message, got: {diags:?}"
    );
    for (code, msg) in &diags {
        if *code == 2322 {
            assert!(
                !msg.contains("'{ b:"),
                "TS2322 should not anchor at outer property `a`'s shape — drill to leaf, got: {msg}"
            );
        }
    }
}

#[test]
fn paren_wrapped_object_literal_drills_to_property() {
    let src = r#"
interface T { a: string; }
const x: T = ({ a: 42 });
"#;
    let diags = diagnostics(src);
    let prop_level = diags
        .iter()
        .find(|(code, msg)| *code == 2322 && msg.contains("'number'") && msg.contains("'string'"));
    assert!(
        prop_level.is_some(),
        "expected per-property TS2322 `number → string` through paren wrapper, got: {diags:?}"
    );
}

#[test]
fn assignment_in_property_value_drills_into_rhs() {
    let src = r#"
interface T { a: { b: string }; }
let q: { b: string } | undefined;
const x: T = { a: q = { b: 42 } };
"#;
    let diags = diagnostics(src);
    let leaf = diags
        .iter()
        .find(|(code, msg)| *code == 2322 && msg.contains("'number'") && msg.contains("'string'"));
    assert!(
        leaf.is_some(),
        "expected TS2322 deep leaf through `q = (...)` assignment in value, got: {diags:?}"
    );
}

/// `const x: T = expr satisfies S` should anchor the TS2322 at the
/// variable binding (matching tsc), not deep inside the object literal.
/// tsc treats explicit type assertions (`satisfies`, `as`, `<T>`) as
/// opaque for elaboration: drilling through the assertion would emit
/// the diagnostic at the inner property-value position instead of at
/// the assignment site. Regression: conformance test
/// `typeSatisfaction_vacuousIntersectionOfContextualTypes.ts`.
#[test]
fn satisfies_wrapped_object_literal_anchors_at_binding() {
    // `const b ...` — the variable name `b` starts at byte 7 on the second line.
    // The leading newline and `const ` (6 bytes) puts `b` at offset 1 + 6 = 7.
    let src =
        "\nconst b: { xyz: \"baz\" } = { xyz: \"foo\" } satisfies { xyz: \"foo\" | \"bar\" };\n";
    let diags = diagnostics_with_pos(src);
    let ts2322: Vec<&(u32, u32, String)> =
        diags.iter().filter(|(code, _, _)| *code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 diagnostic, got: {diags:?}"
    );
    let (_, start, msg) = ts2322[0];
    let object_literal_start = src.find("{ xyz: \"foo\"").expect("literal in source") as u32;
    assert!(
        *start < object_literal_start,
        "TS2322 should anchor at or before the variable binding (offset {start} < object literal at {object_literal_start}), got start={start} msg={msg:?}",
    );
}

/// Same check for `as` type assertions: drilling into the inner object
/// literal would shift the diagnostic to the assertion's expression
/// position. tsc anchors at the assignment.
#[test]
fn as_assertion_wrapped_object_literal_anchors_at_binding() {
    let src = "\ninterface T { xyz: \"baz\"; }\nconst b: T = { xyz: \"foo\" } as { xyz: \"foo\" | \"bar\" };\n";
    let diags = diagnostics_with_pos(src);
    let ts2322: Vec<&(u32, u32, String)> =
        diags.iter().filter(|(code, _, _)| *code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 diagnostic, got: {diags:?}"
    );
    let (_, start, msg) = ts2322[0];
    let object_literal_start = src.find("{ xyz: \"foo\"").expect("literal in source") as u32;
    assert!(
        *start < object_literal_start,
        "TS2322 should anchor at or before the variable binding (offset {start} < object literal at {object_literal_start}), got start={start} msg={msg:?}",
    );
}

/// `as const` is not opaque for elaboration like `as T` / `satisfies T`:
/// it freezes literals and adds readonly without changing structural shape.
/// tsc drills into the offending property when the inner object literal
/// mismatches the declared type.
///
/// Regression for `constAssertions.ts` (Foo54374 case): `const x: { b: 2 }
/// = { b: 3 } as const` should anchor TS2322 at the `b: 3` property, not
/// at the variable binding.
#[test]
fn as_const_wrapped_object_literal_drills_to_property() {
    let src = r#"
interface Foo54374 {
  a: 1;
  b: 2;
}

const fooConst54374: Foo54374 = {
  a: 1,
  b: 3
} as const
"#;
    let diags = diagnostics_with_pos(src);

    // tsc anchors at `b: 3`. We anchor at the `b` property element. Both
    // sit *inside* the object literal, well past the variable binding.
    let var_binding_offset = src.find("fooConst54374").unwrap() as u32;
    let object_literal_start = src.find('{').unwrap() as u32;
    let b_property_offset = src.rfind("b: 3").unwrap() as u32;

    let ts2322: Vec<_> = diags.iter().filter(|(code, ..)| *code == 2322).collect();
    assert!(!ts2322.is_empty(), "expected TS2322, got: {diags:?}");
    for (_code, start, msg) in ts2322 {
        assert!(
            *start >= object_literal_start,
            "TS2322 for as-const should drill into the object literal (start={start} >= object literal at {object_literal_start}), not anchor at the binding (offset {var_binding_offset}). msg={msg:?}",
        );
        // Tighter: the diagnostic should anchor at or after the offending property.
        assert!(
            *start >= b_property_offset || (msg.contains("'3'") && msg.contains("'2'")),
            "TS2322 should anchor at the `b: 3` property (offset >= {b_property_offset}) OR have the leaf 3-vs-2 message; got start={start} msg={msg:?}",
        );
        // Sanity: leaf-level message shape.
        assert!(
            msg.contains("'3'") && msg.contains("'2'"),
            "expected 3-vs-2 leaf message, got: {msg:?}",
        );
    }
}
