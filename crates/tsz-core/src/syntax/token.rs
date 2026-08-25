use crate::source::Span;

macro_rules! define_token_kinds {
    ($($keyword:ident => ($text:literal, $is_identifier:literal),)+) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum TokenKind {
            EndOfFile,
            Identifier,
            PrivateIdentifier,
            NumericLiteral,
            BigIntLiteral,
            StringLiteral,
            RegularExpressionLiteral,
            NoSubstitutionTemplateLiteral,
            TemplateHead,
            TemplateMiddle,
            TemplateTail,
            $($keyword,)+
            LeftBrace,
            RightBrace,
            LeftParen,
            RightParen,
            LeftBracket,
            RightBracket,
            Colon,
            Semicolon,
            Comma,
            Dot,
            DotDotDot,
            Question,
            QuestionDot,
            QuestionQuestion,
            Equals,
            FatArrow,
            Plus,
            PlusPlus,
            PlusEquals,
            Minus,
            MinusMinus,
            MinusEquals,
            Star,
            StarStar,
            StarEquals,
            StarStarEquals,
            Slash,
            SlashEquals,
            Percent,
            PercentEquals,
            Bar,
            BarBar,
            BarEquals,
            BarBarEquals,
            Ampersand,
            AmpersandAmpersand,
            AmpersandEquals,
            AmpersandAmpersandEquals,
            Caret,
            CaretEquals,
            LessThan,
            LessThanSlash,
            LessThanEquals,
            LessThanLessThan,
            LessThanLessThanEquals,
            GreaterThan,
            GreaterThanEquals,
            GreaterThanGreaterThan,
            GreaterThanGreaterThanEquals,
            GreaterThanGreaterThanGreaterThan,
            GreaterThanGreaterThanGreaterThanEquals,
            Bang,
            BangEquals,
            BangEqualsEquals,
            EqualsEquals,
            EqualsEqualsEquals,
            QuestionQuestionEquals,
            Tilde,
            At,
            Hash,
        }

        impl TokenKind {
            pub(super) fn from_keyword(text: &str) -> Self {
                match text {
                    $($text => Self::$keyword,)+
                    _ => Self::Identifier,
                }
            }

            /// Whether this token can be represented as an identifier node.
            ///
            /// TypeScript preserves strict-mode future-reserved words in the tree and
            /// diagnoses their legality later, when the surrounding strict/yield/await
            /// context is known. Keeping the spelling here also lets emit recover the
            /// source faithfully after a diagnostic.
            pub(crate) const fn is_identifier(self) -> bool {
                match self {
                    Self::Identifier => true,
                    $(Self::$keyword => $is_identifier,)+
                    _ => false,
                }
            }

            pub(crate) const fn is_identifier_name(self) -> bool {
                matches!(self, Self::Identifier $(| Self::$keyword)+)
            }
        }
    };
}

define_token_kinds! {
    Abstract => ("abstract", true),
    Accessor => ("accessor", true),
    Let => ("let", true),
    Const => ("const", false),
    Var => ("var", false),
    Await => ("await", true),
    Break => ("break", false),
    Case => ("case", false),
    Catch => ("catch", false),
    Class => ("class", false),
    Continue => ("continue", false),
    Debugger => ("debugger", false),
    Defer => ("defer", true),
    Delete => ("delete", false),
    Do => ("do", false),
    Else => ("else", false),
    Enum => ("enum", false),
    Extends => ("extends", false),
    Finally => ("finally", false),
    For => ("for", false),
    Function => ("function", false),
    If => ("if", false),
    Import => ("import", false),
    Implements => ("implements", true),
    In => ("in", false),
    InstanceOf => ("instanceof", false),
    New => ("new", false),
    Return => ("return", false),
    Super => ("super", false),
    Switch => ("switch", false),
    This => ("this", false),
    Throw => ("throw", false),
    Try => ("try", false),
    TypeOf => ("typeof", false),
    While => ("while", false),
    With => ("with", false),
    Yield => ("yield", true),
    Type => ("type", true),
    Interface => ("interface", true),
    Export => ("export", false),
    Default => ("default", false),
    Declare => ("declare", true),
    Async => ("async", true),
    Assert => ("assert", true),
    Asserts => ("asserts", true),
    Constructor => ("constructor", true),
    From => ("from", true),
    Get => ("get", true),
    Global => ("global", true),
    Infer => ("infer", true),
    Intrinsic => ("intrinsic", true),
    Is => ("is", true),
    Module => ("module", true),
    Namespace => ("namespace", true),
    Object => ("object", true),
    Of => ("of", true),
    Out => ("out", true),
    Override => ("override", true),
    Package => ("package", true),
    Private => ("private", true),
    Protected => ("protected", true),
    Public => ("public", true),
    Readonly => ("readonly", true),
    Require => ("require", true),
    Satisfies => ("satisfies", true),
    Set => ("set", true),
    Static => ("static", true),
    Symbol => ("symbol", true),
    Unique => ("unique", true),
    Using => ("using", true),
    True => ("true", false),
    False => ("false", false),
    Null => ("null", false),
    Undefined => ("undefined", true),
    Any => ("any", true),
    Unknown => ("unknown", true),
    Never => ("never", true),
    Void => ("void", false),
    Boolean => ("boolean", true),
    Number => ("number", true),
    String => ("string", true),
    BigInt => ("bigint", true),
    KeyOf => ("keyof", true),
    As => ("as", true),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
