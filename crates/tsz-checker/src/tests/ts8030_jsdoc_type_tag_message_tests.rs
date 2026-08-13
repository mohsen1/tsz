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

/// An object-literal method with no JSDoc of its own must not inherit the
/// outer `@type` tag describing the object shape it lives in — that tag has
/// nothing to say about whether the method itself is "callable" in the
/// TS8030 sense (it always is). Reduced from `typeFromContextualThisType.ts`
/// (conformance/salsa), whose oracle reports only TS7006, never TS8030.
#[test]
fn object_literal_method_without_own_jsdoc_does_not_inherit_outer_type_tag() {
    let codes = js_diagnostics(concat!(
        "/** @type {{ a(): void; b?(n: number): number; }} */\n",
        "const o1 = {\n",
        "    a() {\n",
        "        this.b = n => n;\n",
        "    }\n",
        "};\n"
    ))
    .into_iter()
    .map(|(code, _)| code)
    .collect::<Vec<_>>();
    assert!(!codes.contains(&8030), "got {codes:?}");
}

#[test]
fn boundary_is_binder_name_independent() {
    for (var_name, method_name, member_name) in [
        ("o1", "a", "b"),
        ("shape", "run", "on"),
        ("_x0", "go", "cb"),
    ] {
        let source = format!(
            "/** @type {{ {method_name}(): void; {member_name}?(n: number): number; }} */\nconst {var_name} = {{\n    {method_name}() {{\n        this.{member_name} = n => n;\n    }}\n}};\n"
        );
        let codes = js_diagnostics(&source)
            .into_iter()
            .map(|(code, _)| code)
            .collect::<Vec<_>>();
        assert!(!codes.contains(&8030), "source={source} got {codes:?}");
    }
}

/// Same boundary, array-literal form: a method-shorthand element of an array
/// literal must not inherit the array's own `@type` tag either.
#[test]
fn array_literal_method_without_own_jsdoc_does_not_inherit_outer_type_tag() {
    let codes = js_diagnostics(concat!(
        "/** @type {{ run(): void }[]} */\n",
        "const list = [{\n",
        "    run() {}\n",
        "}];\n"
    ))
    .into_iter()
    .map(|(code, _)| code)
    .collect::<Vec<_>>();
    assert!(!codes.contains(&8030), "got {codes:?}");
}

/// The same boundary applies when the object literal is the right-hand side
/// of a property assignment rather than a variable initializer. Reduced from
/// `contextualTypedSpecialAssignment.ts` (conformance/salsa).
#[test]
fn object_literal_method_in_property_assignment_does_not_inherit_outer_type_tag() {
    let codes = js_diagnostics(concat!(
        "/** @typedef {{ status: 'done', m(n: number): void }} DoneStatus */\n",
        "var ns = {};\n",
        "/** @type {DoneStatus} */\n",
        "ns.x = {\n",
        "    status: 'done',\n",
        "    m(n) { }\n",
        "};\n"
    ))
    .into_iter()
    .map(|(code, _)| code)
    .collect::<Vec<_>>();
    assert!(!codes.contains(&8030), "got {codes:?}");
}

/// Positive control: JSDoc written directly above the method itself must
/// still fire TS8030 through the object-literal boundary — the boundary
/// guard only blocks inheriting an *ancestor's* tag, not the method's own.
#[test]
fn object_literal_method_with_its_own_type_tag_still_reports_ts8030() {
    let codes = ts8030_messages(concat!(
        "const obj = {\n",
        "    /** @type {number} */\n",
        "    m() { return 1; }\n",
        "};\n"
    ));
    assert_eq!(codes, vec![EXPECTED.to_string()]);
}
