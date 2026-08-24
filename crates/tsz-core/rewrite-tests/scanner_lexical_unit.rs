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
