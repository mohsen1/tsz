//! A comment in the source seam between a function-like's name and its
//! parameter-list `(` belongs immediately after the emitted name (before `(`),
//! not inside the parameter list. `tsc` emits `function clone /* <T> */(a, b)`,
//! keeping the comment attached to the name. These tests cover the shared
//! name → `(` seam across function declarations, function expressions, methods,
//! and accessors, and verify the rule is structural (not keyed on identifier
//! names) and does not drag body / type-parameter-internal comments into the
//! seam.

use crate::output::printer::{PrintOptions, Printer};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrintOptions {
        target: ScriptTarget::ES2017,
        ..Default::default()
    };
    let mut printer = Printer::new(&parser.arena, options);
    printer.set_source_text(source);
    printer.print(root);
    printer.finish().code
}

#[test]
fn function_declaration_seam_comment_stays_after_name() {
    // Vary the function name and parameter names to prove the rule is structural.
    for (name, p0, p1) in [("clone", "source", "target"), ("renamed", "m", "n")] {
        let source = format!("function {name} /* c */({p0}, {p1}) {{ return {p0}; }}\n");
        let output = emit(&source);
        assert!(
            output.contains(&format!("function {name} /* c */({p0}, {p1})")),
            "[{name}] seam comment must sit after the name, before `(`.\nOutput:\n{output}",
        );
        // It must NOT have leaked into the parameter list.
        assert!(
            !output.contains("(/* c */"),
            "[{name}] seam comment must not leak into the parameter list.\nOutput:\n{output}",
        );
    }
}

#[test]
fn function_expression_seam_comment_stays_after_name() {
    for name in ["copy", "fnExpr"] {
        let source = format!("const f = function {name} /* c */(a, b) {{ return a; }};\n");
        let output = emit(&source);
        assert!(
            output.contains(&format!("function {name} /* c */(a, b)")),
            "[{name}] function-expression seam comment must sit after the name.\nOutput:\n{output}",
        );
    }
}

#[test]
fn method_seam_comment_stays_after_name() {
    for name in ["duplicate", "renamedMethod"] {
        let source = format!("class C {{ {name} /* c */(x, y) {{ return x; }} }}\n");
        let output = emit(&source);
        assert!(
            output.contains(&format!("{name} /* c */(x, y)")),
            "[{name}] method seam comment must sit after the name.\nOutput:\n{output}",
        );
        assert!(
            !output.contains("(/* c */"),
            "[{name}] method seam comment must not leak into the parameter list.\nOutput:\n{output}",
        );
    }
}

#[test]
fn object_method_seam_comment_stays_after_name() {
    let source = "const o = { meth /* c */(a, b) { return a; } };\n";
    let output = emit(source);
    assert!(
        output.contains("meth /* c */(a, b)"),
        "object-method seam comment must sit after the name.\nOutput:\n{output}",
    );
}

#[test]
fn accessor_seam_comment_stays_after_name() {
    let source = "class D {\n    get val /* g */() { return 1; }\n    set val /* s */(v) { }\n}\n";
    let output = emit(source);
    assert!(
        output.contains("get val /* g */()"),
        "getter seam comment must sit after the name.\nOutput:\n{output}",
    );
    assert!(
        output.contains("set val /* s */(v)"),
        "setter seam comment must sit after the name.\nOutput:\n{output}",
    );
    assert!(
        !output.contains("(/* s */"),
        "setter seam comment must not leak into the parameter list.\nOutput:\n{output}",
    );
}

#[test]
fn comment_before_type_parameter_list_is_kept_at_name() {
    // `tsc` keeps a comment that sits between the name and the `<` of an erased
    // type-parameter list, attaching it to the name (the `<...>` is erased).
    for var in ["T", "K"] {
        let source = format!("function generic /* keep */<{var}>(a) {{ return a; }}\n");
        let output = emit(&source);
        assert!(
            output.contains("function generic /* keep */(a)"),
            "[{var}] pre-`<` seam comment must be kept at the name.\nOutput:\n{output}",
        );
    }
}

#[test]
fn comment_inside_type_parameter_list_is_erased() {
    // A comment *inside* the erased `<...>` must NOT be dragged into the seam:
    // vary the iteration variable name to prove the rule is structural.
    for var in ["T", "X"] {
        let source = format!("function tp<{var} /* drop */>(a) {{ return a; }}\n");
        let output = emit(&source);
        assert!(
            !output.contains("/* drop */"),
            "[{var}] type-parameter-internal comment must be erased.\nOutput:\n{output}",
        );
        assert!(
            output.contains("function tp(a)"),
            "[{var}] type parameters must be erased from JS output.\nOutput:\n{output}",
        );
    }
}

#[test]
fn empty_parameter_method_body_comment_is_not_dragged_into_seam() {
    // Regression guard: an empty-parameter method whose body holds a comment
    // containing `(` (e.g. `Echo("bar1")`). The parser can extend the name's
    // `end` past the real `(`; a naive `(` search would then match the body
    // comment's `(` and drag the body comment into the seam, corrupting the
    // method header into `bar1() { /*Echo("bar1");*/() { }`. The header must
    // stay structurally intact: name immediately followed by the real `()`.
    for name in ["bar1", "method"] {
        let source = format!("class Foo {{ {name}() {{ /*Echo(\"{name}\");*/ }} }}\n");
        let output = emit(&source);
        assert!(
            output.contains(&format!("{name}() ")),
            "[{name}] method header must stay intact: name then `()`.\nOutput:\n{output}",
        );
        // No body comment dragged in front of the real `(`.
        assert!(
            !output.contains(&format!("{name} /*")) && !output.contains("*/()"),
            "[{name}] body comment must not be dragged into the seam.\nOutput:\n{output}",
        );
    }
}

#[test]
fn no_seam_comment_leaves_output_unchanged() {
    // Negative case: when there is no inter-seam comment, the output is the
    // plain function with no spurious spacing or comments.
    let source = "function noSeam(p, q) { return p; }\n";
    let output = emit(source);
    assert!(
        output.contains("function noSeam(p, q)"),
        "function without a seam comment must emit normally.\nOutput:\n{output}",
    );
}
