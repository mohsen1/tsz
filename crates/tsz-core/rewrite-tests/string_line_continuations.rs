use std::path::PathBuf;
use std::sync::Arc;

use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    ClassMemberKind, ExpressionKind, Literal, StatementKind, StringLiteral, TypeMemberKind,
    TypeMemberNameKind, TypeNodeKind, parse_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn parse(source: &str) -> tsz::syntax::ParseOutput {
    parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("syntax.ts"),
        Arc::<str>::from(source),
    ))
}

fn parsed_string_value(raw: &str) -> String {
    let parsed = parse(&format!("const value = {raw};"));
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("expected one statement");
    };
    let StatementKind::Variable(declaration) = &statement.kind else {
        panic!("expected a variable declaration");
    };
    let Some(initializer) = &declaration.initializer else {
        panic!("expected an initializer");
    };
    let ExpressionKind::Literal(Literal::String(StringLiteral::Plain(value))) = &initializer.kind
    else {
        panic!("expected a plain string literal");
    };
    value.clone()
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn javascript(output: &tsz::CompileOutput) -> &str {
    output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .unwrap_or_else(|| panic!("missing JavaScript product: {output:?}"))
        .text
        .as_str()
}

#[test]
fn all_ecmascript_line_continuators_cook_to_the_same_scalar_value() {
    for (name, raw) in [
        ("lf", "'a\\\nb'"),
        ("crlf", "'a\\\r\nb'"),
        ("cr", "'a\\\rb'"),
        ("ls", "'a\\\u{2028}b'"),
        ("ps", "'a\\\u{2029}b'"),
        ("double-quote", "\"a\\\nb\""),
        ("multiple", "'a\\\nb\\\r\nc'"),
        ("indentation", "'a\\\n  b'"),
        ("empty", "'\\\n'"),
    ] {
        let expected = match name {
            "indentation" => "a  b",
            "empty" => "",
            "multiple" => "abc",
            _ => "ab",
        };
        assert_eq!(parsed_string_value(raw), expected, "{name}");
    }

    for (name, raw, expected) in [
        ("unescaped-ls", "'a\u{2028}b'", "a\u{2028}b"),
        ("unescaped-ps", "'a\u{2029}b'", "a\u{2029}b"),
    ] {
        assert_eq!(parsed_string_value(raw), expected, "{name}");
    }
}

#[test]
fn property_class_and_type_member_names_share_the_scanner_cooked_value() {
    let object = parse("const value = { 'te\\\nxt': 1 };");
    assert!(object.diagnostics.is_empty(), "{:?}", object.diagnostics);
    let [statement] = object.unit.statements.as_slice() else {
        panic!("expected one object statement");
    };
    let StatementKind::Variable(declaration) = &statement.kind else {
        panic!("expected object variable");
    };
    let Some(ExpressionKind::Object(properties)) = declaration
        .initializer
        .as_ref()
        .map(|initializer| &initializer.kind)
    else {
        panic!("expected object initializer");
    };
    assert_eq!(properties[0].name, "text");

    let class = parse("class Box { 'te\\\nxt' = 1; }");
    assert!(class.diagnostics.is_empty(), "{:?}", class.diagnostics);
    let [statement] = class.unit.statements.as_slice() else {
        panic!("expected one class statement");
    };
    let StatementKind::Class(declaration) = &statement.kind else {
        panic!("expected class declaration");
    };
    assert_eq!(declaration.members[0].name, "text");
    assert!(matches!(
        declaration.members[0].kind,
        ClassMemberKind::Property { .. }
    ));

    let alias = parse("type Shape = { 'te\\\nxt': string };");
    assert!(alias.diagnostics.is_empty(), "{:?}", alias.diagnostics);
    let [statement] = alias.unit.statements.as_slice() else {
        panic!("expected one type alias");
    };
    let StatementKind::TypeAlias(declaration) = &statement.kind else {
        panic!("expected type alias declaration");
    };
    let TypeNodeKind::Object(members) = &declaration.ty.kind else {
        panic!("expected object type");
    };
    let TypeMemberKind::Property { name, .. } = &members[0].kind else {
        panic!("expected property member");
    };
    assert!(matches!(
        &name.kind,
        TypeMemberNameKind::StringLiteral(value) if value == "text"
    ));
}

#[test]
fn module_specifiers_consume_the_same_cooked_value() {
    for source in [
        "import './mo\\\ndule';",
        "import './mo\\\r\ndule';",
        "import './mo\\\rdule';",
        "import './mo\\\u{2028}dule';",
        "import './mo\\\u{2029}dule';",
    ] {
        let parsed = parse(source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{source:?}: {:?}",
            parsed.diagnostics
        );
        let [statement] = parsed.unit.statements.as_slice() else {
            panic!("expected one import");
        };
        let StatementKind::Import(declaration) = &statement.kind else {
            panic!("expected import declaration");
        };
        assert_eq!(declaration.module_specifier, "./module", "{source:?}");
    }
}

#[test]
fn corpus_property_row_checks_clean_and_preserves_authored_javascript() {
    let source = "var x = {'text\\\n':'hello'}\nx.text = \"bar\"";
    for no_check in [false, true] {
        let output = Compiler::new().compile(
            vec![SourceInput::new(
                "stringLiteralPropertyNameWithLineContinuation1.ts",
                Arc::<str>::from(source),
            )],
            &CompilerOptions {
                target: "es2015".to_string(),
                no_check,
                ..CompilerOptions::default()
            },
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(output.exit_status, CompileExitStatus::Success);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(
            javascript(&output),
            "\"use strict\";\nvar x = { 'text\\\n': 'hello' };\nx.text = \"bar\";\n"
        );
    }
}

#[test]
fn doubled_backslashes_and_unterminated_continuations_do_not_launder_recovery() {
    for source in ["const value = 'a\\\\\nb';", "const value = 'a\\\r"] {
        let parsed = parse(source);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == 1002),
            "{source:?}: {:?}",
            parsed.diagnostics
        );
    }

    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "unterminated.ts",
            Arc::<str>::from("const value = 'a\\\r"),
        )],
        &CompilerOptions::default(),
    );
    assert!(codes(&output).contains(&1002));
}
