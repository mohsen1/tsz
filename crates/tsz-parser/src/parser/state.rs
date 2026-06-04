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

/// Context flag: parsing an `if` condition header.
pub const CONTEXT_FLAG_IF_CONDITION: u32 = 2097152;

/// Context flag: parsing parameters of a recovered class member named `if`.
pub const CONTEXT_FLAG_RECOVERED_IF_CLASS_MEMBER_PARAMETERS: u32 = 4194304;

/// A parse-time diagnostic (error or warning).
#[derive(Clone, Debug)]
pub struct ParseDiagnostic {
    pub start: u32,
    pub length: u32,
    pub message: String,
    pub code: u32,
}

impl ParseDiagnostic {
    /// Canonical ordering for parse diagnostics, mirroring the TypeScript
    /// compiler's `compareDiagnostics`. All parse diagnostics for one parse
    /// belong to the same source file, so the file key is constant and the
    /// order is fully determined by `(start, length, code, message)`. This is a
    /// total order over the observable fields, so diagnostics that tie on
    /// position still have a stable, reproducible order independent of the
    /// scanner/parser merge order in which they were produced.
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        self.start
            .cmp(&other.start)
            .then_with(|| self.length.cmp(&other.length))
            .then_with(|| self.code.cmp(&other.code))
            .then_with(|| self.message.cmp(&other.message))
    }
}

pub struct IncrementalParseResult {
    pub statements: NodeList,
    pub end_pos: u32,
    pub end_of_file_token: NodeIndex,
    pub reparse_start: u32,
}

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
    /// A malformed object method used `=>` where a body/return annotation should
    /// appear. Abort the object-literal member list so the return-token tail is
    /// recovered as ordinary statements.
    pub(crate) abort_object_literal_recovery_once: bool,
    /// An object literal aborted because a template literal appeared where a
    /// property name was expected (a template literal used as a key). The
    /// object closes at the template, the template becomes a tagged-template
    /// tail, and the variable-declaration-list recovery should treat a
    /// following `:` as a missing comma between declarators rather than a type
    /// annotation.
    pub(crate) recovered_template_literal_property_in_object: bool,
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
    /// Function declarations recover hard reserved parameter-name keywords by
    /// leaving the keyword in the token stream for statement-level recovery.
    pub(crate) recover_reserved_parameter_as_statement_tail_allowed: bool,
    /// Set when the current function declaration parameter list yielded a hard
    /// reserved keyword back to the statement parser.
    pub(crate) reserved_parameter_yielded_to_statement: bool,
}

include!("state_parts/part1.rs");
include!("state_parts/part2.rs");

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
