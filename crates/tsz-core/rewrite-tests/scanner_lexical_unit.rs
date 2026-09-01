use std::path::PathBuf;
use std::sync::Arc;

use crate::bind::{Meaning, ScopeId, bind_source_with_kind};
use crate::source::FileId;
use crate::syntax::{ExpressionKind, StatementKind, TypeNodeKind, parse_source};
use crate::{Compiler, CompilerOptions, SourceInput};

use super::*;

fn source(text: &str) -> SourceText {
    SourceText::new(
        FileId(7),
        PathBuf::from("scanner-case.ts"),
        Arc::<str>::from(text),
    )
}

fn scan(text: &str) -> (SourceText, ScanOutput) {
    let source = source(text);
    let output = scan_source(&source);
    (source, output)
}

fn checked_contextual_diagnostics(
    text: &str,
    no_check: bool,
) -> Vec<crate::diagnostics::Diagnostic> {
    Compiler::new()
        .compile(
            vec![SourceInput::new(
                "contextual-grammar.ts",
                Arc::<str>::from(text),
            )],
            &CompilerOptions {
                no_check,
                no_emit: true,
                target: "es2022".to_string(),
                ..CompilerOptions::default()
            },
        )
        .semantic_diagnostics
}

fn assert_one(text: &str, expected: TokenKind) {
    let (source, output) = scan(text);
    assert!(
        output.diagnostics.is_empty(),
        "unexpected diagnostics for {text:?}: {:?}",
        output.diagnostics
    );
    assert_eq!(
        output
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![expected, TokenKind::EndOfFile],
        "wrong token for {text:?}"
    );
    assert_eq!(source.slice(output.tokens[0].span), text);
    assert_eq!(output.tokens[0].span.start, 0);
    assert_eq!(output.tokens[0].span.end, text.len() as u32);
}

// Kept as a private scanner unit body so lexical coverage can inspect
// scanner-owned spans without counting test fixtures as compiler production.
#[test]
fn recognizes_the_complete_typescript_keyword_set() {
    let cases = [
        ("abstract", TokenKind::Abstract),
        ("accessor", TokenKind::Accessor),
        ("any", TokenKind::Any),
        ("as", TokenKind::As),
        ("assert", TokenKind::Assert),
        ("asserts", TokenKind::Asserts),
        ("async", TokenKind::Async),
        ("await", TokenKind::Await),
        ("bigint", TokenKind::BigInt),
        ("boolean", TokenKind::Boolean),
        ("break", TokenKind::Break),
        ("case", TokenKind::Case),
        ("catch", TokenKind::Catch),
        ("class", TokenKind::Class),
        ("const", TokenKind::Const),
        ("constructor", TokenKind::Constructor),
        ("continue", TokenKind::Continue),
        ("debugger", TokenKind::Debugger),
        ("declare", TokenKind::Declare),
        ("default", TokenKind::Default),
        ("defer", TokenKind::Defer),
        ("delete", TokenKind::Delete),
        ("do", TokenKind::Do),
        ("else", TokenKind::Else),
        ("enum", TokenKind::Enum),
        ("export", TokenKind::Export),
        ("extends", TokenKind::Extends),
        ("false", TokenKind::False),
        ("finally", TokenKind::Finally),
        ("for", TokenKind::For),
        ("from", TokenKind::From),
        ("function", TokenKind::Function),
        ("get", TokenKind::Get),
        ("global", TokenKind::Global),
        ("if", TokenKind::If),
        ("implements", TokenKind::Implements),
        ("import", TokenKind::Import),
        ("in", TokenKind::In),
        ("infer", TokenKind::Infer),
        ("instanceof", TokenKind::InstanceOf),
        ("interface", TokenKind::Interface),
        ("intrinsic", TokenKind::Intrinsic),
        ("is", TokenKind::Is),
        ("keyof", TokenKind::KeyOf),
        ("let", TokenKind::Let),
        ("module", TokenKind::Module),
        ("namespace", TokenKind::Namespace),
        ("never", TokenKind::Never),
        ("new", TokenKind::New),
        ("null", TokenKind::Null),
        ("number", TokenKind::Number),
        ("object", TokenKind::Object),
        ("of", TokenKind::Of),
        ("out", TokenKind::Out),
        ("override", TokenKind::Override),
        ("package", TokenKind::Package),
        ("private", TokenKind::Private),
        ("protected", TokenKind::Protected),
        ("public", TokenKind::Public),
        ("readonly", TokenKind::Readonly),
        ("require", TokenKind::Require),
        ("return", TokenKind::Return),
        ("satisfies", TokenKind::Satisfies),
        ("set", TokenKind::Set),
        ("static", TokenKind::Static),
        ("string", TokenKind::String),
        ("super", TokenKind::Super),
        ("switch", TokenKind::Switch),
        ("symbol", TokenKind::Symbol),
        ("this", TokenKind::This),
        ("throw", TokenKind::Throw),
        ("true", TokenKind::True),
        ("try", TokenKind::Try),
        ("type", TokenKind::Type),
        ("typeof", TokenKind::TypeOf),
        ("undefined", TokenKind::Undefined),
        ("unique", TokenKind::Unique),
        ("unknown", TokenKind::Unknown),
        ("using", TokenKind::Using),
        ("var", TokenKind::Var),
        ("void", TokenKind::Void),
        ("while", TokenKind::While),
        ("with", TokenKind::With),
        ("yield", TokenKind::Yield),
    ];
    for (text, kind) in cases {
        assert_one(text, kind);
    }

    // The pinned TS7 enum reserves `ImmediateKeyword`, but its scanner's
    // `textToKeywordObj` does not map the source spelling to that kind.
    assert_one("immediate", TokenKind::Identifier);
}

#[test]
fn recognizes_modern_punctuation_with_longest_match_spans() {
    let cases = [
        ("...", TokenKind::DotDotDot),
        ("?.", TokenKind::QuestionDot),
        ("??", TokenKind::QuestionQuestion),
        ("??=", TokenKind::QuestionQuestionEquals),
        ("++", TokenKind::PlusPlus),
        ("+=", TokenKind::PlusEquals),
        ("--", TokenKind::MinusMinus),
        ("-=", TokenKind::MinusEquals),
        ("**", TokenKind::StarStar),
        ("*=", TokenKind::StarEquals),
        ("**=", TokenKind::StarStarEquals),
        ("%", TokenKind::Percent),
        ("%=", TokenKind::PercentEquals),
        ("||", TokenKind::BarBar),
        ("|=", TokenKind::BarEquals),
        ("||=", TokenKind::BarBarEquals),
        ("&&", TokenKind::AmpersandAmpersand),
        ("&=", TokenKind::AmpersandEquals),
        ("&&=", TokenKind::AmpersandAmpersandEquals),
        ("^", TokenKind::Caret),
        ("^=", TokenKind::CaretEquals),
        ("</", TokenKind::LessThanSlash),
        ("<=", TokenKind::LessThanEquals),
        ("<<", TokenKind::LessThanLessThan),
        ("<<=", TokenKind::LessThanLessThanEquals),
        (">=", TokenKind::GreaterThanEquals),
        (">>", TokenKind::GreaterThanGreaterThan),
        (">>=", TokenKind::GreaterThanGreaterThanEquals),
        (">>>", TokenKind::GreaterThanGreaterThanGreaterThan),
        (">>>=", TokenKind::GreaterThanGreaterThanGreaterThanEquals),
        ("!=", TokenKind::BangEquals),
        ("!==", TokenKind::BangEqualsEquals),
        ("==", TokenKind::EqualsEquals),
        ("===", TokenKind::EqualsEqualsEquals),
        ("=>", TokenKind::FatArrow),
        ("~", TokenKind::Tilde),
        ("@", TokenKind::At),
        ("#", TokenKind::Hash),
    ];
    for (text, kind) in cases {
        assert_one(text, kind);
    }

    let (_, output) = scan("?.1");
    assert!(output.diagnostics.is_empty());
    assert_eq!(
        output
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![
            TokenKind::Question,
            TokenKind::NumericLiteral,
            TokenKind::EndOfFile,
        ]
    );
}

#[test]
fn scans_numeric_private_decorator_and_identifier_escape_forms() {
    let text = r"@sealed #field 0 1. 0.25 .5 1e3 1_000 0xff 0b1010 0o755 42n 0xffn \u{006e}ame";
    let (source, output) = scan(text);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let actual = output
        .tokens
        .iter()
        .map(|token| (token.kind, source.slice(token.span)))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            (TokenKind::At, "@"),
            (TokenKind::Identifier, "sealed"),
            (TokenKind::PrivateIdentifier, "#field"),
            (TokenKind::NumericLiteral, "0"),
            (TokenKind::NumericLiteral, "1."),
            (TokenKind::NumericLiteral, "0.25"),
            (TokenKind::NumericLiteral, ".5"),
            (TokenKind::NumericLiteral, "1e3"),
            (TokenKind::NumericLiteral, "1_000"),
            (TokenKind::NumericLiteral, "0xff"),
            (TokenKind::NumericLiteral, "0b1010"),
            (TokenKind::NumericLiteral, "0o755"),
            (TokenKind::BigIntLiteral, "42n"),
            (TokenKind::BigIntLiteral, "0xffn"),
            (TokenKind::Identifier, "\\u{006e}ame"),
            (TokenKind::EndOfFile, ""),
        ]
    );
}

#[test]
fn cooks_identifier_escapes_once_and_retains_authored_provenance() {
    let text = r"\u0052ow \u{0052}ow \u{102A7} a\u0062\u{0063} #\u0061 \u0069f \u{0069}f";
    let (source, output) = scan(text);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::PrivateIdentifier,
            TokenKind::If,
            TokenKind::If,
            TokenKind::EndOfFile,
        ]
    );
    assert_eq!(
        output
            .identifier_values
            .iter()
            .map(|identifier| {
                (
                    source.slice(identifier.span),
                    identifier.cooked.as_str(),
                    identifier.escape,
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (r"\u0052ow", "Row", IdentifierEscapeProvenance::Unicode,),
            (
                r"\u{0052}ow",
                "Row",
                IdentifierEscapeProvenance::ExtendedUnicode,
            ),
            (
                r"\u{102A7}",
                "𐊧",
                IdentifierEscapeProvenance::ExtendedUnicode,
            ),
            (
                r"a\u0062\u{0063}",
                "abc",
                IdentifierEscapeProvenance::UnicodeAndExtendedUnicode,
            ),
            (r"#\u0061", "#a", IdentifierEscapeProvenance::Unicode,),
            (r"\u0069f", "if", IdentifierEscapeProvenance::Unicode),
            (
                r"\u{0069}f",
                "if",
                IdentifierEscapeProvenance::ExtendedUnicode,
            ),
        ]
    );
    assert_eq!(
        output.identifier_values[5].escape,
        IdentifierEscapeProvenance::Unicode
    );
    assert_eq!(
        output.identifier_values[6].escape,
        IdentifierEscapeProvenance::ExtendedUnicode
    );
}

#[test]
fn rejects_each_fixed_width_surrogate_and_non_identifier_escape_independently() {
    // Pinned by `unicodeEscapesInNames02.ts` and
    // `invalidUnicodeEscapeSequance4.ts`: fixed-width surrogate escapes do
    // not combine, and an escape must be valid for its identifier position.
    let text = r"\uD800\uDEA7 \u0031a a\u002Dx";
    let (source, output) = scan(text);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (1127, 0, 0, "Invalid character."),
            (1127, 6, 0, "Invalid character."),
            (1127, 13, 0, "Invalid character."),
            (1127, 22, 0, "Invalid character."),
        ]
    );
    assert_eq!(
        output
            .tokens
            .iter()
            .map(|token| (token.kind, source.slice(token.span)))
            .collect::<Vec<_>>(),
        vec![
            (TokenKind::InvalidCharacter, "\\"),
            (TokenKind::Identifier, "uD800"),
            (TokenKind::InvalidCharacter, "\\"),
            (TokenKind::Identifier, "uDEA7"),
            (TokenKind::InvalidCharacter, "\\"),
            (TokenKind::Identifier, "u0031a"),
            (TokenKind::Identifier, "a"),
            (TokenKind::InvalidCharacter, "\\"),
            (TokenKind::Identifier, "u002Dx"),
            (TokenKind::EndOfFile, ""),
        ]
    );
    assert!(output.identifier_values.is_empty());
}

#[test]
fn unicode_identifier_classification_rejects_symbols_and_keeps_letters_and_joiners() {
    let text = "function f(a,¬) {} const λ=1; const joined=λ\u{200c}x;";
    let (source, output) = scan(text);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![(1127, 13, 1, "Invalid character.")]
    );
    assert!(
        output.tokens.iter().any(|token| {
            token.kind == TokenKind::Identifier && source.slice(token.span) == "λ"
        })
    );
    assert!(output.tokens.iter().any(|token| {
        token.kind == TokenKind::Identifier && source.slice(token.span) == "λ\u{200c}x"
    }));
}

#[test]
fn unicode_15_1_identifier_ranges_match_raw_and_escaped_typescript_names() {
    // U+037A is ID_Start but not XID_Start. U+088F was unassigned in Unicode
    // 15.1, so newer Unicode tables must not silently admit it.
    for text in ["\u{037a}x", "x\u{037a}", r"\u037Ax", r"x\u037A"] {
        assert_one(text, TokenKind::Identifier);
    }

    let (source, raw) = scan("\u{088f}x x\u{088f}");
    assert_eq!(
        raw.diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (1127, 0, 1, "Invalid character."),
            (1127, 4, 1, "Invalid character.")
        ],
    );
    assert_eq!(
        raw.tokens
            .iter()
            .map(|token| (token.kind, source.slice(token.span)))
            .collect::<Vec<_>>(),
        vec![
            (TokenKind::InvalidCharacter, "\u{088f}"),
            (TokenKind::Identifier, "x"),
            (TokenKind::Identifier, "x"),
            (TokenKind::InvalidCharacter, "\u{088f}"),
            (TokenKind::EndOfFile, ""),
        ],
    );

    let (source, escaped) = scan(r"\u088Fx x\u088F");
    assert_eq!(
        escaped
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (1127, 0, 0, "Invalid character."),
            (1127, 9, 0, "Invalid character.")
        ],
    );
    assert_eq!(
        escaped
            .tokens
            .iter()
            .map(|token| (token.kind, source.slice(token.span)))
            .collect::<Vec<_>>(),
        vec![
            (TokenKind::InvalidCharacter, "\\"),
            (TokenKind::Identifier, "u088Fx"),
            (TokenKind::Identifier, "x"),
            (TokenKind::InvalidCharacter, "\\"),
            (TokenKind::Identifier, "u088F"),
            (TokenKind::EndOfFile, ""),
        ],
    );
}

#[test]
fn escaped_keywords_are_diagnosed_only_in_keyword_grammar_positions() {
    let reserved = source(r"\u0069f (true) {}");
    let reserved_parse = parse_source(&reserved);
    assert!(matches!(
        reserved_parse.unit.statements.as_slice(),
        [statement] if matches!(&statement.kind, StatementKind::If(_))
    ));
    assert_eq!(reserved_parse.diagnostics.len(), 1);
    assert_eq!(
        (
            reserved_parse.diagnostics[0].code,
            reserved_parse.diagnostics[0].start,
            reserved_parse.diagnostics[0].length,
            reserved_parse.diagnostics[0].message_text.as_str(),
        ),
        (1260, 0, 7, "Keywords cannot contain escape characters.",)
    );
    assert_eq!(
        &reserved.text[..reserved_parse.diagnostics[0].length as usize],
        r"\u0069f"
    );

    let contextual = source(r"type typ\u{0065} = string;");
    let contextual_parse = parse_source(&contextual);
    assert!(
        contextual_parse.diagnostics.is_empty(),
        "escaped contextual keyword used as a name is legal: {:?}",
        contextual_parse.diagnostics
    );
    let [statement] = contextual_parse.unit.statements.as_slice() else {
        panic!("one contextual-keyword alias expected")
    };
    let StatementKind::TypeAlias(alias) = &statement.kind else {
        panic!("contextual-keyword name must remain a type alias")
    };
    assert_eq!(alias.name, "type");
    assert_eq!((alias.name_span.start, alias.name_span.end), (5, 16));
    assert_eq!(contextual.slice(alias.name_span), r"typ\u{0065}");

    // `scannerUnicodeEscapeInKeyword2.ts` pins contextual keywords as legal
    // identifier nodes in bindings, expressions, arrows, names, properties,
    // tuple labels, and the special `this` parameter.
    for text in [
        r"const typ\u0065 = 1;",
        r"const value = typ\u0065;",
        r"const value = \u0061sync;",
        r"const arrow = typ\u0065 => type;",
        r"const value = class typ\u0065 {};",
        r"const value = { def\u0061ult: 1 };",
        r"const value = object.def\u0061ult;",
        r"const value = `x${typ\u0065}`;",
        r"type Tuple = [typ\u0065: string];",
        r"function f(th\u0069s: string): void;",
        r"type Query = typeof th\u0069s;",
    ] {
        let source = source(text);
        let parsed = parse_source(&source);
        assert!(
            parsed.diagnostics.is_empty(),
            "escaped contextual identifier must remain legal in {text:?}: {:?}",
            parsed.diagnostics
        );
    }

    let expression = source(r"const value = typ\u0065;");
    let expression_parse = parse_source(&expression);
    let [statement] = expression_parse.unit.statements.as_slice() else {
        panic!("one contextual identifier expression expected")
    };
    let StatementKind::Variable(declaration) = &statement.kind else {
        panic!("a variable declaration was expected")
    };
    let Some(initializer) = &declaration.declarators[0].initializer else {
        panic!("the contextual identifier initializer must be retained")
    };
    let ExpressionKind::Identifier {
        name, name_span, ..
    } = &initializer.kind
    else {
        panic!("the initializer must remain an identifier expression")
    };
    assert_eq!(name, "type");
    assert_eq!(expression.slice(*name_span), r"typ\u0065");

    for (text, start, length) in [
        (r"\u0076ar x = 1;", 0, 8),
        (r"\u{0076}ar x = 1;", 0, 10),
        (r"\u0061sync function f() {}", 0, 10),
        (r"typ\u0065 NotOk = 0;", 0, 9),
        (r"type T = str\u0069ng;", 9, 11),
        (r"type T = th\u0069s;", 9, 9),
    ] {
        let source = source(text);
        let parsed = parse_source(&source);
        let matching = parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 1260)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "wrong TS1260 set for {text:?}");
        assert_eq!((matching[0].start, matching[0].length), (start, length));
        assert_eq!(
            matching[0].message_text,
            "Keywords cannot contain escape characters."
        );
    }
}

#[test]
fn escaped_hard_keywords_use_binding_diagnostics_without_a_false_ts1260() {
    for (text, code, start, authored, message) in [
        (
            r"const \u0069f = 1;",
            1389,
            "const ".len() as u32,
            r"\u0069f",
            "'if' is not allowed as a variable declaration name.",
        ),
        (
            r"function \u0072eturn() {}",
            1359,
            "function ".len() as u32,
            r"\u0072eturn",
            "Identifier expected. '\\u0072eturn' is a reserved word that cannot be used here.",
        ),
    ] {
        let source = source(text);
        let parsed = parse_source(&source);
        assert_eq!(
            parsed.diagnostics.first().map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            )),
            Some((code, start, authored.len() as u32, message)),
            "wrong escaped hard-keyword binding diagnostic for {text:?}",
        );
        assert!(
            parsed
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != 1260),
            "a BindingIdentifier is not a keyword-consumption position: {:?}",
            parsed.diagnostics,
        );
    }
}

#[test]
fn escaped_yield_is_a_keyword_only_inside_authored_generators() {
    let text = concat!(
        r"var \u0079ield = 0;",
        "\n",
        r"function *gen() {",
        "\n",
        r"  \u0079ield 1;",
        "\n",
        r"  function inner() { \u0079ield + 3; }",
        "\n",
        r"  const arrow = () => \u0079ield + 4;",
        "\n",
        r"}",
        "\n",
        r"function *renamed() { \u{0079}ield 2; }",
    );
    let source = source(text);
    let parsed = parse_source(&source);
    let fixed_start = text.find(r"\u0079ield 1").unwrap() as u32;
    let extended_start = text.find(r"\u{0079}ield 2").unwrap() as u32;
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                1260,
                fixed_start,
                r"\u0079ield".len() as u32,
                "Keywords cannot contain escape characters.",
            ),
            (
                1260,
                extended_start,
                r"\u{0079}ield".len() as u32,
                "Keywords cannot contain escape characters.",
            ),
        ]
    );

    let StatementKind::Function(generator) = &parsed.unit.statements[1].kind else {
        panic!("the first generator declaration must retain its function owner")
    };
    let StatementKind::Expression(expression) = &generator.body[0].kind else {
        panic!("the generator keyword must retain an expression node")
    };
    let ExpressionKind::Identifier {
        name, entity_name, ..
    } = &expression.kind
    else {
        panic!("the temporary generator model must retain the cooked keyword")
    };
    assert_eq!(name, "yield");
    assert!(!entity_name, "a grammar keyword is not a value reference");
}

#[test]
fn escaped_await_is_a_keyword_only_inside_authored_async_functions() {
    let text = concat!(
        r"var \u0061wait = 0;",
        "\n",
        r"async function main() {",
        "\n",
        r"  \u0061wait 12;",
        "\n",
        r"  function inner() { \u0061wait + 1; }",
        "\n",
        r"  const arrow = () => \u0061wait + 2;",
        "\n",
        r"}",
        "\n",
        r"const asyncArrow = async () => { \u{0061}wait 13; };",
    );
    let source = source(text);
    let parsed = parse_source(&source);
    let function_start = text.find(r"\u0061wait 12").unwrap() as u32;
    let arrow_start = text.find(r"\u{0061}wait 13").unwrap() as u32;
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 1260)
            .map(|diagnostic| (
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                function_start,
                r"\u0061wait".len() as u32,
                "Keywords cannot contain escape characters.",
            ),
            (
                arrow_start,
                r"\u{0061}wait".len() as u32,
                "Keywords cannot contain escape characters.",
            ),
        ]
    );
}

#[test]
fn escaped_await_uses_the_external_module_expression_context_without_reserving_bindings() {
    for (text, authored) in [
        (r"var \u0061wait = 1; \u0061wait; export {};", r"\u0061wait"),
        (
            r"var \u{0061}wait = 1; \u{0061}wait; export {};",
            r"\u{0061}wait",
        ),
    ] {
        let source = source(text);
        let parsed = parse_source(&source);
        let expression_start = text.rfind(authored).unwrap() as u32;
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
                .collect::<Vec<_>>(),
            vec![
                (1260, expression_start, authored.len() as u32),
                (
                    1109,
                    expression_start + authored.len() as u32,
                    ";".len() as u32,
                ),
            ],
            "wrong external-module await diagnostics for {text:?}",
        );
    }

    for path in ["case.cts", "case.mts", "case.cjs", "case.mjs"] {
        let text = r"var \u0061wait = 1; \u0061wait;";
        let source = SourceText::new(FileId(7), PathBuf::from(path), Arc::<str>::from(text));
        let parsed = parse_source(&source);
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![1260, 1109],
            "module-format extension must establish the await expression context for {path}",
        );
    }

    let source = source(concat!(
        "export {};",
        r"var \u0061wait = 1;",
        r"function nested() { var \u0061wait = 2; \u0061wait; }",
        r"class C { method() { var \u{0061}wait = 3; \u{0061}wait; } }",
    ));
    let parsed = parse_source(&source);
    assert!(
        parsed.diagnostics.is_empty(),
        "bindings and ordinary function/method bodies reset the module await expression context: {:?}",
        parsed.diagnostics,
    );
}

#[test]
fn escaped_await_and_yield_bindings_follow_function_grammar_contexts() {
    for (text, code, message) in [
        (
            r"async function f(\u0061wait: number) {}",
            1359,
            "Identifier expected. '\\u0061wait' is a reserved word that cannot be used here.",
        ),
        (
            r"async function outer() { function \u0061wait() {} }",
            1359,
            "Identifier expected. '\\u0061wait' is a reserved word that cannot be used here.",
        ),
        (
            r"function *g(\u0079ield: number) {}",
            1212,
            "Identifier expected. '\\u0079ield' is a reserved word in strict mode.",
        ),
        (
            r"function *\u0079ield() {}",
            1212,
            "Identifier expected. '\\u0079ield' is a reserved word in strict mode.",
        ),
        (
            r"function *g<\u0079ield>() {}",
            1212,
            "Identifier expected. '\\u0079ield' is a reserved word in strict mode.",
        ),
        (
            r"function *outer() { function inner(\u0079ield: number) {} }",
            1212,
            "Identifier expected. '\\u0079ield' is a reserved word in strict mode.",
        ),
        (
            r"class C { *m(\u0079ield: number) {} }",
            1213,
            "Identifier expected. '\\u0079ield' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
        ),
        (
            r"class C { async m(\u0061wait: number) {} }",
            1359,
            "Identifier expected. '\\u0061wait' is a reserved word that cannot be used here.",
        ),
    ] {
        let diagnostics = checked_contextual_diagnostics(text, false);
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
                .collect::<Vec<_>>(),
            vec![(code, message)],
            "wrong contextual binding diagnostics for {text:?}",
        );
        assert!(
            checked_contextual_diagnostics(text, true).is_empty(),
            "--noCheck must suppress contextual binding diagnostics for {text:?}",
        );
    }

    for text in [
        r"async function \u0061wait() {}",
        r"async function f<\u0061wait>() {}",
        r"async function outer() { function inner(\u0061wait: number) {} }",
        r"async function f() { const o = { \u0061wait: 1 }; }",
        r"function *g() { const o = { \u0079ield: 1 }; }",
        r"class C { \u0061wait() {} \u0079ield() {} }",
    ] {
        let source = source(text);
        let parsed = parse_source(&source);
        assert!(
            parsed.diagnostics.is_empty(),
            "IdentifierName/property contexts remain legal in {text:?}: {:?}",
            parsed.diagnostics,
        );
    }
}

#[test]
fn class_strict_context_covers_the_whole_definition_and_then_resets() {
    for (text, authored) in [
        (r"class \u0079ield {}", r"\u0079ield"),
        (r"class \u{0079}ield {}", r"\u{0079}ield"),
        (r"class C<\u0079ield> {}", r"\u0079ield"),
        (r"class C<\u{0079}ield> {}", r"\u{0079}ield"),
        (r"class C extends \u0079ield {}", r"\u0079ield"),
        (r"class C extends \u{0079}ield {}", r"\u{0079}ield"),
        (r"class C implements \u0079ield {}", r"\u0079ield"),
        (r"class C implements \u{0079}ield {}", r"\u{0079}ield"),
        (r"const value = class \u0079ield {};", r"\u0079ield"),
        (r"const value = class C<\u{0079}ield> {};", r"\u{0079}ield"),
        (r"const value = class extends \u0079ield {};", r"\u0079ield"),
        (r"class C { field = \u0079ield; }", r"\u0079ield"),
        (r"class C { field = \u{0079}ield; }", r"\u{0079}ield"),
        (r"class C { method() { \u0079ield; } }", r"\u0079ield"),
        (r"class C { method() { \u{0079}ield; } }", r"\u{0079}ield"),
    ] {
        let diagnostics = checked_contextual_diagnostics(text, false);
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.message_text.clone()))
                .collect::<Vec<_>>(),
            vec![(
                1213,
                format!(
                    "Identifier expected. '{authored}' is a reserved word in strict mode. Class definitions are automatically in strict mode."
                ),
            )],
            "wrong class strict-context diagnostics for {text:?}",
        );
        assert!(
            checked_contextual_diagnostics(text, true).is_empty(),
            "--noCheck must suppress class strict-context diagnostics for {text:?}",
        );
    }

    let trailing_source = source(r"class C {} var \u0079ield = 1;");
    let parsed = parse_source(&trailing_source);
    assert!(
        parsed.diagnostics.is_empty(),
        "class strict context must not leak into the following variable: {:?}",
        parsed.diagnostics,
    );

    let source = source(concat!(
        "export {};",
        r"class C { field = \u0061wait; method() { \u{0061}wait; } }",
    ));
    let parsed = parse_source(&source);
    assert!(
        parsed.diagnostics.is_empty(),
        "module await context must not leak into ordinary class fields or methods: {:?}",
        parsed.diagnostics,
    );
}

#[test]
fn generator_yield_context_distinguishes_class_bindings_from_heritage_and_decorators() {
    for text in [
        "function *parameterized() { class Owner<yield> {} }",
        "function *outer() { class Renamed extends (yield 1) {} }",
        "function *implemented() { class Owner implements yield {} }",
        "function *changed() { const value = class Nested extends (yield) {}; }",
        "function *wrapped() { { class Inner extends (yield 2) {} } }",
        "function *decorated() { class Owner { @(yield 'tag') method() {} } }",
        "function ordinary() { class Owner { @(yield 'tag') method() {} } }",
    ] {
        for no_check in [false, true] {
            let diagnostics = checked_contextual_diagnostics(text, no_check);
            assert!(
                diagnostics
                    .iter()
                    .all(|diagnostic| !matches!(diagnostic.code, 1212 | 1213)),
                "a class heritage or recovered decorator expression must not publish a strict binding diagnostic in {text:?}: {diagnostics:?}",
            );
        }
    }

    for text in [
        "function *named() { class yield {} }",
        "function *outer() { const value = class yield {}; }",
        "function ordinary() { class Direct extends yield {} }",
        "function *outer() { function nested() { class Reset extends yield {} } }",
        "function *outer() { class Owner { method(yield: number) {} } }",
        "function *outer() { class Owner { field = yield; } }",
    ] {
        let expected_start = text.find("yield").unwrap() as u32;
        let diagnostics = checked_contextual_diagnostics(text, false);
        let contextual = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 1213)
            .collect::<Vec<_>>();
        assert_eq!(
            contextual
                .iter()
                .map(|diagnostic| (
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.message_text.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![(
                1213,
                expected_start,
                "yield".len() as u32,
                "Identifier expected. 'yield' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
            )],
            "an ordinary function must reset the surrounding generator's Yield context in {text:?}",
        );
        assert!(
            checked_contextual_diagnostics(text, true)
                .iter()
                .all(|diagnostic| diagnostic.code != 1213),
            "--noCheck must suppress the contextual class-strict diagnostic for {text:?}",
        );
    }
}

#[test]
fn async_parenthesized_arrow_parameters_enter_await_context_before_the_body() {
    for (text, authored) in [
        (r"const fixed = async (\u0061wait) => 1;", r"\u0061wait"),
        (
            r"const extended = async (\u{0061}wait) => 2;",
            r"\u{0061}wait",
        ),
    ] {
        let diagnostics = checked_contextual_diagnostics(text, false);
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| (diagnostic.code, diagnostic.message_text.clone()))
                .collect::<Vec<_>>(),
            vec![(
                1359,
                format!(
                    "Identifier expected. '{authored}' is a reserved word that cannot be used here."
                ),
            )],
            "wrong async-arrow parameter diagnostics for {text:?}",
        );
        assert!(
            checked_contextual_diagnostics(text, true).is_empty(),
            "--noCheck must suppress async-arrow parameter diagnostics for {text:?}",
        );
    }

    let source = source(r"const nested = async () => () => \u0061wait + 4;");
    let parsed = parse_source(&source);
    assert!(
        parsed.diagnostics.is_empty(),
        "an ordinary nested arrow resets the await body context: {:?}",
        parsed.diagnostics,
    );
}

#[test]
fn ordinary_arrow_parameters_inherit_outer_await_and_yield_keyword_contexts() {
    for (text, authored) in [
        (
            r"async function outer() { const f = (\u0061wait: number) => 0; }",
            r"\u0061wait",
        ),
        (
            r"async function outer() { const f = (\u{0061}wait: number) => 0; }",
            r"\u{0061}wait",
        ),
        (
            r"function *outer() { const f = (\u0079ield: number) => 0; }",
            r"\u0079ield",
        ),
        (
            r"function *outer() { const f = (\u{0079}ield: number) => 0; }",
            r"\u{0079}ield",
        ),
    ] {
        let source = source(text);
        let parsed = parse_source(&source);
        let expected_start = text.find(authored).unwrap() as u32;
        assert_eq!(
            parsed
                .diagnostics
                .iter()
                .filter(|diagnostic| matches!(diagnostic.code, 1212 | 1260 | 1359))
                .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
                .collect::<Vec<_>>(),
            vec![(1260, expected_start, authored.len() as u32)],
            "ordinary-arrow parameters must consume inherited await/yield as keywords in {text:?}",
        );
    }
}

#[test]
fn cooked_identifier_names_reach_binder_identity_without_losing_raw_spans() {
    let source = source(r"type \u0052ow=keyof {tag:number};type Box<\u0052ow>=[Row,\u{0052}ow];");
    let parsed = parse_source(&source);
    assert!(
        parsed.diagnostics.is_empty(),
        "unexpected parser diagnostics: {:?}",
        parsed.diagnostics
    );
    let [_, box_alias] = parsed.unit.statements.as_slice() else {
        panic!("two type aliases expected")
    };
    let StatementKind::TypeAlias(alias) = &box_alias.kind else {
        panic!("Box must parse as a type alias")
    };
    let [parameter] = alias.type_parameters.as_slice() else {
        panic!("one type parameter expected")
    };
    assert_eq!(parameter.name, "Row");
    assert_eq!(source.slice(parameter.name_span), r"\u0052ow");
    let TypeNodeKind::Tuple(elements) = &alias.ty.kind else {
        panic!("tuple alias body expected")
    };
    let [plain, escaped] = elements.as_slice() else {
        panic!("two tuple elements expected")
    };
    let TypeNodeKind::Reference {
        name: plain_name,
        name_span: plain_span,
        ..
    } = &plain.kind
    else {
        panic!("plain reference expected")
    };
    let TypeNodeKind::Reference {
        name: escaped_name,
        name_span: escaped_span,
        ..
    } = &escaped.kind
    else {
        panic!("escaped reference expected")
    };
    assert_eq!((plain_name.as_str(), escaped_name.as_str()), ("Row", "Row"));
    assert_eq!(source.slice(*plain_span), "Row");
    assert_eq!(source.slice(*escaped_span), r"\u{0052}ow");

    let bindings = bind_source_with_kind(
        source.id,
        crate::source::SourceKind::TypeScript,
        &parsed.unit,
    );
    let declaration = bindings
        .resolve(ScopeId(0), "Row", Meaning::Type)
        .and_then(|declaration| bindings.declaration(declaration))
        .expect("the cooked alias identity must resolve through the binder");
    assert_eq!(declaration.name, "Row");
    assert_eq!(source.slice(declaration.name_span), r"\u0052ow");
    assert_eq!(
        bindings.resolve(ScopeId(0), r"\u0052ow", Meaning::Type),
        None,
        "authored escape spelling must not become a second semantic name"
    );
}

#[test]
fn scans_nested_template_chunks_without_losing_delimiter_spans() {
    assert_one("`plain`", TokenKind::NoSubstitutionTemplateLiteral);

    let text = "`first ${left} middle ${right} last`";
    let (source, output) = scan(text);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let actual = output
        .tokens
        .iter()
        .map(|token| (token.kind, source.slice(token.span)))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            (TokenKind::TemplateHead, "`first ${"),
            (TokenKind::Identifier, "left"),
            (TokenKind::TemplateMiddle, "} middle ${"),
            (TokenKind::Identifier, "right"),
            (TokenKind::TemplateTail, "} last`"),
            (TokenKind::EndOfFile, ""),
        ]
    );

    let text = "`outer ${value + `inner ${item}`} tail`";
    let (source, output) = scan(text);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let actual = output
        .tokens
        .iter()
        .map(|token| (token.kind, source.slice(token.span)))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            (TokenKind::TemplateHead, "`outer ${"),
            (TokenKind::Identifier, "value"),
            (TokenKind::Plus, "+"),
            (TokenKind::TemplateHead, "`inner ${"),
            (TokenKind::Identifier, "item"),
            (TokenKind::TemplateTail, "}`"),
            (TokenKind::TemplateTail, "} tail`"),
            (TokenKind::EndOfFile, ""),
        ]
    );
}

#[test]
fn scans_regex_literals_without_stealing_division_tokens() {
    let text = r"const pattern = /a\/[b-d]+/giu; value / divisor; return /x\/y/;";
    let (source, output) = scan(text);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let actual = output
        .tokens
        .iter()
        .map(|token| (token.kind, source.slice(token.span)))
        .collect::<Vec<_>>();
    assert!(actual.contains(&(TokenKind::RegularExpressionLiteral, r"/a\/[b-d]+/giu")));
    assert!(actual.contains(&(TokenKind::Slash, "/")));
    assert!(actual.contains(&(TokenKind::RegularExpressionLiteral, r"/x\/y/")));
}

#[test]
fn contextual_type_ends_do_not_steal_prefix_or_relational_regex_literals() {
    let text = concat!(
        r"async function f() { return await /[a-z\/]+/; }",
        "\n",
        r"function* g() { yield /[a-z\/]+/; }",
        "\n",
        r"void /[a-z\/]+/;",
        "\n",
        r"value > /[a-z\/]+/.source;",
    );
    let (source, output) = scan(text);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.regular_expression_literals.len(), 4);
    assert_eq!(
        output
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::RegularExpressionLiteral)
            .map(|token| source.slice(token.span))
            .collect::<Vec<_>>(),
        vec![r"/[a-z\/]+/"; 4],
    );
}

#[test]
fn records_regex_pattern_flag_spans_and_bounded_unterminated_recovery() {
    let (source, output) = scan("var pattern = /abc/g;");
    let [scanned] = output.regular_expression_literals.as_slice() else {
        panic!("expected one scanned regular expression");
    };
    let literal = scanned.syntax_literal();
    assert_eq!(literal.raw, "/abc/g");
    assert_eq!(literal.pattern, "abc");
    assert_eq!(literal.flags, "g");
    assert_eq!(source.slice(literal.pattern_span), "abc");
    assert_eq!(source.slice(literal.flags_span), "g");
    assert!(literal.terminated);
    assert!(literal.validation_supported());

    let (source, output) = scan("var r = /abc");
    assert_eq!(output.diagnostics.len(), 1);
    assert_eq!(
        (
            output.diagnostics[0].code,
            output.diagnostics[0].start,
            output.diagnostics[0].length,
            output.diagnostics[0].message_text.as_str(),
        ),
        (1161, 8, 4, "Unterminated regular expression literal.")
    );
    let token = output
        .tokens
        .iter()
        .find(|token| token.kind == TokenKind::RegularExpressionLiteral)
        .expect("unterminated recovery remains one regex token");
    assert_eq!(source.slice(token.span), "/abc");
    let literal = output.regular_expression_literals[0].syntax_literal();
    assert!(!literal.terminated);
    assert!(literal.validation_supported());

    let (source, slash_equals_recovery) = scan("/=");
    assert_eq!(slash_equals_recovery.diagnostics[0].code, 1161);
    assert_eq!(
        source.slice(slash_equals_recovery.tokens[0].span),
        "/=",
        "a primary-position slash-equals follows TS7 regex recovery"
    );
    assert_eq!(
        slash_equals_recovery.tokens[0].kind,
        TokenKind::RegularExpressionLiteral
    );

    for text in ["var r = /abc\n", "var r = /abc\u{2028}"] {
        let (_, line_break_recovery) = scan(text);
        let literal = line_break_recovery.regular_expression_literals[0].syntax_literal();
        assert!(literal.recovery_at_line_break, "{text:?}");
        assert!(!literal.validation_supported(), "{text:?}");
    }

    let (_, open_class) = scan("var r = /[/");
    let literal = open_class.regular_expression_literals[0].syntax_literal();
    assert!(!literal.terminated);
    assert!(!literal.validation_supported());
}

#[test]
fn preserves_unterminated_string_and_comment_diagnostics() {
    let (_, string_output) = scan("\"oops");
    assert_eq!(string_output.diagnostics.len(), 1);
    assert_eq!(
        (
            string_output.diagnostics[0].code,
            string_output.diagnostics[0].start,
            string_output.diagnostics[0].length,
            string_output.diagnostics[0].message_text.as_str(),
        ),
        (1002, 0, 5, "Unterminated string literal.")
    );

    let comment = "/* never closes";
    let (_, comment_output) = scan(comment);
    assert_eq!(comment_output.diagnostics.len(), 1);
    assert_eq!(
        (
            comment_output.diagnostics[0].code,
            comment_output.diagnostics[0].start,
            comment_output.diagnostics[0].length,
            comment_output.diagnostics[0].message_text.as_str(),
        ),
        (1010, 0, comment.len() as u32, "'*/' expected.")
    );
}

#[test]
fn valid_modern_lexical_forms_do_not_report_invalid_characters() {
    let text = r#"#!/usr/bin/env node
            import value, { type Shape as Alias } from "pkg";
            @sealed export class Box<T> extends Base implements Shape {
                #value = 0xffn;
                method(...items: T[]) {
                    return this?.#value ?? /[a-z\/]++/v;
                }
            }
        "#;
    let (_, output) = scan(text);
    assert!(
        output
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 1127),
        "valid lexical forms produced TS1127: {:?}",
        output.diagnostics
    );
}

#[test]
fn no_check_preserves_contextual_async_binding_products_without_diagnostics() {
    let source = concat!(
        r"export async function task(\u0061wait: number): Promise<void> {}",
        "\n",
        r"export class Worker { async run(\u{0061}wait: number): Promise<void> {} }",
        "\n",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "contextual-async.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            declaration: true,
            no_check: true,
            module: "esnext".to_string(),
            target: "es2017".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
    assert_eq!(
        output.semantic_completion,
        crate::program::SemanticCompletion::Complete
    );
    assert_eq!(output.exit_status.code(), 0);
    let product = |path: &str| {
        output
            .emitted_files
            .iter()
            .find(|file| file.path.to_string_lossy() == path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .text
            .as_str()
    };
    assert_eq!(
        product("contextual-async.js"),
        concat!(
            r"export async function task(\u0061wait) { }",
            "\n",
            "export class Worker {\n",
            r"    async run(\u{0061}wait) { }",
            "\n}\n",
        ),
    );
    assert_eq!(
        product("contextual-async.d.ts"),
        concat!(
            r"export declare function task(\u0061wait: number): Promise<void>;",
            "\n",
            "export declare class Worker {\n",
            r"    run(\u{0061}wait: number): Promise<void>;",
            "\n}\n",
        ),
    );
}

#[test]
fn classifies_pinned_reference_and_detached_source_trivia() {
    let (_, output) = scan(concat!(
        "/*! detached pinned */\n\n",
        "/*! attached pinned */\n",
        "/// <reference path=\"./types.d.ts\" />\n",
        "/// ordinary triple slash\n",
        "/** @license ordinary */\n",
        "const value = 1;",
    ));
    assert_eq!(
        output
            .comments
            .iter()
            .map(|comment| comment.class)
            .collect::<Vec<_>>(),
        vec![
            CommentClass::DetachedPinned,
            CommentClass::Pinned,
            CommentClass::TripleSlashReference,
            CommentClass::Ordinary,
            CommentClass::Ordinary,
        ]
    );
}
