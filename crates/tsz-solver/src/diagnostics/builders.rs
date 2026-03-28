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

use super::format::TypeFormatter;
use crate::TypeDatabase;
use crate::def::DefinitionStore;
use crate::diagnostics::{DiagnosticSeverity, SourceSpan, TypeDiagnostic, codes};
use crate::types::TypeId;
use std::sync::Arc;

// =============================================================================
// Diagnostic Builder
// =============================================================================

/// Builder for creating type error diagnostics.
pub struct DiagnosticBuilder<'a> {
    interner: &'a dyn TypeDatabase,
    formatter: TypeFormatter<'a>,
}

impl<'a> DiagnosticBuilder<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        DiagnosticBuilder {
            interner,
            formatter: TypeFormatter::new(interner).with_diagnostic_mode(),
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
            interner,
            formatter: TypeFormatter::with_symbols(interner, symbol_arena).with_diagnostic_mode(),
        }
    }

    fn materialize_finite_mapped_type_for_display(&self, ty: TypeId) -> Option<TypeId> {
        match self.interner.lookup(ty) {
            Some(crate::types::TypeData::Mapped(mapped_id)) => {
                let mapped = self.interner.mapped_type(mapped_id);
                let names = crate::type_queries::collect_finite_mapped_property_names(
                    self.interner,
                    mapped_id,
                )?;
                let mut names: Vec<_> = names.into_iter().collect();
                names.sort_by(|a, b| {
                    self.interner
                        .resolve_atom_ref(*a)
                        .cmp(&self.interner.resolve_atom_ref(*b))
                });

                let mut properties = Vec::with_capacity(names.len());
                for name in names {
                    let property_name = self.interner.resolve_atom_ref(name).to_string();
                    let type_id = crate::type_queries::get_finite_mapped_property_type(
                        self.interner,
                        mapped_id,
                        &property_name,
                    )?;
                    let type_id = self.normalize_excess_display_type(type_id);
                    let mut property = crate::PropertyInfo::new(name, type_id);
                    property.optional =
                        mapped.optional_modifier == Some(crate::MappedModifier::Add);
                    property.readonly =
                        mapped.readonly_modifier == Some(crate::MappedModifier::Add);
                    properties.push(property);
                }

                Some(self.interner.object(properties))
            }
            Some(crate::types::TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                let mut changed = false;
                let remapped: Vec<_> = members
                    .iter()
                    .map(|&member| {
                        if let Some(materialized) =
                            self.materialize_finite_mapped_type_for_display(member)
                        {
                            changed = true;
                            materialized
                        } else {
                            member
                        }
                    })
                    .collect();
                changed.then(|| self.interner.intersection(remapped))
            }
            Some(crate::types::TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                let mut changed = false;
                let remapped: Vec<_> = members
                    .iter()
                    .map(|&member| {
                        if let Some(materialized) =
                            self.materialize_finite_mapped_type_for_display(member)
                        {
                            changed = true;
                            materialized
                        } else {
                            member
                        }
                    })
                    .collect();
                changed.then(|| self.interner.union(remapped))
            }
            _ => None,
        }
    }

    fn normalize_excess_display_type(&self, ty: TypeId) -> TypeId {
        let ty = crate::evaluate_type(self.interner, ty);
        match self.interner.lookup(ty) {
            Some(crate::types::TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                let args: Vec<_> = app
                    .args
                    .iter()
                    .map(|&arg| self.normalize_excess_display_type(arg))
                    .collect();
                if args == app.args {
                    ty
                } else {
                    self.interner.application(app.base, args)
                }
            }
            Some(crate::types::TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                let params: Vec<_> = shape
                    .params
                    .iter()
                    .map(|param| crate::ParamInfo {
                        type_id: self.normalize_excess_display_type(param.type_id),
                        ..*param
                    })
                    .collect();
                let return_type = self.normalize_excess_display_type(shape.return_type);
                if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                    && return_type == shape.return_type
                {
                    ty
                } else {
                    self.interner.function(crate::FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type: shape.this_type,
                        return_type,
                        type_predicate: shape.type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
                }
            }
            Some(crate::types::TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                self.interner.union_preserve_members(
                    members
                        .iter()
                        .map(|&member| self.normalize_excess_display_type(member))
                        .collect(),
                )
            }
            Some(crate::types::TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                self.interner.intersection(
                    members
                        .iter()
                        .map(|&member| self.normalize_excess_display_type(member))
                        .collect(),
                )
            }
            _ => ty,
        }
    }

    fn split_optional_object_for_excess_display(&self, ty: TypeId) -> TypeId {
        let ty = crate::evaluate_type(self.interner, ty);
        if let Some(crate::types::TypeData::Union(list_id)) = self.interner.lookup(ty) {
            let members = self.interner.type_list(list_id);
            let non_undefined: Vec<_> = members
                .iter()
                .copied()
                .filter(|member| *member != TypeId::UNDEFINED)
                .collect();
            if non_undefined.len() == 1 && non_undefined.len() != members.len() {
                return non_undefined[0];
            }
        }
        ty
    }

    fn split_wildcard_object_for_excess_display(&mut self, ty: TypeId) -> Option<String> {
        let ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        let ty = self.split_optional_object_for_excess_display(ty);
        let shape = crate::type_queries::get_object_shape(self.interner, ty)?;
        if shape.string_index.is_some() || shape.number_index.is_some() {
            return None;
        }

        let wildcard_name = self.interner.intern_string("*");
        let mut wildcard_props = Vec::new();
        let mut named_props = Vec::new();

        for prop in &shape.properties {
            let mut cloned = prop.clone();
            cloned.type_id = self.normalize_excess_display_type(cloned.type_id);
            cloned.write_type = self.normalize_excess_display_type(cloned.write_type);
            if cloned.name == wildcard_name {
                wildcard_props.push(cloned);
            } else {
                named_props.push(cloned);
            }
        }

        if wildcard_props.is_empty() || named_props.is_empty() {
            return None;
        }

        let named_obj = self.interner.object(named_props);
        let wildcard_obj = self.interner.object(wildcard_props);
        Some(format!(
            "{} & {}",
            self.formatter.format(named_obj),
            self.formatter.format(wildcard_obj)
        ))
    }

    fn format_excess_property_target(&mut self, target: TypeId) -> String {
        if let Some(display) = self.split_wildcard_object_for_excess_display(target) {
            return display;
        }

        if let Some(crate::types::TypeData::Intersection(list_id)) = self.interner.lookup(target) {
            let members = self.interner.type_list(list_id);
            let mut changed = false;
            let parts: Vec<String> = members
                .iter()
                .map(|&member| {
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        self.formatter.format(materialized).to_string()
                    } else {
                        self.formatter.format(member).to_string()
                    }
                })
                .collect();
            if changed {
                return parts.join(" & ");
            }
        }

        let target = self
            .materialize_finite_mapped_type_for_display(target)
            .unwrap_or(target);
        self.formatter.format(target).to_string()
    }

    /// Create a diagnostic builder with access to definition store.
    ///
    /// This prevents "Lazy(N)" fallback strings in diagnostic messages by
    /// resolving `DefIds` to their type names.
    pub fn with_def_store(mut self, def_store: &'a DefinitionStore) -> Self {
        self.formatter = self.formatter.with_def_store(def_store);
        self
    }

    /// Add namespace module name mapping for displaying module namespace types
    /// as `typeof import("module")` instead of their object shape.
    pub fn with_namespace_module_names(
        mut self,
        names: &'a rustc_hash::FxHashMap<crate::types::TypeId, String>,
    ) -> Self {
        self.formatter = self.formatter.with_namespace_module_names(names);
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

    /// Create a "This expression is not callable" diagnostic (TS2349).
    pub fn not_callable(&mut self, _type_id: TypeId) -> TypeDiagnostic {
        TypeDiagnostic::error(
            "This expression is not callable.".to_string(),
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
    /// When `expected_min < expected_max`, formats as "Expected 1-3 arguments, but got 0."
    pub fn argument_count_mismatch(
        &mut self,
        expected_min: usize,
        expected_max: usize,
        got: usize,
    ) -> TypeDiagnostic {
        let expected_str = if expected_min < expected_max {
            format!("{expected_min}-{expected_max}")
        } else {
            expected_max.to_string()
        };
        TypeDiagnostic::error(
            format!("Expected {expected_str} arguments, but got {got}."),
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
        let target_str = self.format_excess_property_target(target);
        TypeDiagnostic::error(
            format!(
                "Object literal may only specify known properties, and '{prop_name}' does not exist in type '{target_str}'."
            ),
            codes::EXCESS_PROPERTY,
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

    /// Add namespace module name mapping for displaying module namespace types.
    pub fn with_namespace_module_names(
        mut self,
        names: &'a rustc_hash::FxHashMap<crate::types::TypeId, String>,
    ) -> Self {
        self.builder = self.builder.with_namespace_module_names(names);
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
        expected_min: usize,
        expected_max: usize,
        got: usize,
        start: u32,
        length: u32,
    ) -> TypeDiagnostic {
        self.builder
            .argument_count_mismatch(expected_min, expected_max, got)
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
    pub fn argument_count_mismatch(
        &mut self,
        expected_min: usize,
        expected_max: usize,
        got: usize,
        loc: &SourceLocation,
    ) {
        let mut builder = DiagnosticBuilder::new(self.interner);
        let diag = builder
            .argument_count_mismatch(expected_min, expected_max, got)
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
