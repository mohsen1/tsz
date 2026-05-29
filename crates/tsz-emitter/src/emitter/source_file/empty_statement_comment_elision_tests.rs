//! Fix B: an ASI-elidable empty statement preceded only by comments is elided
//! together with its leading comments — exactly as when no comments precede it.
//!
//! When a declaration has no source semicolon adjacent to its value, the parser
//! merges a following `;` (a separate empty statement) into the declaration's
//! range. Comments that appear after a line break between the declaration value
//! and that `;` are leading trivia of the elided empty statement, and tsc elides
//! them. These tests verify the elision is independent of whether comments
//! precede the empty statement, and that genuine same-line trailing comments and
//! explicit-semicolon comments are preserved.

use crate::emitter::JsxEmit;
use crate::output::printer::{PrintOptions, Printer};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit(file_name: &str, source: &str, options: PrintOptions) -> String {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = Printer::new(&parser.arena, options);
    printer.set_source_text(source);
    printer.print(root);
    printer.finish().code
}

fn es2015() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2015,
        ..Default::default()
    }
}

fn es2015_jsx() -> PrintOptions {
    PrintOptions {
        target: ScriptTarget::ES2015,
        jsx: JsxEmit::React,
        ..Default::default()
    }
}

#[test]
fn empty_statement_with_leading_line_comments_is_elided_with_them() {
    // `const x = {...}` has no source `;`; the trailing `;` is a separate empty
    // statement. Own-line comments between the value and the `;` are leading
    // trivia of the elided empty statement and must NOT be emitted; the `;`
    // collapses onto the declaration. Vary the declared name to prove the rule
    // is structural, not name-keyed.
    for name in ["x", "value", "fooBar"] {
        let source = format!("const {name} = {{ v: 1 }}\n// c1\n// c2\n;\nconst after = 2;\n");
        let output = emit("test.ts", &source, es2015());
        assert!(
            output.contains(&format!("const {name} = {{ v: 1 }};")),
            "[{name}] declaration should get a collapsed semicolon.\nOutput:\n{output}",
        );
        assert!(
            !output.contains("// c1") && !output.contains("// c2"),
            "[{name}] leading comments of the elided empty statement must be elided.\nOutput:\n{output}",
        );
        // No stray standalone `;` line.
        assert!(
            !output.contains("\n;"),
            "[{name}] elided empty statement must not emit a stray `;`.\nOutput:\n{output}",
        );
    }
}

#[test]
fn empty_statement_without_leading_comments_is_elided_unchanged() {
    // Baseline: no comments before the empty statement. The behavior must match
    // the comment case (declaration gets a collapsed `;`, empty statement gone).
    let source = "const x = { v: 1 }\n;\nconst after = 2;\n";
    let output = emit("test.ts", source, es2015());
    assert!(
        output.contains("const x = { v: 1 };"),
        "declaration should get a collapsed semicolon.\nOutput:\n{output}",
    );
    assert!(
        !output.contains("\n;"),
        "elided empty statement must not emit a stray `;`.\nOutput:\n{output}",
    );
}

#[test]
fn inline_comment_before_explicit_semicolon_is_preserved() {
    // Negative case: a same-line inline comment before an EXPLICIT `;` is a
    // genuine trailing comment of the declaration and must be preserved.
    let source = "const x = 1 /* inline */ ;\nconst after = 2;\n";
    let output = emit("test.ts", source, es2015());
    assert!(
        output.contains("/* inline */"),
        "inline trailing comment before explicit `;` must be preserved.\nOutput:\n{output}",
    );
    assert!(
        output.contains("const x = 1 /* inline */;"),
        "inline comment then explicit `;` should emit unchanged.\nOutput:\n{output}",
    );
}

#[test]
fn jsx_leading_comment_empty_statement_elision() {
    // The reported witness shape: `; <jsx>` after a comment-only block. The
    // empty `;` (merged into the preceding declaration) and its leading line
    // comments are elided. Vary the JSX tag to prove the rule is structural.
    for tag in ["a", "b", "video"] {
        let source = format!("const x = {{ v: 1 }}\n\n// note 1\n// note 2\n; <{tag}></{tag}>\n");
        let output = emit("file.tsx", &source, es2015_jsx());
        assert!(
            output.contains("const x = { v: 1 };"),
            "[{tag}] declaration should get a collapsed semicolon.\nOutput:\n{output}",
        );
        assert!(
            !output.contains("// note 1") && !output.contains("// note 2"),
            "[{tag}] leading comments of the elided empty statement must be elided.\nOutput:\n{output}",
        );
        assert!(
            !output.contains("\n;"),
            "[{tag}] elided empty statement must not emit a stray `;`.\nOutput:\n{output}",
        );
    }
}
