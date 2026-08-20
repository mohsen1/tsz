//! Two JSDoc tag diagnostics the pinned compiler no longer reports.
//!
//! TS8022 ("JSDoc '@extends'/'@augments' is not attached to a class") and
//! TS8021 ("JSDoc '@typedef' tag should either have a type annotation or be
//! followed by '@property' or '@member' tags") have zero expectations across
//! the whole conformance corpus — including the tests written specifically to
//! provoke them, `jsdocAugments_notAClass` and `extendsTag4`, which now expect
//! no diagnostics at all. tsz kept emitting both.
//!
//! The sibling code TS8023 ("does not match the 'extends' clause") is still
//! reported and is covered here as a positive control, so a regression that
//! silences the whole `@augments` area would be caught.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn js_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn augments_on_a_non_class_is_not_reported() {
    for tag in ["augments", "extends"] {
        // Attached to a function, an arrow, and a variable declaration.
        for decl in [
            "function b() {}",
            "const b = () => {};",
            "var b = 1;",
            "class B {}",
        ] {
            let source = format!("class A {{}}\n/** @{tag} A */\n{decl}\n");
            let codes = js_codes(&source);
            assert!(
                !codes.contains(&8022),
                "@{tag} on `{decl}` must not report TS8022; got {codes:?}"
            );
        }
    }
}

#[test]
fn dangling_augments_comment_is_not_reported() {
    // A JSDoc comment attached to nothing at all.
    let codes = js_codes("class A {}\n\n/** @augments A */\n");
    assert!(!codes.contains(&8022), "got {codes:?}");
}

#[test]
fn augments_reporting_is_binder_name_independent() {
    for (base, other) in [("A", "B"), ("Widget", "Gadget"), ("_x0", "_y1")] {
        let source = format!("class {base} {{}}\n/** @augments {base} */\nfunction f() {{}}\n");
        assert!(
            !js_codes(&source).contains(&8022),
            "base={base} must not report TS8022"
        );
        // Positive control: the mismatch diagnostic is a different code and
        // must survive.
        let mismatch = format!(
            "class {base} {{}}\nclass {other} {{}}\n/** @augments {base} */\nclass C extends {other} {{}}\n"
        );
        assert!(
            js_codes(&mismatch).contains(&8023),
            "base={base} other={other}: TS8023 must still be reported"
        );
    }
}

#[test]
fn typedef_without_type_or_properties_is_not_reported() {
    for source in [
        "/** @typedef T */\nvar x = 1;\n",
        "/**\n * @typedef Foo\n */\nvar y = 2;\n",
    ] {
        let codes = js_codes(source);
        assert!(!codes.contains(&8021), "got {codes:?} for {source:?}");
    }
}

#[test]
fn typedef_with_type_or_properties_still_resolves() {
    // Positive control: well-formed typedefs keep working, so the removal did
    // not disturb typedef handling itself.
    let codes = js_codes(
        "/** @typedef {{ name: string }} Person */\n\
         /** @param {Person} p */\nfunction greet(p) { return p.name; }\n",
    );
    assert!(!codes.contains(&8021), "got {codes:?}");
    assert!(
        !codes.contains(&2304),
        "Person should resolve; got {codes:?}"
    );
}
