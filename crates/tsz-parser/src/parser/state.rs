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

use tsz_common::ScriptTarget;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::file_extensions::is_ts_declaration_file_name;
use tsz_common::limits::MAX_PARSER_RECURSION_DEPTH;

use std::cell::Cell;

use crate::parser::{
    NodeIndex, NodeList,
    node::{IdentifierData, NodeArena},
    syntax_kind_ext,
};
use rustc_hash::FxHashMap;
use tracing::warn;
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
/// Used for class-field-only recovery and keyword restrictions.
pub const CONTEXT_FLAG_CLASS_FIELD_INITIALIZER: u32 = 32768;
/// Context flag: parsing inside a tuple element where `?` is an optional marker.
/// When set, postfix `?` should NOT be treated as JSDoc nullable (TS17019).
pub const CONTEXT_FLAG_IN_TUPLE_ELEMENT: u32 = 65536;
/// Context flag: parsing the property name of a generator method (`* [name]`).
/// Suppresses TS1213 for `yield` in computed property names of generator methods
/// (tsc does not emit TS1213 in this position).
pub const CONTEXT_FLAG_GENERATOR_MEMBER_NAME: u32 = 131072;
/// Context flag: parsing a `${...}` template span expression.
/// Empty spans at EOF report TS1109 from template-span recovery so the
/// expression error can anchor before trailing trivia while TS1005 anchors at EOF.
pub const CONTEXT_FLAG_TEMPLATE_SPAN_EXPRESSION: u32 = 262144;
/// Context flag: parsing a binding pattern as a function parameter name.
pub const CONTEXT_FLAG_PARAMETER_BINDING_PATTERN: u32 = 524288;
/// Context flag: parsing a function-like body.
pub const CONTEXT_FLAG_FUNCTION_BODY: u32 = 1048576;

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
    /// ECMAScript target used by target-sensitive scanner recovery.
    pub(crate) language_version: ScriptTarget,
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
    /// Number of scanner diagnostics observed at the time the most recent
    /// parser-side diagnostic was pushed. `scanner_diagnostics[idx..]` for
    /// any `idx >= this` represents scanner emissions that happened *after*
    /// our last parser push and therefore are the effective "lastError" tail
    /// for tsc's `parseErrorAtPosition` `lastError.start` dedup. Without
    /// this, a TS1124 emitted by the scanner (`1ee`'s empty exponent) would
    /// not suppress a follow-up TS1005 the parser emits at the same position
    /// the way tsc's single `parseDiagnostics` vec does.
    pub(crate) scanner_diagnostics_high_water_mark: usize,
    /// Tracks whether we've already reported a usize->u32 offset overflow
    /// during the current parse session to avoid log spam on pathological input.
    pub(crate) reported_offset_overflow: Cell<bool>,
    /// Tracks whether we've already reported a u32->u16 node-flag overflow
    /// during the current parse session to avoid log spam on pathological input.
    pub(crate) reported_node_flag_overflow: Cell<bool>,
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
    /// Whether the current import/export specifier consumed scanner debris from
    /// an invalid braced unicode escape in an identifier tail.
    pub(crate) current_specifier_recovered_braced_unicode_escape_debris: bool,
    /// When recovery consumes a malformed arrow-body `}` directly, keep a small
    /// number of following module-closing braces in the token stream so outer
    /// list recovery can report them as stray braces.
    pub(crate) deferred_module_close_braces: u32,
    /// When malformed import-attribute recovery breaks a type constituent,
    /// stop consuming `&`-continued intersections so the tail falls back to
    /// statement-level recovery like TypeScript.
    pub(crate) abort_intersection_continuation: bool,
    /// When statement-like recovery inside a type-member container should leave
    /// actual `}` tokens for statement-level TS1128 recovery, skip this many
    /// enclosing close-brace expectations.
    pub(crate) deferred_type_member_close_braces: u32,
    /// After malformed import-attribute recovery inside an intersection type,
    /// parse the next `import()` options object with generic expression
    /// grammar so its diagnostics degrade like TypeScript's fallback path.
    pub(crate) fallback_import_type_options_once: bool,
    /// A malformed array-binding tail should keep `=` visible to declaration
    /// recovery so statement-level TS1128 can land there instead of being
    /// consumed as a normal initializer.
    pub(crate) pending_array_binding_tail_recovery: bool,
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
    /// Class-member recovery has already treated a previously consumed `}` as the
    /// class close, so the enclosing class parser should not also emit `}` expected.
    pub(crate) suppress_next_missing_class_close_brace_error_once: bool,
    /// A class declaration recovered from a missing `{` at a stray `.`, so the
    /// next non-block `}` should be treated as a stray statement-list token.
    pub(crate) non_block_close_brace_statement_errors_remaining: u8,
    /// Recovery has already consumed stray outer `}` tokens, so do not add a
    /// final missing-`}` cascade at EOF for the abandoned statement-list
    /// container. The stored depth scopes the suppression to that container,
    /// so nested EOF close-brace expectations still report their own errors.
    pub(crate) suppress_missing_close_brace_at_eof_statement_depth: Option<u32>,
    /// Number of active block-like statement lists being parsed. Used only to
    /// scope abandoned-container EOF close-brace suppression.
    pub(crate) statement_list_depth: u32,
    /// Speculative async-arrow parsing consumed `=>` while recovering a malformed
    /// parameter list, so the async-arrow candidate must roll back.
    pub(crate) saw_arrow_parameter_recovery: bool,
    /// A failed async-arrow speculation left a trailing `: Type =>` tail that
    /// should use the narrower variable-declaration recovery path.
    pub(crate) pending_failed_async_arrow_colon_recovery: bool,
    /// Depth of nested type-member containers (interfaces, type literals,
    /// mapped types with member tails) currently being parsed.
    pub(crate) type_member_container_depth: u32,
    /// When true, suppress escape-sequence errors in template literals.
    /// Tagged templates (ES2018+) allow invalid escape sequences.
    pub(crate) in_tagged_template: bool,
    /// Number of JSX child-expression recoveries in the current expression
    /// statement that deferred a missing `}`. When the statement terminator is
    /// reached, emit TS1005 `'}' expected.` at `;` to match tsc recovery.
    pub(crate) pending_jsx_missing_close_brace_in_expression_statement: u32,
    /// Extra expression statements recovered while parsing a preceding statement.
    /// Used for invalid conditional tails after block-bodied arrows where tsc
    /// still emits the branch expressions as standalone statements.
    pub(crate) pending_recovered_expression_statements: Vec<NodeIndex>,
    /// Current lower bound for scanning parse diagnostics when JSX recovery
    /// absorbs statement terminators into `JsxText`.
    pub(crate) jsx_missing_brace_semicolon_window_start: Option<u32>,
    /// An empty JSX attribute expression (`attr={}`) should not synthesize a
    /// semicolon-position missing `}` while recovering the surrounding element.
    pub(crate) suppress_next_jsx_missing_brace_at_semicolon: bool,
    /// We are parsing a nested JSX element as an attribute initializer
    /// (`attr=<...>`). Invalid nested heads use JSX-attribute diagnostics.
    pub(crate) in_jsx_attribute_initializer_element: bool,
    /// A JSX attribute list consumed a string literal without `=`, as in
    /// `<div className"app">`; report expression recovery at semicolon.
    pub(crate) recover_jsx_missing_attr_initializer_head: bool,
    /// A malformed JSX attribute list used bracket syntax in a tag head, as in
    /// `<a[foo]>`; the closing tag should be recovered by outer expression code.
    pub(crate) suppress_next_jsx_head_missing_semicolon: bool,
    /// A JSX closing tag had trailing attributes (`</div {...props}>`). The
    /// tail is recovered as source-level syntax after the JSX expression.
    pub(crate) recover_jsx_closing_tag_trailing_tail: bool,
    /// A JSX closing tag had a second namespace separator (`</a:b:c>`). The
    /// parsed closing name stops at `a:b`; the `:c` tail is recovered by the
    /// surrounding expression/declaration parser.
    pub(crate) recover_jsx_closing_tag_extra_namespace_tail: bool,
    /// A TSX expression started with an invalid namespace head (`<:a`). The `<`
    /// is recovered as the initializer expression and the `:a` tail belongs to
    /// declaration/expression recovery.
    pub(crate) recover_jsx_invalid_namespace_head_tail: bool,
    /// Set when `parse_namespace_import` encountered a reserved word that also
    /// starts a statement (e.g. `while` in `import * as while from "foo"`).
    /// Signals `parse_import_declaration_with_modifiers` to bail out of import
    /// recovery without consuming the token, so the outer statement parser
    /// re-parses it as the head of a statement — matching tsc, which emits
    /// TS1359 at the reserved word and then cascades the statement's
    /// diagnostics (`'(' expected.` / `')' expected.`) at the following tokens.
    pub(crate) namespace_import_yielded_to_statement: bool,
}

impl ParserState {
    #[inline]
    #[must_use]
    pub(crate) fn u32_from_usize(&self, value: usize) -> u32 {
        match u32::try_from(value) {
            Ok(value) => value,
            Err(_) => {
                if !self.reported_offset_overflow.replace(true) {
                    warn!(
                        overflow_value = value,
                        "parser offset overflowed u32; clamping to u32::MAX"
                    );
                }
                u32::MAX
            }
        }
    }

    #[inline]
    #[must_use]
    pub(crate) fn u16_from_node_flags(&self, value: u32) -> u16 {
        match u16::try_from(value) {
            Ok(value) => value,
            Err(_) => {
                if !self.reported_node_flag_overflow.replace(true) {
                    warn!(
                        overflow_value = value,
                        "parser node flags overflowed u16; truncating high bits"
                    );
                }
                (value & u32::from(u16::MAX)) as u16
            }
        }
    }

    /// Create a new Parser for the given source text.
    #[must_use]
    pub fn new(file_name: String, source_text: String) -> Self {
        Self::new_with_language_version(file_name, source_text, ScriptTarget::default())
    }

    /// Create a new Parser for the given source text and ECMAScript target.
    #[must_use]
    pub fn new_with_language_version(
        file_name: String,
        source_text: String,
        language_version: ScriptTarget,
    ) -> Self {
        let estimated_nodes = source_text.len() / 20; // Rough estimate
        // Zero-copy: Pass source_text directly to scanner without cloning
        // This eliminates the 2x memory overhead from duplicating the source
        let mut scanner = ScannerState::new(source_text, true);
        scanner.set_language_version(language_version);
        Self {
            scanner,
            arena: NodeArena::with_capacity(estimated_nodes),
            file_name,
            language_version,
            context_flags: 0,
            current_token: SyntaxKind::Unknown,
            parse_diagnostics: Vec::new(),
            node_count: 0,
            recursion_depth: 0,
            last_error_pos: 0,
            scanner_diagnostics_high_water_mark: 0,
            reported_offset_overflow: Cell::new(false),
            reported_node_flag_overflow: Cell::new(false),
            label_scopes: vec![FxHashMap::default()],
            seen_module_indicator: false,
            last_named_imports_consumed_closing_brace: false,
            last_named_imports_recovered_to_from: false,
            last_named_imports_had_structural_error: false,
            current_specifier_recovered_braced_unicode_escape_debris: false,
            deferred_module_close_braces: 0,
            abort_intersection_continuation: false,
            deferred_type_member_close_braces: 0,
            fallback_import_type_options_once: false,
            pending_array_binding_tail_recovery: false,
            in_import_type_options_context: false,
            import_attribute_tail_recovered: false,
            suppress_object_literal_comma_once: false,
            suppress_next_missing_close_paren_error_once: false,
            suppress_next_missing_class_close_brace_error_once: false,
            non_block_close_brace_statement_errors_remaining: 0,
            suppress_missing_close_brace_at_eof_statement_depth: None,
            statement_list_depth: 0,
            saw_arrow_parameter_recovery: false,
            pending_failed_async_arrow_colon_recovery: false,
            type_member_container_depth: 0,
            in_tagged_template: false,
            pending_jsx_missing_close_brace_in_expression_statement: 0,
            pending_recovered_expression_statements: Vec::new(),
            jsx_missing_brace_semicolon_window_start: None,
            suppress_next_jsx_missing_brace_at_semicolon: false,
            in_jsx_attribute_initializer_element: false,
            recover_jsx_missing_attr_initializer_head: false,
            suppress_next_jsx_head_missing_semicolon: false,
            recover_jsx_closing_tag_trailing_tail: false,
            recover_jsx_closing_tag_extra_namespace_tail: false,
            recover_jsx_invalid_namespace_head_tail: false,
            namespace_import_yielded_to_statement: false,
        }
    }

    pub fn reset(&mut self, file_name: String, source_text: String) {
        self.file_name = file_name;
        self.scanner.set_text(source_text, None, None);
        self.scanner.set_language_version(self.language_version);
        self.arena.clear();
        self.context_flags = 0;
        self.current_token = SyntaxKind::Unknown;
        self.parse_diagnostics.clear();
        self.node_count = 0;
        self.recursion_depth = 0;
        self.last_error_pos = 0;
        self.reported_offset_overflow.set(false);
        self.reported_node_flag_overflow.set(false);
        self.label_scopes.clear();
        self.label_scopes.push(FxHashMap::default());
        self.seen_module_indicator = false;
        self.last_named_imports_consumed_closing_brace = false;
        self.last_named_imports_recovered_to_from = false;
        self.last_named_imports_had_structural_error = false;
        self.current_specifier_recovered_braced_unicode_escape_debris = false;
        self.deferred_module_close_braces = 0;
        self.deferred_type_member_close_braces = 0;
        self.abort_intersection_continuation = false;
        self.fallback_import_type_options_once = false;
        self.pending_array_binding_tail_recovery = false;
        self.in_import_type_options_context = false;
        self.import_attribute_tail_recovered = false;
        self.suppress_object_literal_comma_once = false;
        self.suppress_next_missing_close_paren_error_once = false;
        self.suppress_next_missing_class_close_brace_error_once = false;
        self.non_block_close_brace_statement_errors_remaining = 0;
        self.suppress_missing_close_brace_at_eof_statement_depth = None;
        self.statement_list_depth = 0;
        self.saw_arrow_parameter_recovery = false;
        self.pending_failed_async_arrow_colon_recovery = false;
        self.type_member_container_depth = 0;
        self.in_tagged_template = false;
        self.pending_jsx_missing_close_brace_in_expression_statement = 0;
        self.pending_recovered_expression_statements.clear();
        self.jsx_missing_brace_semicolon_window_start = None;
        self.suppress_next_jsx_missing_brace_at_semicolon = false;
        self.in_jsx_attribute_initializer_element = false;
        self.recover_jsx_missing_attr_initializer_head = false;
        self.suppress_next_jsx_head_missing_semicolon = false;
        self.recover_jsx_closing_tag_trailing_tail = false;
        self.recover_jsx_closing_tag_extra_namespace_tail = false;
        self.recover_jsx_invalid_namespace_head_tail = false;
        self.namespace_import_yielded_to_statement = false;
        // The high-water mark tracks the count of scanner diagnostics that
        // have been considered by the parser-side dedup at `parse_error_at`.
        // When the parser is reused via `reset()` the caller passes a fresh
        // source text. Without clearing the mark AND the scanner's diagnostic
        // vec, the dedup check at line 1080 (`scanner_diags.len() > HWM`) could
        // see a stale `last()` whose `pos` accidentally matches a new-parse
        // error and wrongly suppress it. The Scanner's `set_text` (called
        // above) intentionally does NOT clear the diagnostics for callers
        // outside ParserState, so we explicitly clear them here. (Devin
        // review on PR #1521.)
        self.scanner.clear_scanner_diagnostics();
        self.scanner_diagnostics_high_water_mark = 0;
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

    /// Returns true when the most recent parse diagnostic was a leading-zero
    /// numeric literal error (TS1121 / TS1489) at a position different from
    /// the current token. These are orthogonal to the missing-semicolon
    /// error (TS1005) that follows them in cases like `00.5;` — tsc emits
    /// both because its `parseErrorAtPosition` dedups only by exact start.
    pub(crate) fn last_error_was_leading_zero_at_other_pos(&self) -> bool {
        use tsz_common::diagnostics::diagnostic_codes;
        let Some(last) = self.parse_diagnostics.last() else {
            return false;
        };
        let is_leading_zero = last.code
            == diagnostic_codes::OCTAL_LITERALS_ARE_NOT_ALLOWED_USE_THE_SYNTAX
            || last.code == diagnostic_codes::DECIMALS_WITH_LEADING_ZEROS_ARE_NOT_ALLOWED;
        is_leading_zero && last.start != self.token_pos()
    }

    pub(crate) fn should_emit_jsx_missing_close_brace_at_semicolon(
        &self,
        range_start: u32,
        semicolon_pos: u32,
    ) -> bool {
        let has_unexpected_brace = self.parse_diagnostics.iter().any(|diag| {
            diag.start >= range_start
                && diag.start < semicolon_pos
                && diag.code == diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_RBRACE
        });
        if !has_unexpected_brace {
            return false;
        }

        let has_jsx_unclosed_tag = self.parse_diagnostics.iter().any(|diag| {
            diag.start >= range_start
                && diag.start < semicolon_pos
                && diag.code == diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG
        });
        if !has_jsx_unclosed_tag {
            return false;
        }

        let has_missing_close_brace = self.parse_diagnostics.iter().any(|diag| {
            diag.start >= range_start
                && diag.start <= semicolon_pos
                && diag.code == diagnostic_codes::EXPECTED
                && diag.message == "'}' expected."
        });

        !has_missing_close_brace
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
        is_ts_declaration_file_name(&self.file_name)
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

    /// Returns true when the current `Unknown` token is the leading backslash
    /// of scanner debris for a braced unicode escape (`\u{...}`).
    pub(crate) fn current_unknown_starts_braced_unicode_escape_debris(&mut self) -> bool {
        if !self.is_token(SyntaxKind::Unknown) || self.scanner.get_token_text_ref() != "\\" {
            return false;
        }

        let unknown_end = self.token_end();
        let saved_token = self.current_token;
        let saved_state = self.scanner.save_state();

        self.next_token();
        let saw_escape_u = self.token_pos() == unknown_end
            && self.is_identifier_or_keyword()
            && self.scanner.get_token_text_ref() == "u";
        let u_end = self.token_end();

        let result = if saw_escape_u {
            self.next_token();
            self.token_pos() == u_end && self.is_token(SyntaxKind::OpenBraceToken)
        } else {
            false
        };

        self.scanner.restore_state(saved_state);
        self.current_token = saved_token;
        result
    }

    /// Consume the current invalid-character token and the adjacent `u` token
    /// from braced unicode escape debris, leaving the parser at `{`.
    pub(crate) fn consume_braced_unicode_escape_debris_after_unknown(&mut self) {
        debug_assert!(self.current_unknown_starts_braced_unicode_escape_debris());
        self.parse_error_at_current_token(
            tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
            tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
        );
        self.next_token(); // consume `\`
        if self.is_identifier_or_keyword() && self.scanner.get_token_text_ref() == "u" {
            self.next_token(); // consume `u`, leaving `{`
        }
    }

    /// Returns true when the current `Unknown` token is non-braced
    /// backslash/escape debris from an invalid unicode escape in a declaration
    /// identifier.
    pub(crate) fn current_unknown_starts_invalid_unicode_identifier_debris(&self) -> bool {
        if !self.is_token(SyntaxKind::Unknown) {
            return false;
        }

        let src = self.scanner.source_text();
        let start = self.scanner.get_token_start();
        let bytes = src.as_bytes();
        bytes.get(start) == Some(&b'\\')
            && bytes.get(start + 1) == Some(&b'u')
            && bytes.get(start + 2) != Some(&b'{')
    }

    /// Recover an invalid unicode escape that appears where a declaration
    /// identifier is required. `tsc` drops the leading backslash and keeps the
    /// `u...` debris as identifier text, optionally merging the adjacent
    /// identifier token when the scanner split after a valid-but-illegal
    /// identifier-start escape such as `\u0031a`.
    pub(crate) fn parse_recovered_invalid_unicode_escape_identifier(&mut self) -> NodeIndex {
        debug_assert!(self.current_unknown_starts_invalid_unicode_identifier_debris());
        let start_pos = self.token_pos();
        let mut end_pos = self.token_end();
        let token_text = self.scanner.get_token_text_ref();
        let mut recovered = token_text
            .strip_prefix('\\')
            .unwrap_or(token_text)
            .to_string();

        self.parse_error_at_current_token(
            tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
            tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
        );
        self.next_token();

        while self.token_pos() == end_pos && self.is_identifier_or_keyword() {
            let text = if (self.scanner.get_token_flags() & TokenFlags::UnicodeEscape as u32) != 0 {
                self.scanner.get_token_value_ref().to_string()
            } else {
                self.scanner.get_token_text_ref().to_string()
            };
            recovered.push_str(&text);
            end_pos = self.token_end();
            self.next_token();
        }

        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                atom: Atom::NONE,
                escaped_text: recovered,
                original_text: None,
                type_arguments: None,
            },
        )
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

    /// Returns true if the current token is contextually reserved and cannot be used
    /// as a break/continue label in the current context. Matches tsc's `isIdentifier()`
    /// which returns false for `await` in await context and `yield` in yield context.
    #[inline]
    pub(crate) fn is_contextually_reserved_label(&self) -> bool {
        (self.is_token(SyntaxKind::AwaitKeyword)
            && (self.in_static_block_context() || self.in_async_context()))
            || (self.is_token(SyntaxKind::YieldKeyword) && self.in_generator_context())
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

    pub(crate) const fn in_function_body_context(&self) -> bool {
        (self.context_flags & CONTEXT_FLAG_FUNCTION_BODY) != 0
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

        // In static blocks, 'await' used as a class member computed property name
        // should emit TS1109 "Expression expected", not TS1213. Check this before
        // the general class-member modifier check so tsc parity is preserved.
        if self.in_class_member_name()
            && self.in_static_block_context()
            && self.is_token(SyntaxKind::AwaitKeyword)
        {
            return true;
        }

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
    ///
    /// Returns `true` when the recovery fully terminated the member — either by
    /// consuming a body block (`{ ... }`) or by consuming a member-terminator
    /// (`;` or `,`). Callers should skip their own body lookup in this case to
    /// avoid emitting a redundant `'{' expected.` past the actual terminator.
    pub(crate) fn recover_from_missing_method_open_paren(&mut self) -> bool {
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
            return true;
        }

        if self.is_token(SyntaxKind::SemicolonToken) || self.is_token(SyntaxKind::CommaToken) {
            self.next_token();
            return true;
        }
        false
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
        if kind == SyntaxKind::CloseBraceToken
            && self.suppress_next_missing_class_close_brace_error_once
        {
            self.suppress_next_missing_class_close_brace_error_once = false;
            if !self.is_token(SyntaxKind::CloseBraceToken) {
                return false;
            }
        }
        if kind == SyntaxKind::CloseBraceToken && self.is_token(SyntaxKind::EndOfFileToken) {
            let suppress_for_this_statement_list = self
                .suppress_missing_close_brace_at_eof_statement_depth
                .is_some_and(|depth| depth == self.statement_list_depth);
            if suppress_for_this_statement_list {
                self.suppress_missing_close_brace_at_eof_statement_depth = None;
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
                    && self.last_error_pos != self.token_pos())
                || (kind == SyntaxKind::LessThanSlashToken
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
            SyntaxKind::WhileKeyword => "while",
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
            self.scanner_diagnostics_high_water_mark = self.scanner.get_scanner_diagnostics().len();
            return;
        }
        // tsc routes scanner errors through the same `parseErrorAtPosition`
        // path via `scanError`, so they share the same `lastError` slot. We
        // mirror that here: any scanner diagnostics emitted *after* our most
        // recent parser push are the effective "lastError" tail. If the very
        // last such scanner diagnostic shares this start, dedup applies.
        //
        // Crucially, scanner-side dedup does NOT advance the high-water mark
        // — multiple parser errors at the same scanner-claimed position
        // (e.g. malformed `0b21010;` triggers both a `,'` expected and a `;'`
        // expected at the bad-digit position) all dedup against the same
        // scanner diag. Advancing the mark here would consume the slot and
        // leak the second parser error. We only advance the mark when
        // actually pushing a parser diagnostic (below), at which point the
        // scanner diag has been "absorbed" into our lastError tail.
        let scanner_diags = self.scanner.get_scanner_diagnostics();
        if scanner_diags.len() > self.scanner_diagnostics_high_water_mark
            && let Some(last_scanner) = scanner_diags.last()
            && self.u32_from_usize(last_scanner.pos) == start
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
        // After pushing a parser diagnostic, the effective "lastError" is
        // ours; subsequent scanner emissions reset the comparison frame.
        self.scanner_diagnostics_high_water_mark = self.scanner.get_scanner_diagnostics().len();
    }

    /// Report parse error at current token with specific error code
    fn recover_after_reserved_word_in_variable_declaration(&mut self, keyword: SyntaxKind) {
        use tsz_common::diagnostics::diagnostic_codes;

        self.next_token();

        // In tsc, `var class;` causes the variable declaration list to abort, then the
        // statement loop reparses `class` as a class declaration which expects `{` but
        // finds `;`, emitting TS1005 '{' expected.' at the semicolon. We emit this
        // directly, then consume the reserved word so the declaration parser can move on.
        if keyword == SyntaxKind::ClassKeyword && self.is_token(SyntaxKind::SemicolonToken) {
            self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
        } else if keyword == SyntaxKind::ExportKeyword && self.is_token(SyntaxKind::AsKeyword) {
            // `const export as namespace oo4;` — tsc recovers by re-parsing the
            // trailing `as namespace <id>` as the export-as-namespace syntax and
            // emits no further diagnostic beyond TS1389 on `export`.  Silently
            // consume the `as namespace <id>` tail so we don't cascade into
            // "';' expected." at `as`.
            self.next_token(); // consume `as`
            if self.is_token(SyntaxKind::NamespaceKeyword) {
                self.next_token(); // consume `namespace`
                if self.is_identifier_or_keyword() {
                    self.next_token();
                }
            }
        } else if keyword == SyntaxKind::TypeOfKeyword {
            if !self.is_expression_start() {
                // `var typeof;` → TS1109 because `;` can't start an expression.
                self.error_expression_expected();
            } else if self.is_token(SyntaxKind::OpenParenToken) {
                // `var typeof(x);` → skip the parenthesized expression to avoid extra TS1005.
                // TSC reparses `typeof(x)` as a typeof expression, consuming the operand.
                let mut paren_depth = 0u32;
                while !matches!(
                    self.token(),
                    SyntaxKind::SemicolonToken
                        | SyntaxKind::CloseBraceToken
                        | SyntaxKind::EndOfFileToken
                ) && !self.scanner.has_preceding_line_break()
                {
                    match self.token() {
                        SyntaxKind::OpenParenToken => paren_depth += 1,
                        SyntaxKind::CloseParenToken => {
                            if paren_depth == 0 {
                                break;
                            }
                            paren_depth -= 1;
                        }
                        _ => {}
                    }
                    self.next_token();
                    if paren_depth == 0 {
                        break;
                    }
                }
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

    /// Error: TS1390 - '{0}' is not allowed as a parameter name.
    ///
    /// For a few legacy keyword parameter forms, tsc also emits a companion parser
    /// diagnostic during recovery. We mirror that shape here to avoid falling through
    /// to checker-only diagnostics such as TS7006.
    pub(crate) fn error_reserved_word_in_parameter_name(&mut self) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        let keyword = self.token();
        if self.should_report_error() {
            let word = self.current_keyword_text();
            let msg = diagnostic_messages::IS_NOT_ALLOWED_AS_A_PARAMETER_NAME.replace("{0}", word);
            self.parse_error_at_current_token(
                &msg,
                diagnostic_codes::IS_NOT_ALLOWED_AS_A_PARAMETER_NAME,
            );
        }

        // Consume the reserved word so companion recovery diagnostics are anchored at
        // the following token position (matching tsc's reserved-parameter recovery).
        self.next_token();

        // Match tsc recovery for common reserved parameter names:
        //   enum/function -> TS1003 at the following token (typically ')')
        //   class         -> TS1005 "'{' expected." at the following token
        //   while/for     -> TS1005 "'(' expected." at the following token
        match keyword {
            SyntaxKind::EnumKeyword | SyntaxKind::FunctionKeyword => {
                self.parse_error_at_current_token(
                    "Identifier expected.",
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
            SyntaxKind::ClassKeyword => {
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
            }
            SyntaxKind::WhileKeyword | SyntaxKind::ForKeyword => {
                self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            }
            _ => {}
        }
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
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                diagnostic_codes::INVALID_CHARACTER,
            );
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
        self.parse_companion_error_at(
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
            // emit a plain TS1005 ";' expected" via parse_error_at which
            // deduplicates by exact start position (matching tsc's
            // parseErrorAtCurrentToken). We emit TS1005 even when the expression
            // had prior errors (like TS1121 for octal literals), matching tsc
            // behavior for cases like `00.5;` where both errors should be reported.
            // Suppress cascading TS1005 when a recent error was emitted nearby —
            // except when the prior error was a leading-zero diagnostic
            // (TS1121/TS1489) at a different position. Those are orthogonal to
            // the missing-semicolon error: tsc's `parseErrorAtPosition` dedups
            // only by exact start, so `00.5;` reports TS1121 at col 1 AND
            // TS1005 at col 3.
            if self.should_report_error() || self.last_error_was_leading_zero_at_other_pos() {
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
}

#[cfg(test)]
mod tests {
    use super::ParserState;

    #[test]
    fn u32_from_usize_clamps_overflow_without_panicking() {
        let parser = ParserState::new("a.ts".to_string(), String::new());

        assert_eq!(parser.u32_from_usize(usize::MAX), u32::MAX);
        assert!(parser.reported_offset_overflow.get());
    }

    #[test]
    fn u16_from_node_flags_truncates_overflow_without_panicking() {
        let parser = ParserState::new("a.ts".to_string(), String::new());

        assert_eq!(parser.u16_from_node_flags(0x1_0001), 1);
        assert!(parser.reported_node_flag_overflow.get());
    }

    #[test]
    fn reset_clears_conversion_overflow_markers() {
        let mut parser = ParserState::new("a.ts".to_string(), String::new());
        let _ = parser.u32_from_usize(usize::MAX);
        let _ = parser.u16_from_node_flags(0x1_0001);

        assert!(parser.reported_offset_overflow.get());
        assert!(parser.reported_node_flag_overflow.get());

        parser.reset("b.ts".to_string(), String::new());

        assert!(!parser.reported_offset_overflow.get());
        assert!(!parser.reported_node_flag_overflow.get());
    }
}

// Integration tests for parse_error_for_missing_semicolon_after live in
// parser/tests/spelling_integration_tests.rs.  Pure spelling-logic tests
// live in parser/spelling.rs.
