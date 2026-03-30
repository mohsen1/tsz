//! Parser - Cache-optimized parser using `NodeArena`
//!
//! This parser uses the Node architecture (16 bytes per node vs 208 bytes)
//! for 13x better cache locality. It produces the same AST semantically
//! but stored in a more efficient format.
//!
//! # Architecture
//!
//! - Uses `NodeArena` instead of `NodeArena`
//! - Each node is 16 bytes (vs 208 bytes for fat Node enum)
//! - Node data is stored in separate typed pools
//! - 4 nodes fit per 64-byte cache line (vs 0.31 for fat nodes)

use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::limits::MAX_PARSER_RECURSION_DEPTH;

use crate::parser::{
    NodeIndex, NodeList,
    node::{IdentifierData, NodeArena},
    syntax_kind_ext,
};
use rustc_hash::FxHashMap;
use tracing::trace;
use tsz_common::interner::Atom;
use tsz_scanner::scanner_impl::{ScannerState, TokenFlags};
use tsz_scanner::{SyntaxKind, token_is_keyword};
// =============================================================================
// Parser Context Flags
// =============================================================================

/// Context flag: inside an async function/method/arrow
pub const CONTEXT_FLAG_ASYNC: u32 = 1;
/// Context flag: inside a generator function/method
pub const CONTEXT_FLAG_GENERATOR: u32 = 2;
/// Context flag: inside a static block (where 'await' is reserved)
pub const CONTEXT_FLAG_STATIC_BLOCK: u32 = 4;
/// Context flag: parsing a parameter default (where 'await' is not allowed)
pub const CONTEXT_FLAG_PARAMETER_DEFAULT: u32 = 8;
/// Context flag: disallow 'in' as a binary operator (for for-statement initializers)
pub const CONTEXT_FLAG_DISALLOW_IN: u32 = 16;
/// Context flag: parsing the `true` branch of a conditional expression.
/// Suppresses type-annotated single-parameter arrow lookahead while
/// that colon belongs to the surrounding conditional operator.
pub const CONTEXT_FLAG_IN_CONDITIONAL_TRUE: u32 = 64;
/// Context flag: parsing a class member name.
pub const CONTEXT_FLAG_CLASS_MEMBER_NAME: u32 = 2048;
/// Context flag: inside an ambient context (declare namespace/module)
pub const CONTEXT_FLAG_AMBIENT: u32 = 32;
/// Context flag: parsing a class body
pub const CONTEXT_FLAG_IN_CLASS: u32 = 4096;
/// Context flag: inside a decorator expression (@expr)
/// When set, `[` should not be treated as element access (it starts a computed property name)
pub const CONTEXT_FLAG_IN_DECORATOR: u32 = 128;
/// Context flag: parsing parameters of a class constructor.
pub const CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS: u32 = 256;
/// Context flag: parsing arrow function parameters.
pub const CONTEXT_FLAG_ARROW_PARAMETERS: u32 = 512;
/// Context flag: disallow conditional types (used inside `infer T extends X` constraint parsing).
/// When set, `T extends U ? X : Y` is not parsed as a conditional type.
pub const CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES: u32 = 1024;
/// Context flag: inside a block statement (function body, bare block, if/while/for body).
/// When set, modifiers like `export` and `declare` are not allowed and emit TS1184.
pub const CONTEXT_FLAG_IN_BLOCK: u32 = 8192;
/// Context flag: parsing inside a parenthesized expression.
/// Used to keep arrow-function/parenthesized recovery behavior consistent.
pub const CONTEXT_FLAG_IN_PARENTHESIZED_EXPRESSION: u32 = 16384;
/// Context flag: parsing a class field initializer.
/// Newline-separated postfix continuations like `\n[` do not attach across fields.
pub const CONTEXT_FLAG_CLASS_FIELD_INITIALIZER: u32 = 32768;
/// Context flag: parsing inside a tuple element where `?` is an optional marker.
/// When set, postfix `?` should NOT be treated as JSDoc nullable (TS17019).
pub const CONTEXT_FLAG_IN_TUPLE_ELEMENT: u32 = 65536;
/// Context flag: parsing the property name of a generator method (`* [name]`).
/// Suppresses TS1213 for `yield` in computed property names of generator methods
/// (tsc does not emit TS1213 in this position).
pub const CONTEXT_FLAG_GENERATOR_MEMBER_NAME: u32 = 131072;

// =============================================================================
// Parse Diagnostic
// =============================================================================

/// A parse-time diagnostic (error or warning).
#[derive(Clone, Debug)]
pub struct ParseDiagnostic {
    pub start: u32,
    pub length: u32,
    pub message: String,
    pub code: u32,
}

pub struct IncrementalParseResult {
    pub statements: NodeList,
    pub end_pos: u32,
    pub end_of_file_token: NodeIndex,
    pub reparse_start: u32,
}

// =============================================================================
// ParserState
// =============================================================================

/// A high-performance parser using Node architecture.
///
/// Error suppression distance in tokens
///
/// If we emitted an error within this distance, suppress subsequent errors
/// to prevent cascading TS1005 and other noise errors.
///
/// This value was chosen empirically to match TypeScript's behavior:
/// - Too small: Cascading errors aren't suppressed effectively
/// - Too large: Genuine secondary errors are suppressed
const ERROR_SUPPRESSION_DISTANCE: u32 = 3;

/// This parser produces the same AST semantically as `ParserState`,
/// but uses the cache-optimized `NodeArena` for storage.
pub struct ParserState {
    /// The scanner for tokenizing
    pub(crate) scanner: ScannerState,
    /// Arena for allocating Nodes
    pub arena: NodeArena,
    /// Source file name
    pub(crate) file_name: String,
    /// Parser context flags
    pub context_flags: u32,
    /// Current token
    pub(crate) current_token: SyntaxKind,
    /// List of parse diagnostics
    pub(crate) parse_diagnostics: Vec<ParseDiagnostic>,
    /// Node count for assigning IDs
    pub(crate) node_count: u32,
    /// Recursion depth for stack overflow protection
    pub(crate) recursion_depth: u32,
    /// Position of last error (to prevent cascading errors at same position)
    pub(crate) last_error_pos: u32,
    /// Stack of label scopes for duplicate label detection (TS1114)
    /// Each scope is a map from label name to the position where it was first defined
    pub(crate) label_scopes: Vec<FxHashMap<String, u32>>,
    /// Whether a top-level import/export has been seen in the current file.
    pub(crate) seen_module_indicator: bool,
    /// Whether the most recently parsed named import list consumed its closing brace.
    pub(crate) last_named_imports_consumed_closing_brace: bool,
    /// Whether the most recently parsed named import list recovered directly to
    /// a `from` clause after a missing closing brace.
    pub(crate) last_named_imports_recovered_to_from: bool,
    /// Whether the most recently parsed named import list hit a structural
    /// recovery path rather than a semantic-only specifier error.
    pub(crate) last_named_imports_had_structural_error: bool,
    /// When recovery consumes a malformed arrow-body `}` directly, keep a small
    /// number of following module-closing braces in the token stream so outer
    /// list recovery can report them as stray braces.
    pub(crate) deferred_module_close_braces: u32,
    /// When malformed import-attribute recovery breaks a type constituent,
    /// stop consuming `&`-continued intersections so the tail falls back to
    /// statement-level recovery like TypeScript.
    pub(crate) abort_intersection_continuation: bool,
    /// After malformed import-attribute recovery inside an intersection type,
    /// parse the next `import()` options object with generic expression
    /// grammar so its diagnostics degrade like TypeScript's fallback path.
    pub(crate) fallback_import_type_options_once: bool,
    /// Parse `import()` options using type-import attribute grammar instead of
    /// generic object-literal expression grammar.
    pub(crate) in_import_type_options_context: bool,
    /// Malformed type-import attribute recovery consumed the import call tail
    /// through `).Name`, so `parse_import_expression` must not expect `)` again.
    pub(crate) import_attribute_tail_recovered: bool,
    /// After a missing object-literal property initializer, allow the next
    /// line-broken property-like token to continue without a synthetic comma error.
    pub(crate) suppress_object_literal_comma_once: bool,
    /// Recovery already reported a missing `)` at a later synchronized position,
    /// so the immediate caller should suppress its fallback `parse_expected(')')`.
    pub(crate) suppress_next_missing_close_paren_error_once: bool,
}

impl ParserState {
    #[inline]
    #[must_use]
    pub(crate) fn u32_from_usize(&self, value: usize) -> u32 {
        let _ = self;
        u32::try_from(value).expect("parser offsets must fit in u32")
    }

    #[inline]
    #[must_use]
    pub(crate) fn u16_from_node_flags(&self, value: u32) -> u16 {
        let _ = self;
        u16::try_from(value).expect("parser node flags must fit in u16")
    }

    /// Create a new Parser for the given source text.
    #[must_use]
    pub fn new(file_name: String, source_text: String) -> Self {
        let estimated_nodes = source_text.len() / 20; // Rough estimate
        // Zero-copy: Pass source_text directly to scanner without cloning
        // This eliminates the 2x memory overhead from duplicating the source
        let scanner = ScannerState::new(source_text, true);
        Self {
            scanner,
            arena: NodeArena::with_capacity(estimated_nodes),
            file_name,
            context_flags: 0,
            current_token: SyntaxKind::Unknown,
            parse_diagnostics: Vec::new(),
            node_count: 0,
            recursion_depth: 0,
            last_error_pos: 0,
            label_scopes: vec![FxHashMap::default()],
            seen_module_indicator: false,
            last_named_imports_consumed_closing_brace: false,
            last_named_imports_recovered_to_from: false,
            last_named_imports_had_structural_error: false,
            deferred_module_close_braces: 0,
            abort_intersection_continuation: false,
            fallback_import_type_options_once: false,
            in_import_type_options_context: false,
            import_attribute_tail_recovered: false,
            suppress_object_literal_comma_once: false,
            suppress_next_missing_close_paren_error_once: false,
        }
    }

    pub fn reset(&mut self, file_name: String, source_text: String) {
        self.file_name = file_name;
        self.scanner.set_text(source_text, None, None);
        self.arena.clear();
        self.context_flags = 0;
        self.current_token = SyntaxKind::Unknown;
        self.parse_diagnostics.clear();
        self.node_count = 0;
        self.recursion_depth = 0;
        self.last_error_pos = 0;
        self.label_scopes.clear();
        self.label_scopes.push(FxHashMap::default());
        self.seen_module_indicator = false;
        self.last_named_imports_consumed_closing_brace = false;
        self.last_named_imports_recovered_to_from = false;
        self.last_named_imports_had_structural_error = false;
        self.deferred_module_close_braces = 0;
        self.abort_intersection_continuation = false;
        self.fallback_import_type_options_once = false;
        self.in_import_type_options_context = false;
        self.import_attribute_tail_recovered = false;
        self.suppress_object_literal_comma_once = false;
        self.suppress_next_missing_close_paren_error_once = false;
    }

    /// Check recursion limit - returns true if we can continue, false if limit exceeded
    pub(crate) fn enter_recursion(&mut self) -> bool {
        if self.recursion_depth >= MAX_PARSER_RECURSION_DEPTH {
            self.parse_error_at_current_token(
                "Maximum recursion depth exceeded",
                diagnostic_codes::UNEXPECTED_TOKEN,
            );
            false
        } else {
            self.recursion_depth += 1;
            true
        }
    }

    /// Centralized error suppression heuristic
    ///
    /// Prevents cascading errors by suppressing error reports if we've already
    /// emitted an error recently (within `ERROR_SUPPRESSION_DISTANCE` tokens).
    ///
    /// This standardizes the inconsistency where:
    /// - `parse_expected()` uses strict equality `!=`
    /// - `parse_semicolon()` uses `abs_diff > 3`
    ///
    /// Returns true if we should report an error, false if we should suppress it
    pub(crate) fn should_report_error(&self) -> bool {
        // Always report first error
        if self.last_error_pos == 0 {
            return true;
        }
        let current = self.token_pos();
        // Report if we've advanced past the suppression distance
        // This prevents multiple errors for the same position while still
        // catching genuine secondary errors
        current.abs_diff(self.last_error_pos) > ERROR_SUPPRESSION_DISTANCE
    }

    /// Check if the last emitted parse diagnostic was an unterminated literal error.
    /// These scanner-level errors (TS1002, TS1160, TS1161) consume tokens past
    /// closing delimiters, making subsequent "missing )" errors noise.
    fn last_error_was_unterminated_literal(&self) -> bool {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_diagnostics.last().is_some_and(|d| {
            matches!(
                d.code,
                diagnostic_codes::UNTERMINATED_STRING_LITERAL
                    | diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL
                    | diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL
                    | diagnostic_codes::UNEXPECTED_END_OF_TEXT
            )
        })
    }

    /// Exit recursion scope
    pub(crate) const fn exit_recursion(&mut self) {
        self.recursion_depth = self.recursion_depth.saturating_sub(1);
    }

    // =========================================================================
    // Token Utilities (shared with regular parser)
    // =========================================================================

    /// Check if we're in a JSX file.
    /// In tsc, .js/.cjs/.mjs/.jsx/.tsx all use LanguageVariant.JSX,
    /// only .ts/.cts/.mts use LanguageVariant.Standard.
    pub(crate) fn is_jsx_file(&self) -> bool {
        std::path::Path::new(&self.file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                ext.eq_ignore_ascii_case("tsx")
                    || ext.eq_ignore_ascii_case("jsx")
                    || ext.eq_ignore_ascii_case("js")
                    || ext.eq_ignore_ascii_case("cjs")
                    || ext.eq_ignore_ascii_case("mjs")
            })
    }

    /// Check if we're in a JavaScript file (not TypeScript).
    pub(crate) fn is_js_file(&self) -> bool {
        std::path::Path::new(&self.file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                ext.eq_ignore_ascii_case("js")
                    || ext.eq_ignore_ascii_case("cjs")
                    || ext.eq_ignore_ascii_case("mjs")
                    || ext.eq_ignore_ascii_case("jsx")
            })
    }

    /// Check if we're in a declaration file (.d.ts/.d.mts/.d.cts, or .d.<ext>.ts).
    pub(crate) fn is_declaration_file(&self) -> bool {
        let file_name = self.file_name.to_ascii_lowercase();
        if file_name.ends_with(".d.ts")
            || file_name.ends_with(".d.mts")
            || file_name.ends_with(".d.cts")
        {
            return true;
        }
        // Arbitrary extension declaration files: .d.<ext>.ts (e.g. .d.html.ts)
        if file_name.ends_with(".ts") {
            let basename = file_name.rsplit('/').next().unwrap_or(&file_name);
            let basename = basename.rsplit('\\').next().unwrap_or(basename);
            return basename.contains(".d.");
        }
        false
    }

    /// Get current token
    #[inline]
    pub(crate) const fn token(&self) -> SyntaxKind {
        self.current_token
    }

    /// Get current token position
    #[inline]
    pub(crate) fn token_pos(&self) -> u32 {
        self.u32_from_usize(self.scanner.get_token_start())
    }

    /// Get full start position of current token (including leading trivia).
    ///
    /// Unlike `token_pos()` which returns the start of the token text itself,
    /// this returns the position where leading trivia (whitespace, comments)
    /// begins. Matches TSC's `scanner.getTokenFullStart()`.
    #[inline]
    pub(crate) fn token_full_start(&self) -> u32 {
        self.u32_from_usize(self.scanner.get_token_full_start())
    }

    /// Get current token end position
    #[inline]
    pub(crate) fn token_end(&self) -> u32 {
        self.u32_from_usize(self.scanner.get_token_end())
    }

    /// Advance to next token
    pub(crate) fn next_token(&mut self) -> SyntaxKind {
        self.current_token = self.scanner.scan();
        self.current_token
    }

    /// Consume a keyword token, checking for TS1260 (keywords cannot contain escape characters).
    /// Call this instead of `next_token()` when consuming a keyword in a keyword position.
    pub(crate) fn consume_keyword(&mut self) {
        self.check_keyword_with_escape();
        self.next_token();
    }

    /// Check if current token is a keyword with unicode escape and emit TS1260 if so.
    /// Only call this when consuming a token that is expected to be a keyword.
    fn check_keyword_with_escape(&mut self) {
        // Skip if not a keyword
        if !token_is_keyword(self.current_token) {
            return;
        }
        // Check for UnicodeEscape flag
        let flags = self.scanner.get_token_flags();
        if (flags & TokenFlags::UnicodeEscape as u32) != 0 {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                self.u32_from_usize(self.scanner.get_token_start()),
                self.u32_from_usize(self.scanner.get_token_end() - self.scanner.get_token_start()),
                "Keywords cannot contain escape characters.",
                diagnostic_codes::KEYWORDS_CANNOT_CONTAIN_ESCAPE_CHARACTERS,
            );
        }
    }

    /// Check if current token matches kind
    #[inline]
    pub(crate) fn is_token(&self, kind: SyntaxKind) -> bool {
        self.current_token == kind
    }

    /// Check if current token is an identifier or any keyword
    /// Keywords can be used as identifiers in many contexts (e.g., class names, property names)
    #[inline]
    pub(crate) const fn is_identifier_or_keyword(&self) -> bool {
        self.current_token as u16 >= SyntaxKind::Identifier as u16
    }

    /// Check if current token is an identifier (excluding reserved words).
    /// Matches tsc's `isIdentifier()`: returns true for plain identifiers and
    /// contextual/future-reserved keywords, but false for reserved words like
    /// `import`, `export`, `class`, `function`, etc.
    #[inline]
    pub(crate) const fn is_identifier(&self) -> bool {
        self.current_token as u16 == SyntaxKind::Identifier as u16
            || (self.current_token as u16 > SyntaxKind::WithKeyword as u16
                && self.current_token as u16 <= SyntaxKind::DeferKeyword as u16)
    }

    /// Check if current token is a future reserved word (strict mode reserved).
    /// These are: implements, interface, let, package, private, protected, public, static, yield.
    /// In strict mode contexts these cannot be used as identifiers.
    #[inline]
    #[allow(dead_code)]
    pub(crate) const fn is_future_reserved_word(&self) -> bool {
        self.current_token as u16 >= SyntaxKind::FIRST_FUTURE_RESERVED_WORD as u16
            && self.current_token as u16 <= SyntaxKind::LAST_FUTURE_RESERVED_WORD as u16
    }

    /// Check if we're in a strict mode context (class body or module).
    /// TypeScript class bodies and modules are always in strict mode.
    #[inline]
    #[allow(dead_code)]
    pub(crate) const fn in_strict_mode_context(&self) -> bool {
        self.in_class_body() || self.seen_module_indicator
    }

    /// Check if current token can start a type member declaration
    #[inline]
    pub(crate) const fn is_type_member_start(&self) -> bool {
        match self.current_token {
            SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken | SyntaxKind::NewKeyword => true,
            _ => self.is_property_name(),
        }
    }

    /// Check if current token can be a property name
    /// Includes identifiers, keywords (as property names), string/numeric literals, computed properties
    #[inline]
    pub(crate) const fn is_property_name(&self) -> bool {
        match self.current_token {
            SyntaxKind::Identifier
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::PrivateIdentifier
            | SyntaxKind::OpenBracketToken // computed property name
            | SyntaxKind::GetKeyword
            | SyntaxKind::SetKeyword => true,
            // Any keyword can be used as a property name
            _ => self.is_identifier_or_keyword()
        }
    }

    /// Check if the next token (after the current one) is `{` on the same line.
    /// Used to disambiguate `class implements {` (class named "implements") from
    /// `class implements SomeType {` (class with implements clause).
    pub(crate) fn next_token_is_open_brace(&mut self) -> bool {
        let saved_token = self.current_token;
        let saved_state = self.scanner.save_state();
        self.next_token();
        let result = !self.scanner.has_preceding_line_break()
            && self.current_token == SyntaxKind::OpenBraceToken;
        self.scanner.restore_state(saved_state);
        self.current_token = saved_token;
        result
    }

    /// Check if the next token (after the current one) is `[` on the same line.
    pub(crate) fn next_token_is_open_bracket(&mut self) -> bool {
        let saved_token = self.current_token;
        let saved_state = self.scanner.save_state();
        self.next_token();
        let result = !self.scanner.has_preceding_line_break()
            && self.current_token == SyntaxKind::OpenBracketToken;
        self.scanner.restore_state(saved_state);
        self.current_token = saved_token;
        result
    }

    /// Used to emit TS1110 (Type expected) instead of TS1005 (identifier expected)
    /// when a type is expected but we encounter a token that can't start a type
    #[inline]
    pub(crate) const fn can_token_start_type(&self) -> bool {
        match self.current_token {
            // Tokens that definitely cannot start a type
            SyntaxKind::CloseParenToken       // )
            | SyntaxKind::CloseBraceToken     // }
            | SyntaxKind::CloseBracketToken   // ]
            | SyntaxKind::CommaToken          // ,
            | SyntaxKind::SemicolonToken      // ;
            | SyntaxKind::ColonToken          // :
            | SyntaxKind::EqualsToken         // =
            | SyntaxKind::EqualsGreaterThanToken  // =>
            | SyntaxKind::GreaterThanToken    // > (e.g., missing type in generic default: T = >)
            | SyntaxKind::BarToken            // | (when at start, not a union)
            | SyntaxKind::AmpersandToken      // & (when at start, not an intersection)
            // Note: QuestionToken is NOT listed here — it is handled by parse_primary_type
            // which emits TS17020 for JSDoc-style leading `?` (e.g., `?string`).
            | SyntaxKind::EndOfFileToken => false,
            // Everything else could potentially start a type
            // (identifiers, keywords, literals, type operators, etc.)
            _ => true
        }
    }

    /// Check if the current token is a delimiter/terminator where a missing type
    /// should be silently recovered (no TS1110). TSC doesn't emit "Type expected" when
    /// a type is simply omitted before a structural delimiter like `)`, `,`, `=>`, etc.
    pub(crate) const fn is_type_terminator_token(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::CloseParenToken          // ) - end of parameter list, parenthesized type
            | SyntaxKind::CloseBracketToken      // ] - end of tuple/array type
            | SyntaxKind::CloseBraceToken        // } - end of object type / block
            | SyntaxKind::CommaToken             // , - next element in list
            | SyntaxKind::SemicolonToken         // ; - end of statement
            | SyntaxKind::EqualsGreaterThanToken // => - arrow (return type missing)
            | SyntaxKind::EndOfFileToken // EOF
        )
    }

    /// Parse type member separators with ASI-aware recovery.
    ///
    /// Type members in interface/type literal bodies allow:
    /// - Explicit `;` or `,`
    /// - ASI-separated members when a line break exists
    ///
    /// When members are missing a separator on the same line, emit
    /// `';' expected.` (TS1005) and continue parsing.
    pub(crate) fn parse_type_member_separator_with_asi(&mut self) {
        if self.parse_optional(SyntaxKind::SemicolonToken)
            || self.parse_optional(SyntaxKind::CommaToken)
        {
            return;
        }

        // No explicit separator and not at a boundary that permits implicit recovery.
        if self.scanner.has_preceding_line_break() || self.is_token(SyntaxKind::CloseBraceToken) {
            return;
        }

        if self.is_type_member_start() {
            self.error_token_expected(";");
        }
    }

    /// Check if we're inside an async function/method/arrow
    #[inline]
    pub(crate) const fn in_async_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_ASYNC) != 0
    }

    /// Check if we're inside a generator function/method
    #[inline]
    pub(crate) const fn in_generator_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_GENERATOR) != 0
    }

    /// Check if we're parsing a class member name.
    #[inline]
    pub(crate) const fn in_class_member_name(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_CLASS_MEMBER_NAME) != 0
    }

    /// Check if we're parsing inside a class body.
    #[inline]
    pub(crate) const fn in_class_body(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_IN_CLASS) != 0
    }

    #[inline]
    pub(crate) const fn in_module_context(&self) -> bool {
        self.seen_module_indicator
    }

    pub(crate) fn report_yield_reserved_word_error(&mut self) {
        self.report_strict_mode_reserved_word_error("yield");
    }

    /// Report TS1212/TS1213/TS1214 for a future reserved word used as an identifier
    /// in strict mode. Uses context-specific messages matching tsc.
    pub(crate) fn report_strict_mode_reserved_word_error(&mut self, word: &str) {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.in_class_body() || self.in_class_member_name() {
            let msg = format!(
                "Identifier expected. '{word}' is a reserved word in strict mode. Class definitions are automatically in strict mode."
            );
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            );
        } else if self.in_module_context() {
            let msg = format!(
                "Identifier expected. '{word}' is a reserved word in strict mode. Modules are automatically in strict mode."
            );
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_MODULES_ARE_AUTOMATICALLY,
            );
        } else {
            let msg = format!("Identifier expected. '{word}' is a reserved word in strict mode.");
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
            );
        }
    }

    /// Check if we're inside a static block
    #[inline]
    pub(crate) const fn in_static_block_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_STATIC_BLOCK) != 0
    }

    /// Check if we're parsing a parameter default (where 'await' is not allowed)
    #[inline]
    pub(crate) const fn in_parameter_default_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_PARAMETER_DEFAULT) != 0
    }

    /// Check if 'in' is disallowed as a binary operator (e.g., in for-statement initializers)
    #[inline]
    pub(crate) const fn in_disallow_in_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_DISALLOW_IN) != 0
    }

    /// Check if we're inside a block statement (function body, bare block, etc.)
    /// where modifiers like `export`/`declare` are not allowed.
    #[inline]
    pub(crate) const fn in_block_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_IN_BLOCK) != 0
    }

    /// Check if we're currently parsing inside a parenthesized expression.
    #[inline]
    pub(crate) const fn in_parenthesized_expression_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_IN_PARENTHESIZED_EXPRESSION) != 0
    }

    /// Check if the current token is an illegal binding identifier in the current context
    /// Returns true if illegal and emits appropriate diagnostic
    pub(crate) fn check_illegal_binding_identifier(&mut self) -> bool {
        use tsz_common::diagnostics::diagnostic_codes;

        // Check if current token is 'await' (either as keyword or identifier)
        let is_await = self.is_token(SyntaxKind::AwaitKeyword)
            || (self.is_token(SyntaxKind::Identifier)
                && self.scanner.get_token_value_ref() == "await");

        // Class members reject modifier-like keywords as computed property names.
        // This emits TS1213 in class member context while leaving object/literal contexts unchanged.
        if self.in_class_member_name()
            && matches!(
                self.token(),
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::ReadonlyKeyword
                    | SyntaxKind::StaticKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::OverrideKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::AwaitKeyword
                    | SyntaxKind::YieldKeyword
            )
        {
            let token_text = self.scanner.get_token_value_ref();
            self.parse_error_at_current_token(
                &format!(
                    "Identifier expected. '{token_text}' is a reserved word in strict mode. Class definitions are automatically in strict mode."
                ),
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            );
            return true;
        }

        if is_await {
            // In async contexts, 'await' cannot be used as a binding identifier.
            // In static blocks, 'await' is always reserved — TSC treats class static
            // blocks as having an implicit async-like context regardless of module mode.
            if self.in_async_context() || self.in_static_block_context() {
                self.parse_error_at_current_token(
                    "Identifier expected. 'await' is a reserved word that cannot be used here.",
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                );
                return true;
            }
        }

        // Check if current token is 'yield' (either as keyword or identifier).
        // `yield` is only a reserved identifier in strict mode.  In non-strict
        // generator functions, `yield` is a keyword for yield-expressions but
        // may still appear as a binding identifier (tsc does not emit TS1212
        // for e.g. `function* f(yield){}` in non-strict, non-module code).
        let is_yield = self.is_token(SyntaxKind::YieldKeyword)
            || (self.is_token(SyntaxKind::Identifier)
                && self.scanner.get_token_value_ref() == "yield");

        if is_yield && self.in_generator_context() && self.in_strict_mode_context() {
            self.report_yield_reserved_word_error();
            return true;
        }

        false
    }

    /// Recover from invalid method/member syntax when `(` is missing after the member name.
    /// This is used for async/generator forms like `async * get x() {}` where a single TS1005
    /// should be emitted and the parser should skip the rest of the member to avoid cascades.
    pub(crate) fn recover_from_missing_method_open_paren(&mut self) {
        while !(self.is_token(SyntaxKind::OpenBraceToken)
            || self.is_token(SyntaxKind::SemicolonToken)
            || self.is_token(SyntaxKind::CommaToken)
            || self.is_token(SyntaxKind::CloseBraceToken))
        {
            self.next_token();
        }

        if self.is_token(SyntaxKind::OpenBraceToken) {
            let body = self.parse_block();
            let _ = body;
            return;
        }

        if self.is_token(SyntaxKind::SemicolonToken) || self.is_token(SyntaxKind::CommaToken) {
            self.next_token();
        }
    }

    /// Parse optional token, returns true if found
    pub fn parse_optional(&mut self, kind: SyntaxKind) -> bool {
        if self.is_token(kind) {
            // Check for TS1260 if consuming a keyword
            if token_is_keyword(kind) {
                self.check_keyword_with_escape();
            }
            self.next_token();
            true
        } else {
            false
        }
    }

    /// Parse expected token, report error if not found
    /// Suppresses error if we already emitted an error at the current position
    /// (to prevent cascading errors from sequential `parse_expected` calls)
    pub fn parse_expected(&mut self, kind: SyntaxKind) -> bool {
        if kind == SyntaxKind::CloseParenToken && self.suppress_next_missing_close_paren_error_once
        {
            self.suppress_next_missing_close_paren_error_once = false;
            if !self.is_token(SyntaxKind::CloseParenToken) {
                return false;
            }
        }

        if self.is_token(kind) {
            // Check for TS1260 if consuming a keyword
            if token_is_keyword(kind) {
                self.check_keyword_with_escape();
            }
            self.next_token();
            true
        } else if self.is_token(SyntaxKind::Unknown) {
            // Unknown token = invalid character. In tsc, the scanner emits TS1127 via
            // scanError callback and advances past the invalid char during scanning.
            // The parser then sees the next real token. We replicate this by emitting
            // TS1127, advancing, and re-checking for the expected token.
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    diagnostic_codes::INVALID_CHARACTER,
                );
            }
            self.next_token();
            // After skipping the invalid character, check if expected token is now present
            if self.is_token(kind) {
                self.next_token();
                return true;
            }
            // Expected token still not found — emit TS1005 at the new position.
            // In tsc, parseErrorAtPosition dedup is same-position only (not distance-based),
            // so TS1005 at the post-Unknown position always emits. Use direct error emit
            // to bypass our distance-based should_report_error() suppression.
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    &format!("'{}' expected.", Self::token_to_string(kind)),
                    diagnostic_codes::EXPECTED,
                );
            }
            false
        } else {
            // Force error emission for missing ) in common patterns.
            // This bypasses the should_report_error() distance check.
            // tsc's parseExpected always emits TS1005 at the current position
            // unless an error was already reported at the exact same position.
            // At EOF, force-emit unless the last error was an unterminated literal
            // (TS1002/TS1160/TS1161) — these scanner errors consume tokens past
            // the `)` and the missing `)` is a cascading artifact.
            let force_emit = (kind == SyntaxKind::CloseParenToken
                && (self.is_token(SyntaxKind::OpenBraceToken)
                    || self.is_token(SyntaxKind::CloseBraceToken)
                    || ((self.is_identifier_or_keyword()
                        || self.is_token(SyntaxKind::ThisKeyword))
                        && self.last_error_pos != 0
                        && self.token_pos().abs_diff(self.last_error_pos) <= 3)
                    || (self.is_token(SyntaxKind::EndOfFileToken)
                        && !self.last_error_was_unterminated_literal())))
                || (kind == SyntaxKind::CloseBraceToken
                    && self.is_token(SyntaxKind::EndOfFileToken)
                    && !self.last_error_was_unterminated_literal()
                    && self.last_error_pos != self.token_pos());

            // Only emit error if we haven't already emitted one at this position
            // This prevents cascading errors like "';' expected" followed by "')' expected"
            // when the real issue is a single missing token
            // Use centralized error suppression heuristic
            if force_emit || self.should_report_error() {
                // Additional check: suppress error for missing closing tokens when we're
                // at a clear statement boundary or EOF (reduces false-positive TS1005 errors)
                let should_suppress = if force_emit {
                    false // Never suppress forced errors
                } else {
                    match kind {
                        SyntaxKind::CloseBraceToken | SyntaxKind::CloseBracketToken => {
                            // At EOF, the file ended before this closing token. TypeScript reports
                            // these missing closing delimiters, so do not suppress at EOF.
                            if self.is_token(SyntaxKind::EndOfFileToken) {
                                false
                            }
                            // If next token starts a statement, the user has clearly moved on
                            // Don't complain about missing closing token
                            else if self.is_statement_start() {
                                true
                            }
                            // If there's a line break, give the user benefit of doubt
                            else {
                                self.scanner.has_preceding_line_break()
                            }
                        }
                        SyntaxKind::CloseParenToken => {
                            // Missing ) is almost always a genuine error — don't suppress
                            // at EOF, statement boundaries, or block delimiters.
                            // Only suppress if on same line with no clear boundary.
                            if self.is_token(SyntaxKind::EndOfFileToken) {
                                // At EOF, suppress when an unterminated literal consumed
                                // past the `)`. Also suppress when a prior error is within
                                // suppression distance (cascading error).
                                self.last_error_was_unterminated_literal()
                                    || (self.last_error_pos != 0
                                        && self.token_pos().abs_diff(self.last_error_pos)
                                            <= ERROR_SUPPRESSION_DISTANCE)
                            } else if self.scanner.has_preceding_line_break() {
                                // At a line break, suppress unless it's a clear boundary
                                !self.is_statement_start()
                                    && !self.is_token(SyntaxKind::CloseBraceToken)
                            } else {
                                false
                            }
                        }
                        _ => false,
                    }
                };

                if !should_suppress {
                    // For forced errors, bypass the normal error budget logic
                    if force_emit {
                        use tsz_common::diagnostics::diagnostic_codes;
                        self.parse_error_at_current_token(
                            &format!("'{}' expected.", Self::token_to_string(kind)),
                            diagnostic_codes::EXPECTED,
                        );
                    } else {
                        self.error_token_expected(Self::token_to_string(kind));
                    }
                }
            }
            false
        }
    }

    /// Convert `SyntaxKind` to human-readable token string
    pub(crate) const fn token_to_string(kind: SyntaxKind) -> &'static str {
        match kind {
            SyntaxKind::OpenBraceToken => "{",
            SyntaxKind::CloseBraceToken => "}",
            SyntaxKind::OpenParenToken => "(",
            SyntaxKind::CloseParenToken => ")",
            SyntaxKind::OpenBracketToken => "[",
            SyntaxKind::CloseBracketToken => "]",
            SyntaxKind::SemicolonToken => ";",
            SyntaxKind::CommaToken => ",",
            SyntaxKind::ColonToken => ":",
            SyntaxKind::DotToken => ".",
            SyntaxKind::EqualsToken => "=",
            SyntaxKind::GreaterThanToken => ">",
            SyntaxKind::LessThanToken => "<",
            SyntaxKind::LessThanSlashToken => "</",
            SyntaxKind::QuestionToken => "?",
            SyntaxKind::ExclamationToken => "!",
            SyntaxKind::AtToken => "@",
            SyntaxKind::AmpersandToken => "&",
            SyntaxKind::BarToken => "|",
            SyntaxKind::PlusToken => "+",
            SyntaxKind::MinusToken => "-",
            SyntaxKind::AsteriskToken => "*",
            SyntaxKind::SlashToken => "/",
            SyntaxKind::EqualsGreaterThanToken => "=>",
            SyntaxKind::DotDotDotToken => "...",
            SyntaxKind::Identifier => "identifier",
            SyntaxKind::TryKeyword => "try",
            SyntaxKind::FromKeyword => "from",
            SyntaxKind::AsKeyword => "as",
            SyntaxKind::OfKeyword => "of",
            _ => "token",
        }
    }

    pub(crate) fn parse_error_at(&mut self, start: u32, length: u32, message: &str, code: u32) {
        // Don't report another error if it would just be at the same position as the last error.
        // This matches tsc's parseErrorAtPosition deduplication behavior where parser errors
        // at the same position are suppressed (only the first one survives).
        if let Some(last) = self.parse_diagnostics.last()
            && last.start == start
        {
            return;
        }
        // Track the position of this error to prevent cascading errors at same position
        self.last_error_pos = start;
        self.parse_diagnostics.push(ParseDiagnostic {
            start,
            length,
            message: message.to_string(),
            code,
        });
    }

    /// Report parse error at current token with specific error code
    pub fn parse_error_at_current_token(&mut self, message: &str, code: u32) {
        let start = self.u32_from_usize(self.scanner.get_token_start());
        let end = self.u32_from_usize(self.scanner.get_token_end());
        self.parse_error_at(start, end - start, message, code);
    }

    /// Emit a companion diagnostic at the current token, bypassing position-based
    /// deduplication.  Use when TSC emits multiple distinct error codes at the same
    /// position (e.g. TS1042 + TS1184 for object-literal modifiers).
    pub(crate) fn parse_companion_error_at_current_token(&mut self, message: &str, code: u32) {
        let start = self.u32_from_usize(self.scanner.get_token_start());
        let end = self.u32_from_usize(self.scanner.get_token_end());
        let length = end - start;
        self.parse_diagnostics.push(ParseDiagnostic {
            start,
            length,
            message: message.to_string(),
            code,
        });
    }
    pub(crate) fn report_invalid_string_or_template_escape_errors(&mut self) {
        let token_text = self.scanner.get_token_text_ref().to_string();
        if token_text.is_empty()
            || (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidEscape as u32) == 0
        {
            return;
        }

        let bytes = token_text.as_bytes();
        let token_len = bytes.len();
        let token_start = self.token_pos() as usize;

        let Some((content_start_offset, content_end_offset)) =
            self.string_template_escape_content_span(token_len, bytes)
        else {
            return;
        };

        if content_end_offset <= content_start_offset || content_end_offset > token_len {
            return;
        }

        let raw = &bytes[content_start_offset..content_end_offset];
        let content_start = token_start + content_start_offset;

        let mut i = 0usize;

        while i < raw.len() {
            if raw[i] != b'\\' {
                i += 1;
                continue;
            }
            if i + 1 >= raw.len() {
                break;
            }
            i = match raw[i + 1] {
                b'x' => self.report_invalid_string_or_template_hex_escape(raw, content_start, i),
                b'u' => {
                    self.report_invalid_string_or_template_unicode_escape(raw, content_start, i)
                }
                _ => i + 1,
            };
        }
    }

    fn string_template_escape_content_span(
        &self,
        token_len: usize,
        bytes: &[u8],
    ) -> Option<(usize, usize)> {
        match self.current_token {
            SyntaxKind::StringLiteral => {
                if token_len < 2
                    || (bytes[0] != b'"' && bytes[0] != b'\'')
                    || bytes[token_len - 1] != bytes[0]
                {
                    return None;
                }
                Some((1, token_len - 1))
            }
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                if bytes[0] != b'`' || bytes[token_len - 1] != b'`' {
                    return None;
                }
                Some((1, token_len - 1))
            }
            SyntaxKind::TemplateHead => {
                if bytes[0] != b'`' || !bytes.ends_with(b"${") {
                    return None;
                }
                Some((1, token_len - 2))
            }
            SyntaxKind::TemplateMiddle | SyntaxKind::TemplateTail => {
                if bytes[0] != b'}' {
                    return None;
                }
                if bytes.ends_with(b"${") {
                    Some((1, token_len - 2))
                } else if bytes.ends_with(b"`") {
                    Some((1, token_len - 1))
                } else {
                    Some((1, token_len))
                }
            }
            _ => None,
        }
    }

    fn report_invalid_string_or_template_hex_escape(
        &mut self,
        raw: &[u8],
        content_start: usize,
        i: usize,
    ) -> usize {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let first = i + 2;
        let second = i + 3;
        let err_len = |offset: usize| u32::from(offset < raw.len());

        if first >= raw.len() || !Self::is_hex_digit(raw[first]) {
            self.parse_error_at(
                self.u32_from_usize(content_start + first),
                err_len(first),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
        } else if second >= raw.len() || !Self::is_hex_digit(raw[second]) {
            self.parse_error_at(
                self.u32_from_usize(content_start + second),
                err_len(second),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
        }
        i + 2
    }

    fn report_invalid_string_or_template_unicode_escape(
        &mut self,
        raw: &[u8],
        content_start: usize,
        i: usize,
    ) -> usize {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if i + 2 >= raw.len() {
            self.parse_error_at(
                self.u32_from_usize(content_start + i + 2),
                u32::from(i + 2 < raw.len()),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
            return i + 2;
        }

        if raw[i + 2] == b'{' {
            let mut close = i + 3;
            let mut has_digit = false;
            while close < raw.len() && Self::is_hex_digit(raw[close]) {
                has_digit = true;
                close += 1;
            }
            if close >= raw.len() {
                if !has_digit {
                    // No hex digits at all: \u{ followed by end → TS1125
                    self.parse_error_at(
                        self.u32_from_usize(content_start + i + 3),
                        0,
                        diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                        diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                    );
                } else {
                    // Had hex digits but no closing brace → TS1508
                    self.parse_error_at(
                        self.u32_from_usize(content_start + close),
                        0,
                        diagnostic_messages::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                        diagnostic_codes::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                    );
                }
            } else if raw[close] == b'}' {
                if !has_digit {
                    self.parse_error_at(
                        self.u32_from_usize(content_start + close),
                        1,
                        diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                        diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                    );
                } else {
                    // Check if the value exceeds 0x10FFFF (TS1198)
                    let hex_str = std::str::from_utf8(&raw[i + 3..close]).unwrap_or("");
                    if let Ok(value) = u32::from_str_radix(hex_str, 16)
                        && value > 0x10FFFF
                    {
                        self.parse_error_at(
                            self.u32_from_usize(content_start + i + 3),
                            (close - i - 3) as u32,
                            diagnostic_messages::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE,
                            diagnostic_codes::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE,
                        );
                    }
                }
            } else if !has_digit {
                self.parse_error_at(
                    self.u32_from_usize(content_start + i + 3),
                    1,
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else {
                self.parse_error_at(
                    self.u32_from_usize(content_start + close),
                    1,
                    diagnostic_messages::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                    diagnostic_codes::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                );
            }
            close + 1
        } else {
            let first = i + 2;
            let second = i + 3;
            let third = i + 4;
            let fourth = i + 5;
            let err_len = |offset: usize| u32::from(offset < raw.len());

            if first >= raw.len() || !Self::is_hex_digit(raw[first]) {
                self.parse_error_at(
                    self.u32_from_usize(content_start + first),
                    err_len(first),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if second >= raw.len() || !Self::is_hex_digit(raw[second]) {
                self.parse_error_at(
                    self.u32_from_usize(content_start + second),
                    err_len(second),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if third >= raw.len() || !Self::is_hex_digit(raw[third]) {
                self.parse_error_at(
                    self.u32_from_usize(content_start + third),
                    err_len(third),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if fourth >= raw.len() || !Self::is_hex_digit(raw[fourth]) {
                self.parse_error_at(
                    self.u32_from_usize(content_start + fourth),
                    err_len(fourth),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            }
            i + 2
        }
    }

    #[inline]
    const fn is_hex_digit(byte: u8) -> bool {
        byte.is_ascii_hexdigit()
    }

    /// Parse regex unicode escape diagnostics for regex literals in /u or /v mode.
    /// NOTE: Currently unused - tsc does not emit TS1125 for invalid unicode escapes
    /// inside regex literals. Kept for potential future use if tsc behavior changes.
    #[allow(dead_code)]
    pub(crate) fn report_invalid_regular_expression_escape_errors(&mut self) {
        let token_text = self.scanner.get_token_text_ref().to_string();
        if !token_text.starts_with('/') || token_text.len() < 2 {
            return;
        }

        let bytes = token_text.as_bytes();
        let mut i = 1usize;
        let mut in_escape = false;
        let mut in_character_class = false;
        while i < bytes.len() {
            let ch = bytes[i];
            if in_escape {
                in_escape = false;
                i += 1;
                continue;
            }
            if ch == b'\\' {
                in_escape = true;
                i += 1;
                continue;
            }
            if ch == b'[' {
                in_character_class = true;
                i += 1;
                continue;
            }
            if ch == b']' {
                in_character_class = false;
                i += 1;
                continue;
            }
            if ch == b'/' && !in_character_class {
                break;
            }
            i += 1;
        }
        if i >= bytes.len() {
            return;
        }

        let body = &token_text[1..i];
        let flags = if i + 1 < token_text.len() {
            &token_text[i + 1..]
        } else {
            ""
        };
        let has_unicode_flag = flags.as_bytes().iter().any(|&b| b == b'u' || b == b'v');
        if !has_unicode_flag {
            return;
        }

        let body_start = self.token_pos() as usize + 1;
        let raw = body.as_bytes();
        let mut j = 0usize;

        while j < raw.len() {
            if raw[j] != b'\\' {
                j += 1;
                continue;
            }
            if j + 1 >= raw.len() {
                break;
            }
            match raw[j + 1] {
                b'x' => {
                    j = self.report_invalid_regular_expression_hex_escape(raw, body_start, j);
                }
                b'u' => {
                    if let Some(next) =
                        self.report_invalid_regular_expression_unicode_escape(raw, body_start, j)
                    {
                        j = next;
                    } else {
                        break;
                    }
                }
                _ => {
                    j += 1;
                }
            }
        }
    }

    fn report_invalid_regular_expression_hex_escape(
        &mut self,
        raw: &[u8],
        body_start: usize,
        j: usize,
    ) -> usize {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let first = j + 2;
        let second = j + 3;
        if first >= raw.len() || !Self::is_hex_digit(raw[first]) {
            self.parse_error_at(
                self.u32_from_usize(body_start + first),
                u32::from(first < raw.len()),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
        } else if second >= raw.len() || !Self::is_hex_digit(raw[second]) {
            self.parse_error_at(
                self.u32_from_usize(body_start + second),
                u32::from(second < raw.len()),
                diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
            );
        }
        j + 2
    }

    fn report_invalid_regular_expression_unicode_escape(
        &mut self,
        raw: &[u8],
        body_start: usize,
        j: usize,
    ) -> Option<usize> {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if j + 2 < raw.len() && raw[j + 2] == b'{' {
            let mut close = j + 3;
            let mut has_digit = false;
            while close < raw.len() && Self::is_hex_digit(raw[close]) {
                has_digit = true;
                close += 1;
            }
            if close >= raw.len() {
                self.parse_error_at(
                    self.u32_from_usize(body_start + close),
                    0,
                    diagnostic_messages::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                    diagnostic_codes::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                );
                return None;
            }
            if raw[close] == b'}' {
                if !has_digit {
                    self.parse_error_at(
                        self.u32_from_usize(body_start + close),
                        1,
                        diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                        diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                    );
                }
            } else if !has_digit {
                self.parse_error_at(
                    self.u32_from_usize(body_start + j + 3),
                    1,
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else {
                self.parse_error_at(
                    self.u32_from_usize(body_start + close),
                    1,
                    diagnostic_messages::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                    diagnostic_codes::UNTERMINATED_UNICODE_ESCAPE_SEQUENCE,
                );
                self.parse_error_at(
                    self.u32_from_usize(body_start + close),
                    1,
                    diagnostic_messages::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                    diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH,
                );
            }
            Some(close + 1)
        } else {
            let first = j + 2;
            let second = j + 3;
            let third = j + 4;
            let fourth = j + 5;
            if first >= raw.len() || !Self::is_hex_digit(raw[first]) {
                self.parse_error_at(
                    self.u32_from_usize(body_start + first),
                    u32::from(first < raw.len()),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if second >= raw.len() || !Self::is_hex_digit(raw[second]) {
                self.parse_error_at(
                    self.u32_from_usize(body_start + second),
                    u32::from(second < raw.len()),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if third >= raw.len() || !Self::is_hex_digit(raw[third]) {
                self.parse_error_at(
                    self.u32_from_usize(body_start + third),
                    u32::from(third < raw.len()),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            } else if fourth >= raw.len() || !Self::is_hex_digit(raw[fourth]) {
                self.parse_error_at(
                    self.u32_from_usize(body_start + fourth),
                    u32::from(fourth < raw.len()),
                    diagnostic_messages::HEXADECIMAL_DIGIT_EXPECTED,
                    diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED,
                );
            }
            Some(j + 2)
        }
    }

    // =========================================================================
    // Typed error helper methods (use these instead of parse_error_at_current_token)
    // =========================================================================

    /// Error: Expression expected (TS1109)
    pub(crate) fn error_expression_expected(&mut self) {
        if !self.is_js_file()
            && self.is_token(SyntaxKind::GreaterThanToken)
            && self
                .get_source_text()
                .get(self.token_pos().saturating_sub(1) as usize..self.token_pos() as usize)
                == Some("<")
        {
            return;
        }
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading TS1109 errors when TS1005 or other errors already reported
        // Use centralized error suppression heuristic
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
        }
    }

    /// Error: Expression or comma expected (TS1137)
    /// Used in array literal element parsing where tsc uses TS1137 instead of TS1109.
    pub(crate) fn error_expression_or_comma_expected(&mut self) {
        if self.should_report_error() {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::EXPRESSION_OR_COMMA_EXPECTED,
                diagnostic_codes::EXPRESSION_OR_COMMA_EXPECTED,
            );
        }
    }

    /// Error: Argument expression expected (TS1135)
    /// Used in function call argument list parsing instead of generic TS1109.
    pub(crate) fn error_argument_expression_expected(&mut self) {
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Argument expression expected.",
                diagnostic_codes::ARGUMENT_EXPRESSION_EXPECTED,
            );
        }
    }

    /// Error: Array element destructuring pattern expected (TS1181)
    /// Used in array binding-pattern-like contexts when an element-like token is invalid.
    pub(crate) fn error_array_element_destructuring_pattern_expected(&mut self) {
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Array element destructuring pattern expected.",
                diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED,
            );
        }
    }

    /// Error: Type expected (TS1110)
    pub(crate) fn error_type_expected(&mut self) {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token("Type expected.", diagnostic_codes::TYPE_EXPECTED);
    }

    /// Error: Identifier expected (TS1003), or Invalid character (TS1127)
    pub(crate) fn error_identifier_expected(&mut self) {
        // When the current token is Unknown (invalid character), emit TS1127
        // instead of TS1003, matching tsc's behavior where the scanner's
        // TS1127 shadows the parser's TS1003 via position-based dedup.
        if self.is_token(SyntaxKind::Unknown) {
            if self.should_report_error() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    diagnostic_codes::INVALID_CHARACTER,
                );
            }
            return;
        }
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading errors when a missing token causes identifier to be expected
        // Use centralized error suppression heuristic
        if self.should_report_error() {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            // tsc uses TS1359 for reserved words ("'X' is a reserved word that cannot
            // be used here") and TS1003 for other non-identifier tokens.
            if self.is_reserved_word() {
                let word = self.current_keyword_text();
                let msg = diagnostic_messages::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE
                    .replace("{0}", word);
                self.parse_error_at_current_token(
                    &msg,
                    diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
                );
            } else {
                self.parse_error_at_current_token(
                    "Identifier expected.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
        }
    }

    /// Check if current token is a reserved word that cannot be used as an identifier
    /// Reserved words are keywords from `BreakKeyword` through `WithKeyword`
    #[inline]
    pub(crate) const fn is_reserved_word(&self) -> bool {
        // Match TypeScript's isReservedWord logic:
        // token >= SyntaxKind.FirstReservedWord && token <= SyntaxKind.LastReservedWord
        self.current_token as u16 >= SyntaxKind::FIRST_RESERVED_WORD as u16
            && self.current_token as u16 <= SyntaxKind::LAST_RESERVED_WORD as u16
    }

    /// Get the text representation of the current keyword token
    pub(crate) const fn current_keyword_text(&self) -> &'static str {
        match self.current_token {
            SyntaxKind::BreakKeyword => "break",
            SyntaxKind::CaseKeyword => "case",
            SyntaxKind::CatchKeyword => "catch",
            SyntaxKind::ClassKeyword => "class",
            SyntaxKind::ConstKeyword => "const",
            SyntaxKind::ContinueKeyword => "continue",
            SyntaxKind::DebuggerKeyword => "debugger",
            SyntaxKind::DefaultKeyword => "default",
            SyntaxKind::DeleteKeyword => "delete",
            SyntaxKind::DoKeyword => "do",
            SyntaxKind::ElseKeyword => "else",
            SyntaxKind::EnumKeyword => "enum",
            SyntaxKind::ExportKeyword => "export",
            SyntaxKind::ExtendsKeyword => "extends",
            SyntaxKind::FalseKeyword => "false",
            SyntaxKind::FinallyKeyword => "finally",
            SyntaxKind::ForKeyword => "for",
            SyntaxKind::FunctionKeyword => "function",
            SyntaxKind::IfKeyword => "if",
            SyntaxKind::ImportKeyword => "import",
            SyntaxKind::InKeyword => "in",
            SyntaxKind::InstanceOfKeyword => "instanceof",
            SyntaxKind::NewKeyword => "new",
            SyntaxKind::NullKeyword => "null",
            SyntaxKind::ReturnKeyword => "return",
            SyntaxKind::SuperKeyword => "super",
            SyntaxKind::SwitchKeyword => "switch",
            SyntaxKind::ThisKeyword => "this",
            SyntaxKind::ThrowKeyword => "throw",
            SyntaxKind::TrueKeyword => "true",
            SyntaxKind::TryKeyword => "try",
            SyntaxKind::TypeOfKeyword => "typeof",
            SyntaxKind::VarKeyword => "var",
            SyntaxKind::VoidKeyword => "void",
            SyntaxKind::WhileKeyword => "while",
            SyntaxKind::WithKeyword => "with",
            // Future reserved words (strict mode)
            SyntaxKind::ImplementsKeyword => "implements",
            SyntaxKind::InterfaceKeyword => "interface",
            SyntaxKind::LetKeyword => "let",
            SyntaxKind::PackageKeyword => "package",
            SyntaxKind::PrivateKeyword => "private",
            SyntaxKind::ProtectedKeyword => "protected",
            SyntaxKind::PublicKeyword => "public",
            SyntaxKind::StaticKeyword => "static",
            SyntaxKind::YieldKeyword => "yield",
            _ => "reserved word",
        }
    }

    fn recover_after_reserved_word_in_variable_declaration(&mut self, keyword: SyntaxKind) {
        use tsz_common::diagnostics::diagnostic_codes;

        // Consume the reserved word token to prevent cascading errors.
        self.next_token();

        // In tsc, `var class;` causes the variable declaration list to abort, then the
        // statement loop reparses `class` as a class declaration which expects `{` but
        // finds `;`, emitting TS1005 '{' expected.' at the semicolon. We emit this
        // directly since we consume the token rather than letting it be reparsed.
        if keyword == SyntaxKind::ClassKeyword && self.is_token(SyntaxKind::SemicolonToken) {
            self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
        }

        // After consuming the reserved word, if the next token can't continue the
        // variable declaration (not `;`, `,`, `=`, `:`, `!`, `}`, or EOF), skip
        // remaining tokens on this statement to prevent cascading errors.
        // e.g., `const export as namespace oo4;` — after consuming `export` (TS1389),
        // skip `as namespace oo4` so only the semicolon remains.
        if !matches!(
            self.token(),
            SyntaxKind::SemicolonToken
                | SyntaxKind::CommaToken
                | SyntaxKind::EqualsToken
                | SyntaxKind::ColonToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::EndOfFileToken
        ) && !self.scanner.has_preceding_line_break()
        {
            // Skip tokens until we reach a statement boundary
            while !matches!(
                self.token(),
                SyntaxKind::SemicolonToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::EndOfFileToken
            ) && !self.scanner.has_preceding_line_break()
            {
                self.next_token();
            }
        }
    }

    /// Error: TS1389 - '{0}' is not allowed as a variable declaration name.
    /// Emitted when a reserved word appears as the binding name of a var/let/const/using declaration.
    ///
    /// In tsc, the reserved word is NOT consumed — the variable declaration list aborts and the
    /// keyword is reparsed by the statement loop. For `var class;`, this means `class` gets parsed
    /// as a class declaration, which then emits TS1005 `'{' expected.` when it finds `;`.
    /// We consume the token to avoid complex recovery differences, but explicitly emit the TS1005
    /// that tsc would produce when `class` is the keyword (since the class declaration would
    /// expect `{` at the semicolon position).
    pub(crate) fn error_reserved_word_in_variable_declaration(&mut self) {
        if self.should_report_error() {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            let keyword = self.token();
            let word = self.current_keyword_text();
            let msg = diagnostic_messages::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME
                .replace("{0}", word);
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
            );
            self.recover_after_reserved_word_in_variable_declaration(keyword);
        }
    }

    pub(crate) fn recover_reserved_word_variable_declaration_tail(&mut self, keyword: SyntaxKind) {
        self.recover_after_reserved_word_in_variable_declaration(keyword);
    }

    /// Error: TS1359 - Identifier expected. '{0}' is a reserved word that cannot be used here.
    pub(crate) fn error_reserved_word_identifier(&mut self) {
        // Use centralized error suppression heuristic
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            let word = self.current_keyword_text();
            if self.is_token(SyntaxKind::YieldKeyword) && self.in_generator_context() {
                self.report_yield_reserved_word_error();
                // Consume the reserved word token to prevent cascading errors
                self.next_token();
                return;
            }
            self.parse_error_at_current_token(
                &format!(
                    "Identifier expected. '{word}' is a reserved word that cannot be used here."
                ),
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
            );
            // Consume the reserved word token to prevent cascading errors
            self.next_token();
        }
    }

    /// Error: '{token}' expected (TS1005)
    pub(crate) fn error_token_expected(&mut self, token: &str) {
        // When the current token is Unknown (invalid character), emit only TS1127.
        // In tsc, the scanner emits TS1127 into parseDiagnostics via scanError callback
        // *before* the parser's parseExpected runs. Since tsc's parseErrorAtPosition dedup
        // suppresses errors at the same position as the last error, the parser's TS1005 is
        // always shadowed by the scanner's TS1127. We replicate this by emitting only TS1127.
        if self.is_token(SyntaxKind::Unknown) {
            if self.should_report_error() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                    diagnostic_codes::INVALID_CHARACTER,
                );
            }
            return;
        }
        // Only emit error if we haven't already emitted one at this position
        // This prevents cascading errors when parse_semicolon() and similar functions call this
        // Use centralized error suppression heuristic
        if self.should_report_error() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                &format!("'{token}' expected."),
                diagnostic_codes::EXPECTED,
            );
        }
    }

    /// Error: Comma expected (TS1005) - specifically for missing commas between parameters/arguments
    pub(crate) fn error_comma_expected(&mut self) {
        self.error_token_expected(",");
    }

    pub(crate) fn current_token_has_scanner_diagnostic(&self, code: u32) -> bool {
        let token_pos = self.token_pos() as usize;
        self.scanner
            .get_scanner_diagnostics()
            .iter()
            .any(|diag| diag.code == code && diag.pos == token_pos)
    }

    pub(crate) fn current_token_has_numeric_literal_follow_error(&self) -> bool {
        use tsz_common::diagnostics::diagnostic_codes;

        self.current_token_has_scanner_diagnostic(
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
        )
    }

    /// Check if current token could start a parameter
    pub(crate) fn is_parameter_start(&mut self) -> bool {
        // Parameters can start with modifiers, identifiers, or binding patterns
        self.is_parameter_modifier()
            || self.is_token(SyntaxKind::AtToken) // decorators on parameters
            || self.is_token(SyntaxKind::DotDotDotToken) // rest parameter
            || self.is_identifier_or_keyword()
            || self.is_token(SyntaxKind::OpenBraceToken) // object binding pattern
            || self.is_token(SyntaxKind::OpenBracketToken) // array binding pattern
    }

    /// Error: Unterminated template literal (TS1160)
    ///
    /// tsc reports this error at the END of the template content (where EOF was hit),
    /// not at the start (the backtick). We match that behavior.
    pub(crate) fn error_unterminated_template_literal_at(&mut self, _start: u32, end: u32) {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at(
            end,
            1,
            "Unterminated template literal.",
            diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL,
        );
    }

    /// Error: Declaration expected (TS1146)
    pub(crate) fn error_declaration_expected(&mut self) {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "Declaration expected.",
            diagnostic_codes::DECLARATION_EXPECTED,
        );
    }

    /// Error: Statement expected (TS1129)
    pub(crate) fn error_statement_expected(&mut self) {
        use tsz_common::diagnostics::diagnostic_codes;
        self.parse_error_at_current_token(
            "Statement expected.",
            diagnostic_codes::STATEMENT_EXPECTED,
        );
    }

    /// Check if a statement is a using/await using declaration not inside a block (TS1156)
    pub(crate) fn check_using_outside_block(&mut self, statement: NodeIndex) {
        use crate::parser::node_flags;
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if statement.is_none() {
            return;
        }

        // Get the node and check if it's a variable statement with using flags
        if let Some(node) = self.arena.get(statement) {
            // Check if it's a variable statement (not a block)
            if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                // Check if it has using or await using flags
                let is_using = (node.flags
                    & self.u16_from_node_flags(node_flags::USING | node_flags::AWAIT_USING))
                    != 0;
                if is_using {
                    // Emit TS1156 error at the statement position
                    self.parse_error_at(
                        node.pos,
                        node.end.saturating_sub(node.pos).max(1),
                        diagnostic_messages::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
                        diagnostic_codes::DECLARATIONS_CAN_ONLY_BE_DECLARED_INSIDE_A_BLOCK,
                    );
                }
            }
        }
    }

    /// Parse semicolon (or recover from missing)
    pub(crate) fn parse_semicolon(&mut self) {
        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        } else if self.is_token(SyntaxKind::Unknown) {
            // Scanner/lexer already reported an error for this token.
            // Avoid cascading TS1005 (';' expected) at the same position.
        } else if !self.can_parse_semicolon() {
            // Suppress cascading TS1005 "';' expected" when a recent error was already
            // emitted. This happens when a prior parse failure (e.g., missing identifier,
            // unsupported syntax) causes the parser to not consume tokens, then
            // parse_semicolon is called and fails too.
            // Use centralized error suppression heuristic
            if self.should_report_error() {
                self.error_token_expected(";");
            }
        }
    }

    // =========================================================================
    // Keyword suggestion for misspelled keywords (TS1434/TS1435/TS1438)
    // =========================================================================

    /// Provides a better error message than the generic "';' expected" for
    /// known common variants of a missing semicolon, such as misspelled keywords.
    ///
    /// Matches TypeScript's `parseErrorForMissingSemicolonAfter`.
    ///
    /// `expression` is the node index of the expression that was parsed before
    /// the missing semicolon.
    pub(crate) fn parse_error_for_missing_semicolon_after(&mut self, expression: NodeIndex) {
        use crate::parser::spelling;
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some((pos, len, expression_text)) =
            self.missing_semicolon_after_expression_text(expression)
        else {
            // For non-identifier expressions (postfix, literals, etc.),
            // emit a plain TS1005 "';' expected" via parse_error_at which
            // deduplicates by exact start position (matching tsc's
            // parseErrorAtCurrentToken). Skip if an error already exists
            // within the expression's range (the expression itself had errors).
            let expr_had_error = if let Some(node) = self.arena.get(expression) {
                self.last_error_pos > 0
                    && self.last_error_pos >= node.pos
                    && self.last_error_pos <= node.end
            } else {
                false
            };
            if !expr_had_error
                || self.is_token(SyntaxKind::ColonToken)
                || self.is_token(SyntaxKind::CloseParenToken)
                || self.is_token(SyntaxKind::OpenBraceToken)
            {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            }
            return;
        };

        if self.parse_missing_semicolon_keyword_error(pos, len, &expression_text) {
            return;
        }

        if self.should_suppress_type_or_keyword_suggestion_for_missing_semicolon(
            expression_text.as_str(),
            pos,
        ) {
            return;
        }

        if let Some(suggestion) = spelling::suggest_keyword(&expression_text) {
            if suggestion == "this" && self.is_token(SyntaxKind::DotToken) {
                self.parse_error_at(
                    pos,
                    len,
                    diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
                return;
            }
            if !self.should_suppress_type_or_keyword_suggestion_for_missing_semicolon(
                suggestion.as_str(),
                pos,
            ) {
                self.parse_error_at(
                    pos,
                    len,
                    &format!("Unknown keyword or identifier. Did you mean '{suggestion}'?"),
                    diagnostic_codes::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN,
                );
            }

            return;
        }

        if self.is_token(SyntaxKind::Unknown) {
            return;
        }

        // If the expression text is already an exact keyword (e.g., `from`, `get`, `set`),
        // the identifier appeared in error recovery from an upstream parse failure.
        // Emitting TS1434 "Unexpected keyword or identifier" here is a cascade artifact —
        // the real error was already reported. tsc suppresses this via different parsing
        // flow that doesn't reach this fallback for exact keywords.
        if spelling::VIABLE_KEYWORD_SUGGESTIONS
            .iter()
            .any(|&kw| kw == expression_text)
        {
            if matches!(
                self.token(),
                SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
            ) {
                self.parse_error_at(
                    pos,
                    len,
                    diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
                return;
            }
            // Keep the suppression for bare keyword recovery, but allow keyword-like
            // statements followed by a literal (notably `from "./mod"`) to report
            // TS1434 like tsc does.
            if !matches!(
                self.token(),
                SyntaxKind::StringLiteral
                    | SyntaxKind::NoSubstitutionTemplateLiteral
                    | SyntaxKind::TemplateHead
            ) {
                return;
            }
        }

        // tsc emits TS1434 "Unexpected keyword or identifier" at the expression
        // position for any identifier that isn't a recognized keyword/type.
        // Suppress when the following token is a closing delimiter (`)`, `]`)
        // that cannot start a new statement — the identifier is part of
        // cascading recovery from an earlier syntax error, not a standalone
        // statement missing a semicolon.
        if matches!(
            self.token(),
            SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
        ) {
            return;
        }
        self.parse_error_at(
            pos,
            len,
            diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
            diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
        );
    }

    fn missing_semicolon_after_expression_text(
        &self,
        expression: NodeIndex,
    ) -> Option<(u32, u32, String)> {
        let node = self.arena.get(expression)?;

        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        // Use source text directly — arena identifier data may be empty for
        // identifiers created during parsing before data is fully populated.
        let source = self.scanner.source_text();
        let text = &source[node.pos as usize..node.end as usize];
        if text.is_empty() {
            return None;
        }

        Some((node.pos, node.end - node.pos, text.to_string()))
    }

    fn parse_missing_semicolon_keyword_error(
        &mut self,
        pos: u32,
        len: u32,
        expression_text: &str,
    ) -> bool {
        use tsz_common::diagnostics::diagnostic_codes;

        match expression_text {
            "const" | "let" | "var" => {
                self.parse_error_at(
                    pos,
                    len,
                    "Variable declaration not allowed at this location.",
                    diagnostic_codes::VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION,
                );
                true
            }
            "declare" => true,
            "interface" => {
                if self.is_token(SyntaxKind::OpenBraceToken) {
                    self.parse_error_at_current_token(
                        "Interface must be given a name.",
                        diagnostic_codes::INTERFACE_MUST_BE_GIVEN_A_NAME,
                    );
                } else {
                    let name = self.scanner.get_token_value_ref().to_string();
                    self.parse_error_at_current_token(
                        &format!("Interface name cannot be '{name}'."),
                        diagnostic_codes::EXPECTED,
                    );
                }
                true
            }
            "type" => {
                self.parse_error_at_current_token("'=' expected.", diagnostic_codes::EXPECTED);
                true
            }
            _ => false,
        }
    }

    fn should_suppress_type_or_keyword_suggestion_for_missing_semicolon(
        &self,
        text: &str,
        token_pos: u32,
    ) -> bool {
        if !matches!(
            text,
            "string"
                | "number"
                | "boolean"
                | "symbol"
                | "bigint"
                | "object"
                | "void"
                | "undefined"
                | "null"
                | "never"
                | "unknown"
                | "any"
        ) {
            return false;
        }

        let source = self.scanner.source_text().as_bytes();
        let mut i = token_pos as usize;
        while i > 0 && source[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        i > 0 && source[i - 1] == b':'
    }

    /// Check if we can parse a semicolon (ASI rules)
    /// Returns true if current token is semicolon or ASI applies
    ///
    /// ASI (Automatic Semicolon Insertion) rules (matching TypeScript):
    /// 1. Explicit semicolon
    /// 2. Before closing brace
    /// 3. At EOF
    /// 4. After line break (no additional checks!)
    ///
    /// Note: This matches TypeScript's `canParseSemicolon()` implementation exactly.
    /// The previous "enhanced" ASI with statement start checks was causing
    /// false-positive TS1005 errors because it was more restrictive than TypeScript.
    pub(crate) fn can_parse_semicolon(&self) -> bool {
        // Explicit semicolon
        if self.is_token(SyntaxKind::SemicolonToken) {
            return true;
        }

        // ASI applies before closing brace
        if self.is_token(SyntaxKind::CloseBraceToken) {
            return true;
        }

        // ASI applies at EOF
        if self.is_token(SyntaxKind::EndOfFileToken) {
            return true;
        }

        // ASI applies after line break (matching TypeScript - no extra checks!)
        self.scanner.has_preceding_line_break()
    }

    /// Check if ASI applies for restricted productions (return, throw, yield, break, continue)
    ///
    /// Restricted productions have special ASI rules:
    /// ASI applies immediately after a line break, WITHOUT checking if the next token starts a statement.
    ///
    /// Examples:
    /// - `return\nx` parses as `return; x;` (ASI applies due to line break)
    /// - `return x` parses as `return x;` (no ASI, x is the return value)
    /// - `throw\nx` parses as `throw; x;` (ASI applies due to line break)
    /// - `throw x` parses as `throw x;` (no ASI, x is the thrown value)
    pub(crate) fn can_parse_semicolon_for_restricted_production(&self) -> bool {
        // Explicit semicolon
        if self.is_token(SyntaxKind::SemicolonToken) {
            return true;
        }

        // ASI applies before closing brace
        if self.is_token(SyntaxKind::CloseBraceToken) {
            return true;
        }

        // ASI applies at EOF
        if self.is_token(SyntaxKind::EndOfFileToken) {
            return true;
        }

        // ASI applies after line break (without checking statement start)
        // This is the key difference from can_parse_semicolon()
        if self.scanner.has_preceding_line_break() {
            return true;
        }

        false
    }

    // =========================================================================
    // Error Resynchronization
    // =========================================================================

    /// Check if we're at a position where an expression can reasonably end
    /// This is used to suppress spurious TS1109 "expression expected" errors when
    /// the user has clearly moved on to the next statement/context.
    ///
    /// For TS1109 (expression expected), we should only suppress if we've reached a closing
    /// delimiter or EOF. We should NOT suppress on statement start keywords because if we're
    /// expecting an expression and see `var`, `let`, `function`, etc., that's likely an error.
    pub(crate) const fn is_at_expression_end(&self) -> bool {
        match self.token() {
            // Only tokens that naturally end expressions and indicate we've moved on
            SyntaxKind::SemicolonToken
            | SyntaxKind::CloseBraceToken
            | SyntaxKind::CloseParenToken
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::EndOfFileToken => true,
            // NOTE: We do NOT suppress on statement start keywords
            // If we're expecting an expression and see `var`, `let`, `function`, etc.,
            // that's likely a genuine error where the user forgot the expression.
            // This fixes the "missing TS1109" issue where errors were being suppressed too aggressively.
            _ => false,
        }
    }

    /// Check if current token can start a statement (synchronization point)
    pub(crate) const fn is_statement_start(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::WithKeyword
                | SyntaxKind::DebuggerKeyword
                | SyntaxKind::ReturnKeyword
                | SyntaxKind::BreakKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::ThrowKeyword
                | SyntaxKind::YieldKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::AwaitKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::Identifier
                | SyntaxKind::StringLiteral
                | SyntaxKind::AtToken
                | SyntaxKind::NumericLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::ExclamationToken
                | SyntaxKind::TildeToken
                | SyntaxKind::PlusToken
                | SyntaxKind::MinusToken
                | SyntaxKind::PlusPlusToken
                | SyntaxKind::MinusMinusToken
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::OpenParenToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::LessThanToken
        )
    }

    /// Check if current token is a synchronization point for error recovery
    /// This includes statement starts plus additional keywords that indicate
    /// boundaries in control structures (else, case, default, catch, finally, etc.)
    pub(crate) const fn is_resync_sync_point(&self) -> bool {
        self.is_statement_start()
            || matches!(
                self.token(),
                SyntaxKind::ElseKeyword
                    | SyntaxKind::CaseKeyword
                    | SyntaxKind::DefaultKeyword
                    | SyntaxKind::CatchKeyword
                    | SyntaxKind::FinallyKeyword
                    | SyntaxKind::CommaToken
            )
    }

    /// Resynchronize after a parse error by skipping to the next statement boundary
    /// This prevents cascading errors by finding a known good synchronization point.
    /// `allow_statement_starts` controls whether token kinds that begin statements
    /// (especially identifiers) are valid sync points.
    pub(crate) fn resync_after_error_with_statement_starts(
        &mut self,
        allow_statement_starts: bool,
    ) {
        // If we're already at a sync point or EOF, no need to resync
        if self.is_resync_sync_point_with_statement_starts(allow_statement_starts) {
            return;
        }

        // Skip tokens until we find a synchronization point
        let mut brace_depth = 0u32;
        let mut paren_depth = 0u32;
        let mut bracket_depth = 0u32;
        let max_iterations = 1000; // Prevent infinite loops

        for _ in 0..max_iterations {
            // Check for EOF
            if self.is_token(SyntaxKind::EndOfFileToken) {
                break;
            }

            // Track nesting depth to handle nested structures
            match self.token() {
                SyntaxKind::OpenBraceToken => {
                    brace_depth += 1;
                    self.next_token();
                    continue;
                }
                SyntaxKind::CloseBraceToken => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                        self.next_token();
                        continue;
                    }
                    // Found closing brace at same level - this is a sync point
                    self.next_token();
                    break;
                }
                SyntaxKind::OpenParenToken => {
                    paren_depth += 1;
                    self.next_token();
                    continue;
                }
                SyntaxKind::CloseParenToken => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                        self.next_token();
                        continue;
                    }
                    // Found closing paren at same level - could be end of expression
                    // Skip it and check if next token is a sync point
                    self.next_token();
                    if self.is_resync_sync_point_with_statement_starts(allow_statement_starts) {
                        break;
                    }
                    continue;
                }
                SyntaxKind::OpenBracketToken => {
                    bracket_depth += 1;
                    self.next_token();
                    continue;
                }
                SyntaxKind::CloseBracketToken => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                        self.next_token();
                        continue;
                    }
                    // Found closing bracket at same level - skip it
                    self.next_token();
                    continue;
                }
                SyntaxKind::SemicolonToken => {
                    // Semicolon is always a sync point (even in nested contexts)
                    self.next_token();
                    break;
                }
                _ => {}
            }

            // If we're at depth 0 and found a sync point, we've resync'd
            if brace_depth == 0
                && paren_depth == 0
                && bracket_depth == 0
                && self.is_resync_sync_point_with_statement_starts(allow_statement_starts)
            {
                break;
            }

            // Keep skipping tokens
            self.next_token();
        }
    }

    fn is_resync_sync_point_with_statement_starts(&self, allow_statement_starts: bool) -> bool {
        self.is_resync_sync_point()
            && (allow_statement_starts || self.token() != SyntaxKind::Identifier)
    }

    /// Default resync behavior: allow statement starts as sync points.
    pub(crate) fn resync_after_error(&mut self) {
        self.resync_after_error_with_statement_starts(true);
    }

    // =========================================================================
    // Expression-Level Error Recovery
    // =========================================================================

    /// Check if current token can start an expression
    pub(crate) const fn is_expression_start(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::NumericLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::StringLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::TemplateMiddle
                | SyntaxKind::TemplateTail
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::Identifier
                | SyntaxKind::ThisKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::AnyKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::NumberKeyword
                | SyntaxKind::BooleanKeyword
                | SyntaxKind::SymbolKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::RequireKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::AwaitKeyword
                | SyntaxKind::YieldKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::InstanceOfKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::OfKeyword
                | SyntaxKind::SatisfiesKeyword
                | SyntaxKind::FromKeyword
                | SyntaxKind::AsKeyword
                | SyntaxKind::IsKeyword
                | SyntaxKind::AssertKeyword
                | SyntaxKind::AssertsKeyword
                | SyntaxKind::IntrinsicKeyword
                | SyntaxKind::OutKeyword
                | SyntaxKind::InferKeyword
                | SyntaxKind::ConstructorKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::KeyOfKeyword
                | SyntaxKind::UniqueKeyword
                | SyntaxKind::GlobalKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::DeferKeyword
                | SyntaxKind::PrivateIdentifier
                | SyntaxKind::PlusToken
                | SyntaxKind::MinusToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::TildeToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::PlusPlusToken
                | SyntaxKind::MinusMinusToken
                | SyntaxKind::OpenParenToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::SlashToken
                | SyntaxKind::SlashEqualsToken
                | SyntaxKind::AtToken
        )
    }

    /// Check if current token is a binary operator
    pub(crate) const fn is_binary_operator(&self) -> bool {
        let precedence = self.get_operator_precedence(self.token());
        precedence > 0
    }

    /// Check if current token is at an expression boundary (a natural stopping point)
    pub(crate) const fn is_expression_boundary(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::SemicolonToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::CommaToken
                | SyntaxKind::ColonToken
                | SyntaxKind::CaseKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::WhileKeyword // for do-while
                | SyntaxKind::AsKeyword
                | SyntaxKind::SatisfiesKeyword
        )
    }

    /// Create a missing expression placeholder for error recovery.
    /// This allows the AST to remain structurally valid even when an expression is missing.
    pub(crate) fn create_missing_expression(&mut self) -> NodeIndex {
        let pos = self.token_pos();
        // Create an identifier with empty text to represent missing expression
        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            pos,
            pos,
            IdentifierData {
                atom: Atom::NONE,
                escaped_text: String::new(),
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Try to recover from a missing right-hand operand in a binary expression.
    /// Returns a placeholder expression if recovery is possible.
    pub(crate) fn try_recover_binary_rhs(&mut self) -> NodeIndex {
        // If we're at an expression boundary after an operator, create a placeholder
        if self.is_expression_boundary() || self.is_statement_start() {
            self.create_missing_expression()
        } else {
            NodeIndex::NONE
        }
    }

    /// Try to rescan `>` as a compound token (`>>`, `>>>`, `>=`, `>>=`, `>>>=`)
    /// Returns the rescanned token (which may be unchanged if no compound token found)
    pub(crate) fn try_rescan_greater_token(&mut self) -> SyntaxKind {
        if self.current_token == SyntaxKind::GreaterThanToken {
            self.current_token = self.scanner.re_scan_greater_token();
        }
        self.current_token
    }

    /// Parse expected `>` token, handling compound tokens like `>>` and `>>>`
    /// When we have `>>`, we need to consume just one `>` and leave `>` for the next parse
    pub(crate) fn parse_expected_greater_than(&mut self) {
        match self.current_token {
            SyntaxKind::GreaterThanToken => {
                // Simple case - just consume the single `>`
                self.next_token();
            }
            SyntaxKind::GreaterThanGreaterThanToken => {
                // `>>` - back up scanner and treat as single `>`
                // After consuming, the remaining `>` becomes the current token
                self.scanner.set_pos(self.scanner.get_pos() - 1);
                self.current_token = SyntaxKind::GreaterThanToken;
            }
            SyntaxKind::GreaterThanGreaterThanGreaterThanToken => {
                // `>>>` - back up scanner and treat as single `>`
                // After consuming, the remaining `>>` becomes the current token
                self.scanner.set_pos(self.scanner.get_pos() - 2);
                self.current_token = SyntaxKind::GreaterThanGreaterThanToken;
            }
            SyntaxKind::GreaterThanEqualsToken => {
                // `>=` - back up scanner and treat as single `>`
                self.scanner.set_pos(self.scanner.get_pos() - 1);
                self.current_token = SyntaxKind::EqualsToken;
            }
            SyntaxKind::GreaterThanGreaterThanEqualsToken => {
                // `>>=` - back up scanner and treat as single `>`
                self.scanner.set_pos(self.scanner.get_pos() - 2);
                self.current_token = SyntaxKind::GreaterThanEqualsToken;
            }
            SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken => {
                // `>>>=` - back up scanner and treat as single `>`
                self.scanner.set_pos(self.scanner.get_pos() - 3);
                self.current_token = SyntaxKind::GreaterThanGreaterThanEqualsToken;
            }
            _ => {
                // error_token_expected already has error suppression check
                self.error_token_expected(">");
            }
        }
    }

    /// Check if the current token starts with `<` (includes `<<` and `<<=`).
    /// Mirrors `is_greater_than_or_compound` for the opening side.
    pub(crate) const fn is_less_than_or_compound(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::LessThanToken
                | SyntaxKind::LessThanLessThanToken
                | SyntaxKind::LessThanLessThanEqualsToken
        )
    }

    /// Consume a single `<` from the current token.
    /// Handles compound tokens like `<<` and `<<=` by leaving the scanner
    /// position unchanged (past the compound token) and setting `current_token`
    /// to the remainder. Unlike `>`, `<` is eagerly combined by the scanner,
    /// so we cannot back up—the scanner would re-combine. Instead, we leave
    /// pos past the compound and set `current_token` to the remainder.
    /// When the remainder is later consumed via `parse_expected(<)` →
    /// `next_token()`, the scanner scans from past the compound, correctly
    /// yielding the token that follows.
    pub(crate) fn parse_expected_less_than(&mut self) {
        match self.current_token {
            SyntaxKind::LessThanToken => {
                self.next_token();
            }
            SyntaxKind::LessThanLessThanToken => {
                // `<<` → consume first `<`, remainder is `<`
                // Scanner pos stays past `<<`; when the second `<` is consumed,
                // next_token() will scan from past both, yielding the following token.
                self.current_token = SyntaxKind::LessThanToken;
            }
            SyntaxKind::LessThanLessThanEqualsToken => {
                // `<<=` → consume first `<`, remainder is `<=`
                // Scanner pos stays past `<<=`; same logic as above.
                self.current_token = SyntaxKind::LessThanEqualsToken;
            }
            _ => {
                self.error_token_expected("<");
            }
        }
    }

    /// Create a `NodeList` from a Vec of `NodeIndex`
    pub(crate) const fn make_node_list(&self, nodes: Vec<NodeIndex>) -> NodeList {
        let _ = self;
        NodeList {
            nodes,
            pos: 0,
            end: 0,
            has_trailing_comma: false,
        }
    }

    /// Get operator precedence
    pub(crate) const fn get_operator_precedence(&self, token: SyntaxKind) -> u8 {
        // NOTE: Assignment operators (=, +=, -=, etc.) are NOT handled by the
        // binary expression chain. They are handled at a higher level in
        // parse_assignment_expression, matching tsc's separation of
        // parseAssignmentExpressionOrHigher vs parseBinaryExpressionRest.
        // They fall through to the default `_ => 0` arm below, which prevents
        // the binary expression loop from consuming `=` after error recovery
        // (e.g., `1 >> = 2` should parse as `1 >> <missing>; = 2;`, not
        // `(1 >> <missing>) = 2`).
        match token {
            SyntaxKind::CommaToken => 1,
            SyntaxKind::QuestionToken => 3,
            SyntaxKind::BarBarToken | SyntaxKind::QuestionQuestionToken => 4,
            SyntaxKind::AmpersandAmpersandToken => 5,
            SyntaxKind::BarToken => 6,
            SyntaxKind::CaretToken => 7,
            SyntaxKind::AmpersandToken => 8,
            SyntaxKind::EqualsEqualsToken
            | SyntaxKind::ExclamationEqualsToken
            | SyntaxKind::EqualsEqualsEqualsToken
            | SyntaxKind::ExclamationEqualsEqualsToken => 9,
            // 'in' is not a binary operator in for-statement initializers
            SyntaxKind::InKeyword => {
                if self.in_disallow_in_context() {
                    0
                } else {
                    10
                }
            }
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::LessThanEqualsToken
            | SyntaxKind::GreaterThanEqualsToken
            | SyntaxKind::InstanceOfKeyword
            | SyntaxKind::AsKeyword
            | SyntaxKind::SatisfiesKeyword => 10,
            SyntaxKind::LessThanLessThanToken
            | SyntaxKind::GreaterThanGreaterThanToken
            | SyntaxKind::GreaterThanGreaterThanGreaterThanToken => 11,
            SyntaxKind::PlusToken | SyntaxKind::MinusToken => 12,
            SyntaxKind::AsteriskToken | SyntaxKind::SlashToken | SyntaxKind::PercentToken => 13,
            SyntaxKind::AsteriskAsteriskToken => 14,
            _ => 0,
        }
    }

    /// Push a new label scope (called when entering a function or module)
    pub(crate) fn push_label_scope(&mut self) {
        let new_depth = self.label_scopes.len() + 1;
        trace!(pos = self.token_pos(), new_depth, "push_label_scope");
        self.label_scopes.push(FxHashMap::default());
    }

    /// Pop the current label scope (called when exiting a function or module)
    pub(crate) fn pop_label_scope(&mut self) {
        let old_depth = self.label_scopes.len();
        trace!(pos = self.token_pos(), old_depth, "pop_label_scope");
        self.label_scopes.pop();
    }

    /// Check if a label already exists in the current scope, and if so, emit TS1114.
    /// Returns true if the label is a duplicate.
    pub(crate) fn check_duplicate_label(&mut self, label_name: &str, label_pos: u32) -> bool {
        let scope_depth = self.label_scopes.len();
        trace!(label_name, label_pos, scope_depth, "check_duplicate_label");
        if let Some(current_scope) = self.label_scopes.last_mut() {
            if current_scope.contains_key(label_name) {
                // Duplicate label - emit TS1114
                use tsz_common::diagnostics::diagnostic_codes;
                let message = format!("Duplicate label '{label_name}'.");
                trace!(label_name, "duplicate label found");
                self.parse_error_at(
                    label_pos,
                    self.u32_from_usize(label_name.len()),
                    &message,
                    diagnostic_codes::DUPLICATE_LABEL,
                );
                return true;
            }
            // Not a duplicate - record this label
            trace!(label_name, "adding label to scope");
            current_scope.insert(label_name.to_string(), label_pos);
        }
        false
    }
}

// Integration tests for parse_error_for_missing_semicolon_after live in
// parser/tests/spelling_integration_tests.rs.  Pure spelling-logic tests
// live in parser/spelling.rs.
