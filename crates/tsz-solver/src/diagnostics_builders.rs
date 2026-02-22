//! Diagnostic builder types for constructing formatted error messages.
//!
//! This module contains the eagerly-rendered diagnostic builders that format
//! human-readable error strings using `TypeFormatter`. These are consumed by
//! the checker for user-facing output.
//!
//! - [`DiagnosticBuilder`]: Core builder that formats type names into messages
//! - [`SpannedDiagnosticBuilder`]: Wraps `DiagnosticBuilder` with source spans
//! - [`DiagnosticCollector`]: Accumulates diagnostics with source tracking
//! - [`SourceLocation`]: Tracks source positions for AST nodes

use crate::TypeDatabase;
use crate::def::DefinitionStore;
use crate::diagnostics::{DiagnosticSeverity, SourceSpan, TypeDiagnostic, codes};
use crate::format::TypeFormatter;
use crate::types::TypeId;
use std::sync::Arc;

// =============================================================================
// Diagnostic Builder
// =============================================================================

/// Builder for creating type error diagnostics.
pub struct DiagnosticBuilder<'a> {
    formatter: TypeFormatter<'a>,
}

impl<'a> DiagnosticBuilder<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        DiagnosticBuilder {
            formatter: TypeFormatter::new(interner),
        }
    }

    /// Create a diagnostic builder with access to symbol names.
    ///
    /// This prevents "Ref(N)" fallback strings in diagnostic messages by
    /// resolving symbol references to their actual names.
    pub fn with_symbols(
        interner: &'a dyn TypeDatabase,
        symbol_arena: &'a tsz_binder::SymbolArena,
    ) -> Self {
        DiagnosticBuilder {
            formatter: TypeFormatter::with_symbols(interner, symbol_arena),
        }
    }

    /// Create a diagnostic builder with access to definition store.
    ///
    /// This prevents "Lazy(N)" fallback strings in diagnostic messages by
    /// resolving `DefIds` to their type names.
    pub fn with_def_store(mut self, def_store: &'a DefinitionStore) -> Self {
        self.formatter = self.formatter.with_def_store(def_store);
        self
    }

    /// Create a "Type X is not assignable to type Y" diagnostic.
    pub fn type_not_assignable(&mut self, source: TypeId, target: TypeId) -> TypeDiagnostic {
        let source_str = self.formatter.format(source);
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!("Type '{source_str}' is not assignable to type '{target_str}'."),
            codes::TYPE_NOT_ASSIGNABLE,
        )
    }

    /// Create a "Property X is missing in type Y" diagnostic.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
    ) -> TypeDiagnostic {
        let source_str = self.formatter.format(source);
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!(
                "Property '{prop_name}' is missing in type '{source_str}' but required in type '{target_str}'."
            ),
            codes::PROPERTY_MISSING,
        )
    }

    /// Create a "Property X does not exist on type Y" diagnostic.
    pub fn property_not_exist(&mut self, prop_name: &str, type_id: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(type_id);
        TypeDiagnostic::error(
            format!("Property '{prop_name}' does not exist on type '{type_str}'."),
            codes::PROPERTY_NOT_EXIST,
        )
    }

    /// Create a "Property X does not exist on type Y. Did you mean Z?" diagnostic (TS2551).
    pub fn property_not_exist_did_you_mean(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        suggestion: &str,
    ) -> TypeDiagnostic {
        let type_str = self.formatter.format(type_id);
        TypeDiagnostic::error(
            format!(
                "Property '{prop_name}' does not exist on type '{type_str}'. Did you mean '{suggestion}'?"
            ),
            codes::PROPERTY_NOT_EXIST_DID_YOU_MEAN,
        )
    }

    /// Create an "Argument not assignable" diagnostic.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
    ) -> TypeDiagnostic {
        let arg_str = self.formatter.format(arg_type);
        let param_str = self.formatter.format(param_type);
        TypeDiagnostic::error(
            format!(
                "Argument of type '{arg_str}' is not assignable to parameter of type '{param_str}'."
            ),
            codes::ARG_NOT_ASSIGNABLE,
        )
    }

    /// Create a "Cannot find name" diagnostic.
    pub fn cannot_find_name(&mut self, name: &str) -> TypeDiagnostic {
        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors (e.g., ",", ";", "(") that were
        // added to the AST for error recovery. The parse error should have
        // already been emitted (e.g., TS1136 "Property assignment expected").
        let is_obviously_invalid = name.len() == 1
            && matches!(
                name.chars().next(),
                Some(
                    ',' | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '&'
                        | '|'
                        | '^'
                        | '!'
                        | '~'
                        | '<'
                        | '>'
                        | '='
                        | '.'
                )
            );

        if is_obviously_invalid {
            // Return a dummy diagnostic with empty message that will be ignored
            return TypeDiagnostic::error("", 0);
        }

        let code = crate::diagnostics::cannot_find_name_code(name);
        TypeDiagnostic::error(format!("Cannot find name '{name}'."), code)
    }

    /// Create a "Type X is not callable" diagnostic.
    pub fn not_callable(&mut self, type_id: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(type_id);
        TypeDiagnostic::error(
            format!("Type '{type_str}' has no call signatures."),
            codes::NOT_CALLABLE,
        )
    }

    pub fn this_type_mismatch(
        &mut self,
        expected_this: TypeId,
        actual_this: TypeId,
    ) -> TypeDiagnostic {
        let expected_str = self.formatter.format(expected_this);
        let actual_str = self.formatter.format(actual_this);
        TypeDiagnostic::error(
            format!(
                "The 'this' context of type '{actual_str}' is not assignable to method's 'this' of type '{expected_str}'."
            ),
            codes::THIS_TYPE_MISMATCH,
        )
    }

    /// Create an "Expected N arguments but got M" diagnostic.
    pub fn argument_count_mismatch(&mut self, expected: usize, got: usize) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Expected {expected} arguments, but got {got}."),
            codes::ARG_COUNT_MISMATCH,
        )
    }

    /// Create a "Cannot assign to readonly property" diagnostic.
    pub fn readonly_property(&mut self, prop_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Cannot assign to '{prop_name}' because it is a read-only property."),
            codes::READONLY_PROPERTY,
        )
    }

    /// Create an "Excess property" diagnostic.
    pub fn excess_property(&mut self, prop_name: &str, target: TypeId) -> TypeDiagnostic {
        let target_str = self.formatter.format(target);
        TypeDiagnostic::error(
            format!(
                "Object literal may only specify known properties, and '{prop_name}' does not exist in type '{target_str}'."
            ),
            codes::EXCESS_PROPERTY,
        )
    }

    // =========================================================================
    // Implicit Any Diagnostics (TS7006, TS7008, TS7010, TS7011)
    // =========================================================================

    /// Create a "Parameter implicitly has an 'any' type" diagnostic (TS7006).
    pub fn implicit_any_parameter(&mut self, param_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Parameter '{param_name}' implicitly has an 'any' type."),
            codes::IMPLICIT_ANY_PARAMETER,
        )
    }

    /// Create a "Parameter implicitly has a specific type" diagnostic (TS7006 variant).
    pub fn implicit_any_parameter_with_type(
        &mut self,
        param_name: &str,
        implicit_type: TypeId,
    ) -> TypeDiagnostic {
        let type_str = self.formatter.format(implicit_type);
        TypeDiagnostic::error(
            format!("Parameter '{param_name}' implicitly has an '{type_str}' type."),
            codes::IMPLICIT_ANY_PARAMETER,
        )
    }

    /// Create a "Member implicitly has an 'any' type" diagnostic (TS7008).
    pub fn implicit_any_member(&mut self, member_name: &str) -> TypeDiagnostic {
        TypeDiagnostic::error(
            format!("Member '{member_name}' implicitly has an 'any' type."),
            codes::IMPLICIT_ANY_MEMBER,
        )
    }

    /// Create a "Variable implicitly has an 'any' type" diagnostic (TS7005).
    pub fn implicit_any_variable(&mut self, var_name: &str, var_type: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(var_type);
        TypeDiagnostic::error(
            format!("Variable '{var_name}' implicitly has an '{type_str}' type."),
            codes::IMPLICIT_ANY,
        )
    }

    /// Create an "implicitly has an 'any' return type" diagnostic (TS7010).
    pub fn implicit_any_return(&mut self, func_name: &str, return_type: TypeId) -> TypeDiagnostic {
        let type_str = self.formatter.format(return_type);
        TypeDiagnostic::error(
            format!(
                "'{func_name}', which lacks return-type annotation, implicitly has an '{type_str}' return type."
            ),
            codes::IMPLICIT_ANY_RETURN,
        )
    }

    /// Create a "Function expression implicitly has an 'any' return type" diagnostic (TS7011).
    pub fn implicit_any_return_function_expression(
        &mut self,
        return_type: TypeId,
    ) -> TypeDiagnostic {
        let type_str = self.formatter.format(return_type);
        TypeDiagnostic::error(
            format!(
                "Function expression, which lacks return-type annotation, implicitly has an '{type_str}' return type."
            ),
            codes::IMPLICIT_ANY_RETURN_FUNCTION_EXPRESSION,
        )
    }
}

// =============================================================================
// Spanned Diagnostic Builder
// =============================================================================

/// A diagnostic builder that automatically attaches source spans.
///
/// This builder wraps `DiagnosticBuilder` and requires a file name and
/// position information for each diagnostic.
pub struct SpannedDiagnosticBuilder<'a> {
    builder: DiagnosticBuilder<'a>,
    file: Arc<str>,
}

impl<'a> SpannedDiagnosticBuilder<'a> {
    pub fn new(interner: &'a dyn TypeDatabase, file: impl Into<Arc<str>>) -> Self {
        SpannedDiagnosticBuilder {
            builder: DiagnosticBuilder::new(interner),
            file: file.into(),
        }
    }

    /// Create a spanned diagnostic builder with access to symbol names.
    pub fn with_symbols(
        interner: &'a dyn TypeDatabase,
        symbol_arena: &'a tsz_binder::SymbolArena,
        file: impl Into<Arc<str>>,
    ) -> Self {
        SpannedDiagnosticBuilder {
            builder: DiagnosticBuilder::with_symbols(interner, symbol_arena),
            file: file.into(),
        }
    }

    /// Add access to definition store for `DefId` name resolution.
    pub fn with_def_store(mut self, def_store: &'a DefinitionStore) -> Self {
        self.builder = self.builder.with_def_store(def_store);
        self
    }

    /// Create a span for this file.
    pub fn span(&self, start: u32, length: u32) -> SourceSpan {
        SourceSpan::new(std::sync::Arc::clone(&self.file), start, length)
    }

    /// Create a "Type X is not assignable to type Y" diagnostic with span.
    pub fn type_not_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .type_not_assignable(source, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Property X is missing" diagnostic with span.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .property_missing(prop_name, source, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Property X does not exist" diagnostic with span.
    pub fn property_not_exist(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .property_not_exist(prop_name, type_id)
            .with_span(self.span(start, length))
    }

    /// Create a "Property X does not exist on type Y. Did you mean Z?" diagnostic with span (TS2551).
    pub fn property_not_exist_did_you_mean(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        suggestion: &str,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .property_not_exist_did_you_mean(prop_name, type_id, suggestion)
            .with_span(self.span(start, length))
    }

    /// Create an "Argument not assignable" diagnostic with span.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .argument_not_assignable(arg_type, param_type)
            .with_span(self.span(start, length))
    }

    /// Create a "Cannot find name" diagnostic with span.
    pub fn cannot_find_name(&mut self, name: &str, start: u32, length: u32) -> TypeDiagnostic {
        self.builder
            .cannot_find_name(name)
            .with_span(self.span(start, length))
    }

    /// Create an "Expected N arguments" diagnostic with span.
    pub fn argument_count_mismatch(
        &mut self,
        expected: usize,
        got: usize,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .argument_count_mismatch(expected, got)
            .with_span(self.span(start, length))
    }

    /// Create a "Type is not callable" diagnostic with span.
    pub fn not_callable(&mut self, type_id: TypeId, start: u32, length: u32) -> TypeDiagnostic {
        self.builder
            .not_callable(type_id)
            .with_span(self.span(start, length))
    }

    pub fn this_type_mismatch(
        &mut self,
        expected_this: TypeId,
        actual_this: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .this_type_mismatch(expected_this, actual_this)
            .with_span(self.span(start, length))
    }

    /// Create an "Excess property" diagnostic with span.
    pub fn excess_property(
        &mut self,
        prop_name: &str,
        target: TypeId,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .excess_property(prop_name, target)
            .with_span(self.span(start, length))
    }

    /// Create a "Cannot assign to readonly property" diagnostic with span.
    pub fn readonly_property(
        &mut self,
        prop_name: &str,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .readonly_property(prop_name)
            .with_span(self.span(start, length))
    }

    /// Add a related location to an existing diagnostic.
    pub fn add_related(
        &self,
        diag: TypeDiagnostic,
        message: impl Into<String>,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        diag.with_related(self.span(start, length), message)
    }
}

// =============================================================================
// Diagnostic Conversion
// =============================================================================

/// Convert a solver `TypeDiagnostic` to a checker Diagnostic.
///
/// This allows the solver's diagnostic infrastructure to integrate
/// with the existing checker diagnostic system.
impl TypeDiagnostic {
    /// Convert to a `checker::Diagnostic`.
    ///
    /// Uses the provided `file_name` if no span is present.
    pub fn to_checker_diagnostic(&self, default_file: &str) -> tsz_common::diagnostics::Diagnostic {
        use tsz_common::diagnostics::{
            Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation,
        };

        let (file, start, length) = if let Some(ref span) = self.span {
            (span.file.to_string(), span.start, span.length)
        } else {
            (default_file.to_string(), 0, 0)
        };

        let category = match self.severity {
            DiagnosticSeverity::Error => DiagnosticCategory::Error,
            DiagnosticSeverity::Warning => DiagnosticCategory::Warning,
            DiagnosticSeverity::Suggestion => DiagnosticCategory::Suggestion,
            DiagnosticSeverity::Message => DiagnosticCategory::Message,
        };

        let related_information: Vec<DiagnosticRelatedInformation> = self
            .related
            .iter()
            .map(|rel| DiagnosticRelatedInformation {
                file: rel.span.file.to_string(),
                start: rel.span.start,
                length: rel.span.length,
                message_text: rel.message.clone(),
                category: DiagnosticCategory::Message,
                code: 0,
            })
            .collect();

        Diagnostic {
            file,
            start,
            length,
            message_text: self.message.clone(),
            category,
            code: self.code,
            related_information,
        }
    }
}

// =============================================================================
// Source Location Tracker
// =============================================================================

/// Tracks source locations for AST nodes during type checking.
///
/// This struct provides a convenient way to associate type checking
/// operations with their source locations for diagnostic generation.
#[derive(Clone)]
pub struct SourceLocation {
    /// File name
    pub file: Arc<str>,
    /// Start position (byte offset)
    pub start: u32,
    /// End position (byte offset)
    pub end: u32,
}

impl SourceLocation {
    pub fn new(file: impl Into<Arc<str>>, start: u32, end: u32) -> Self {
        Self {
            file: file.into(),
            start,
            end,
        }
    }

    /// Get the length of this location.
    pub const fn length(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Convert to a `SourceSpan`.
    pub fn to_span(&self) -> SourceSpan {
        SourceSpan::new(std::sync::Arc::clone(&self.file), self.start, self.length())
    }
}

/// A diagnostic collector that accumulates diagnostics with source tracking.
pub struct DiagnosticCollector<'a> {
    interner: &'a dyn TypeDatabase,
    file: Arc<str>,
    diagnostics: Vec<TypeDiagnostic>,
}

impl<'a> DiagnosticCollector<'a> {
    pub fn new(interner: &'a dyn TypeDatabase, file: impl Into<Arc<str>>) -> Self {
        DiagnosticCollector {
            interner,
            file: file.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Get the collected diagnostics.
    pub fn diagnostics(&self) -> &[TypeDiagnostic] {
        &self.diagnostics
    }

    /// Take the collected diagnostics.
    pub fn take_diagnostics(&mut self) -> Vec<TypeDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Report a type not assignable error.
    pub fn type_not_assignable(&mut self, source: TypeId, target: TypeId, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .type_not_assignable(source, target)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a property missing error.
    pub fn property_missing(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        loc: &SourceLocation,
    ) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .property_missing(prop_name, source, target)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a property not exist error.
    pub fn property_not_exist(&mut self, prop_name: &str, type_id: TypeId, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .property_not_exist(prop_name, type_id)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report an argument not assignable error.
    pub fn argument_not_assignable(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        loc: &SourceLocation,
    ) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .argument_not_assignable(arg_type, param_type)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report a cannot find name error.
    pub fn cannot_find_name(&mut self, name: &str, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder.cannot_find_name(name).with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Report an argument count mismatch error.
    pub fn argument_count_mismatch(&mut self, expected: usize, got: usize, loc: &SourceLocation) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .argument_count_mismatch(expected, got)
            .with_span(loc.to_span());
        self.diagnostics.push(diag);
    }

    /// Convert all collected diagnostics to checker diagnostics.
    pub fn to_checker_diagnostics(&self) -> Vec<tsz_common::diagnostics::Diagnostic> {
        self.diagnostics
            .iter()
            .map(|d| d.to_checker_diagnostic(&self.file))
            .collect()
    }
}
