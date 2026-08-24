use crate::source::Span;

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
    Abstract,
    Accessor,
    Let,
    Const,
    Var,
    Await,
    Break,
    Case,
    Catch,
    Class,
    Continue,
    Debugger,
    Defer,
    Delete,
    Do,
    Else,
    Enum,
    Extends,
    Finally,
    For,
    Function,
    If,
    Import,
    Implements,
    In,
    InstanceOf,
    New,
    Return,
    Super,
    Switch,
    This,
    Throw,
    Try,
    TypeOf,
    While,
    With,
    Yield,
    Type,
    Interface,
    Export,
    Default,
    Declare,
    Async,
    Assert,
    Asserts,
    Constructor,
    From,
    Get,
    Global,
    Infer,
    Intrinsic,
    Is,
    Module,
    Namespace,
    Object,
    Of,
    Out,
    Override,
    Package,
    Private,
    Protected,
    Public,
    Readonly,
    Require,
    Satisfies,
    Set,
    Static,
    Symbol,
    Unique,
    Using,
    True,
    False,
    Null,
    Undefined,
    Any,
    Unknown,
    Never,
    Void,
    Boolean,
    Number,
    String,
    BigInt,
    KeyOf,
    As,
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
    /// Whether this token can be represented as an identifier node.
    ///
    /// TypeScript preserves strict-mode future-reserved words in the tree and
    /// diagnoses their legality later, when the surrounding strict/yield/await
    /// context is known. Keeping the spelling here also lets emit recover the
    /// source faithfully after a diagnostic.
    pub(crate) const fn is_identifier(self) -> bool {
        matches!(
            self,
            Self::Identifier
                | Self::Implements
                | Self::Interface
                | Self::Let
                | Self::Package
                | Self::Private
                | Self::Protected
                | Self::Public
                | Self::Static
                | Self::Yield
                | Self::Abstract
                | Self::Accessor
                | Self::As
                | Self::Asserts
                | Self::Assert
                | Self::Any
                | Self::Async
                | Self::Await
                | Self::Boolean
                | Self::Constructor
                | Self::Declare
                | Self::Get
                | Self::Infer
                | Self::Intrinsic
                | Self::Is
                | Self::KeyOf
                | Self::Module
                | Self::Namespace
                | Self::Never
                | Self::Out
                | Self::Readonly
                | Self::Require
                | Self::Number
                | Self::Object
                | Self::Satisfies
                | Self::Set
                | Self::String
                | Self::Symbol
                | Self::Type
                | Self::Undefined
                | Self::Unique
                | Self::Unknown
                | Self::Using
                | Self::From
                | Self::Global
                | Self::BigInt
                | Self::Override
                | Self::Of
                | Self::Defer
        )
    }

    pub(crate) const fn is_identifier_name(self) -> bool {
        self.is_identifier()
            || matches!(
                self,
                Self::Const
                    | Self::Var
                    | Self::Break
                    | Self::Case
                    | Self::Catch
                    | Self::Class
                    | Self::Continue
                    | Self::Debugger
                    | Self::Delete
                    | Self::Do
                    | Self::Else
                    | Self::Enum
                    | Self::Extends
                    | Self::Finally
                    | Self::For
                    | Self::Function
                    | Self::If
                    | Self::Import
                    | Self::In
                    | Self::InstanceOf
                    | Self::New
                    | Self::Return
                    | Self::Super
                    | Self::Switch
                    | Self::This
                    | Self::Throw
                    | Self::Try
                    | Self::TypeOf
                    | Self::While
                    | Self::With
                    | Self::Export
                    | Self::Default
                    | Self::True
                    | Self::False
                    | Self::Null
                    | Self::Void
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
