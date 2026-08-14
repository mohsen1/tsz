//! Regression guards for #17480: a class's own re-entrant self-reference must
//! keep rendering its INSTANCE name, not the constructor's `typeof Name`.
//!
//! #17456's fix (`get_class_instance_type`'s deferral guard,
//! `types/class_type/entry.rs`) returns a bare `Lazy(DefId)` reference wrapping
//! the class's own `DefKind::Class` identity while the class's constructor
//! build is in flight (`class_constructor_resolution_set`), instead of the
//! member-less `symbol_instance_types` snapshot. `get_class_decl_for_display_type`
//! (`types/queries/core_names.rs`) unconditionally treated ANY bare
//! class-symbol `Lazy` reached this way as the constructor/static side
//! (`is_constructor = true`) when computing a diagnostic's displayed type
//! name, so every diagnostic whose object type resolved through that deferral
//! window rendered `typeof ClassName` instead of `ClassName` — even though the
//! type is the INSTANCE side and property lookup against it (which does not go
//! through this display helper) resolved correctly the whole time.
//!
//! The fix consults the `DefId`'s actual `DefKind` — matching the type
//! printer's own `is_class_constructor()` check in
//! `tsz-solver::diagnostics::format::compound::def_symbol_names` — instead of
//! assuming every such `Lazy` is the constructor. Binder names are varied on
//! purpose: the fix keys off `DefKind`, never an identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;
use tsz_common::common::ScriptTarget;

fn collect_diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    )
}

fn ts18014_messages(source: &str) -> Vec<String> {
    collect_diagnostics(source)
        .iter()
        .filter(|d| d.code == 18014)
        .map(|d| d.message_text.clone())
        .collect()
}

/// A `new Widget()` access re-entered while `Widget`'s own constructor build
/// is in flight (via an un-annotated method reached during that build) must
/// still render the INSTANCE name in a shadowed-private-identifier diagnostic.
/// Mirrors the reported `privateNameNestedMethodAccess.ts` shape with renamed
/// binders throughout (class, private names, nested class).
#[test]
fn reentrant_new_expression_keeps_instance_display_name() {
    let cases: &[(&str, &str)] = &[
        (
            "Widget/alpha/beta/gamma/Gadget (issue repro shape)",
            r#"
class Widget {
    #alpha = 42;
    #beta() { new Widget().#gamma; }
    get #gamma() { return 42; }

    build() {
        return class Gadget {
            #beta() {}
            constructor() {
                new Widget().#alpha;
                new Widget().#beta;
                new Widget().#gamma;
                new Gadget().#beta;
            }
        };
    }
}
"#,
        ),
        (
            "renamed binders (Engine/one/two/three/Part)",
            r#"
class Engine {
    #one = 1;
    #two() { new Engine().#three; }
    get #three() { return 1; }

    assemble() {
        return class Part {
            #two() {}
            constructor() {
                new Engine().#one;
                new Engine().#two;
                new Engine().#three;
                new Part().#two;
            }
        };
    }
}
"#,
        ),
    ];
    for (label, src) in cases {
        let messages = ts18014_messages(src);
        assert_eq!(
            messages.len(),
            1,
            "case {label}: expected exactly one TS18014"
        );
        assert!(
            messages[0].contains("cannot be accessed on type '") && !messages[0].contains("typeof"),
            "case {label}: instance-side access must not render 'typeof' — got: {}",
            messages[0]
        );
    }
}

/// Negative control: a genuinely STATIC shadowed private access under the same
/// re-entrant window must still render `typeof ClassName` — the fix must not
/// blanket-suppress the constructor rendering, only correct it for the
/// instance-side `Lazy` self-reference.
#[test]
fn reentrant_static_access_still_renders_typeof_name() {
    let src = r#"
class Widget {
    static #alpha = 42;
    static #beta() { Widget.#gamma; }
    static get #gamma() { return 42; }

    static build() {
        return class Gadget {
            static #beta() {}
            static setup() {
                Widget.#alpha;
                Widget.#beta;
                Widget.#gamma;
                Gadget.#beta;
            }
        };
    }
}
"#;
    let messages = ts18014_messages(src);
    assert_eq!(messages.len(), 1, "expected exactly one TS18014");
    assert!(
        messages[0].contains("typeof Widget"),
        "genuine static access must still render 'typeof Widget' — got: {}",
        messages[0]
    );
}
