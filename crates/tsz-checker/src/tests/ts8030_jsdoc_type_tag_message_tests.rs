//! TS8030 wording for a JS function declaration whose `@type` tag is not callable.
//!
//! The message text moved with the compiler version this corpus is pinned to:
//! it used to read "The type of a function declaration must match the
//! function's signature." and now reads "A JSDoc '@type' tag on a function must
//! have a signature with the correct number of arguments." tsz had the current
//! wording in its diagnostics table but the emission site carried a hardcoded
//! copy of the old string, so every TS8030 rendered stale.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source, check_source_diagnostics};

const EXPECTED: &str =
    "A JSDoc '@type' tag on a function must have a signature with the correct number of arguments.";

fn js_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts8030_messages(source: &str) -> Vec<String> {
    js_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 8030)
        .map(|(_, message)| message)
        .collect()
}

#[test]
fn non_callable_type_tag_uses_current_wording() {
    let messages = ts8030_messages("/** @type {number} */\nfunction f() { return 1; }\n");
    assert_eq!(messages, vec![EXPECTED.to_string()]);
}

#[test]
fn wording_is_binder_name_independent() {
    // Same structural situation under renamed binders and different
    // non-callable annotations.
    for (name, annotation) in [
        ("f", "number"),
        ("compute", "string"),
        ("_handler0", "boolean"),
    ] {
        let source =
            format!("/** @type {{{annotation}}} */\nfunction {name}() {{ return undefined; }}\n");
        assert_eq!(
            ts8030_messages(&source),
            vec![EXPECTED.to_string()],
            "name={name} annotation={annotation}"
        );
    }
}

#[test]
fn callable_type_tag_reports_nothing() {
    // Positive control: a callable annotation must stay silent, so the wording
    // change cannot be masking a condition change.
    for annotation in ["(a: number) => number", "() => void"] {
        let source = format!("/** @type {{{annotation}}} */\nfunction f(a) {{ return a; }}\n");
        assert!(
            ts8030_messages(&source).is_empty(),
            "annotation={annotation} must not report TS8030"
        );
    }
}

#[test]
fn ts8030_is_not_reported_in_typescript_files() {
    let diags =
        check_source_diagnostics("/** @type {number} */\nfunction f(): number { return 1; }\n");
    assert!(
        diags.iter().all(|d| d.code != 8030),
        "TS8030 is JS-only; got: {diags:?}"
    );
}

// =========================================================================
// An outer `@type` tag describing an object-literal's shape must not be
// misattributed to an unannotated method nested inside that literal
// (typeFromContextualThisType.ts / contextualTypedSpecialAssignment.ts).
// =========================================================================

#[test]
fn outer_type_tag_on_object_literal_does_not_reach_unannotated_method() {
    // `@type {{ a(): void }}` describes `o1`'s shape; `a()` itself carries no
    // JSDoc of its own, so it must not be checked against the outer tag.
    let messages = ts8030_messages(
        r#"/** @type {{ a(): void }} */
const o1 = {
    a() {}
};
"#,
    );
    assert!(
        messages.is_empty(),
        "outer object-shape @type tag must not reach the nested method; got: {messages:?}"
    );
}

#[test]
fn outer_type_tag_on_property_assignment_rhs_does_not_reach_unannotated_method() {
    // Same boundary when the object literal is the right-hand side of a
    // property assignment (`ns.x = {...}`) rather than a variable
    // initializer — reduced from contextualTypedSpecialAssignment.ts.
    let messages = ts8030_messages(concat!(
        "/** @typedef {{ status: 'done', m(n: number): void }} DoneStatus */\n",
        "var ns = {};\n",
        "/** @type {DoneStatus} */\n",
        "ns.x = {\n",
        "    status: 'done',\n",
        "    m(n) { }\n",
        "};\n",
    ));
    assert!(
        messages.is_empty(),
        "outer @type tag on a property-assignment RHS must not reach the nested method; got: {messages:?}"
    );
}

#[test]
fn outer_type_tag_is_binder_and_method_name_independent() {
    for (var_name, method_name, shape) in [
        ("o1", "a", "{ a(): void }"),
        ("config", "run", "{ run(): void }"),
        ("_handler0", "go", "{ go(): void }"),
    ] {
        let source = format!(
            "/** @type {{{shape}}} */\nconst {var_name} = {{\n    {method_name}() {{}}\n}};\n"
        );
        assert!(
            ts8030_messages(&source).is_empty(),
            "var={var_name} method={method_name}: outer @type tag must not reach the nested method; got: {:?}",
            ts8030_messages(&source)
        );
    }
}

#[test]
fn outer_type_tag_on_array_literal_does_not_reach_element_method() {
    // Same boundary rule for array literals: `@type` on the array's own
    // declaration must not cascade into an element's method.
    let messages = ts8030_messages(
        r#"/** @type {{ a(): void }[]} */
const list = [
    { a() {} }
];
"#,
    );
    assert!(
        messages.is_empty(),
        "outer array-shape @type tag must not reach a nested element's method; got: {messages:?}"
    );
}

#[test]
fn outer_type_tag_boundary_is_scoped_to_the_nearest_literal() {
    // Nested object literals: the outer tag must not reach a method two
    // literal-levels deep either.
    let messages = ts8030_messages(
        r#"/** @type {{ inner: { a(): void } }} */
const o1 = {
    inner: {
        a() {}
    }
};
"#,
    );
    assert!(
        messages.is_empty(),
        "outer @type tag must not reach a method nested inside a deeper object literal; got: {messages:?}"
    );
}

#[test]
fn direct_type_tag_on_object_literal_method_still_reports_ts8030() {
    // Positive control: a JSDoc comment written *directly* above the method
    // itself (no ancestor walk needed) must still be checked — this is the
    // checkJsdocTypeTagOnObjectProperty1.ts witness the object-methods check
    // exists for, and the fix above must not disable it.
    let messages = ts8030_messages(
        r#"const obj = {
    /** @type {number} */
    method1(n1) {}
};
"#,
    );
    assert_eq!(
        messages,
        vec![EXPECTED.to_string()],
        "a JSDoc tag written directly above the method must still be checked"
    );
}

#[test]
fn outer_type_tag_on_const_arrow_still_contextually_types_the_body() {
    // Positive control for the ancestor walk itself (a different consumer
    // than TS8030's object-methods check, since TS8030 is only wired for
    // `function` declarations and object-literal methods — a plain
    // `const f = () => {}` is never itself TS8030-checked): when the
    // annotated node *is* the literal's declared value (no object/array
    // literal in between the arrow and its `@type` tag), the walk must
    // still find the tag and use it to contextually type the body, the way
    // `jsdoc_type_function_on_method_shorthand_checks_block_body_return_type`
    // proves for the object-literal-method form above.
    let diags = crate::test_utils::check_js_source_diagnostics(
        r#"// @ts-check
/** @type {(n: number) => number} */
const f = (n) => {
    return "42";
};
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 (string -> number) on the arrow's return, proving the outer \
         @type tag still reaches it through the ancestor walk; got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
