//! Type/value mismatch and declaration error reporting (TS2693, TS2749, TS2708, TS2709).

use super::TypeOnlyKind;
use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::state::CheckerState;
use tracing::trace;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Variable/Declaration Errors
    // =========================================================================

    /// Report error 2403: Subsequent variable declarations must have the same type.
    pub fn error_subsequent_variable_declaration(
        &mut self,
        name: &str,
        prev_type: TypeId,
        current_type: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress for ERROR and UNKNOWN types.
        // ERROR: avoids cascading diagnostics from unresolved types.
        // UNKNOWN: when a lib global (console, Math, etc.) can't be properly
        // typed, it resolves to `unknown`. Comparing against `unknown` always
        // fails and produces false positives. TSC's libs properly type all
        // globals, so this situation only arises from incomplete lib coverage.
        if prev_type == TypeId::ERROR
            || current_type == TypeId::ERROR
            || prev_type == TypeId::UNKNOWN
            || current_type == TypeId::UNKNOWN
        {
            return;
        }
        if let Some(loc) = self.get_source_location(idx) {
            let prev_type_str = self.format_type(prev_type);
            let current_type_str = self.format_type(current_type);
            let message = format!(
                "Subsequent variable declarations must have the same type. Variable '{name}' must be of type '{prev_type_str}', but here has type '{current_type_str}'."
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::SUBSEQUENT_VARIABLE_DECLARATIONS_MUST_HAVE_THE_SAME_TYPE_VARIABLE_MUST_BE_OF_TYP,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2454: Variable is used before being assigned.
    pub fn error_variable_used_before_assigned_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Class-Related Errors
    // =========================================================================

    /// Report error 2715: Abstract property 'X' in class 'C' cannot be accessed in the constructor.
    pub fn error_abstract_property_in_constructor(
        &mut self,
        prop_name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Abstract property '{prop_name}' in class '{class_name}' cannot be accessed in the constructor."
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::ABSTRACT_PROPERTY_IN_CLASS_CANNOT_BE_ACCESSED_IN_THE_CONSTRUCTOR,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Module/Namespace Errors
    // =========================================================================

    /// Report TS2694: Namespace has no exported member.
    pub fn error_namespace_no_export(
        &mut self,
        namespace_name: &str,
        member_name: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message =
                format!("Namespace '{namespace_name}' has no exported member '{member_name}'.");
            self.ctx.diagnostics.push(Diagnostic {
                code: 2694,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    // =========================================================================
    // Type/Value Mismatch Errors
    // =========================================================================

    /// Report TS2698: Spread types may only be created from object types.
    pub fn report_spread_not_object_type(&mut self, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES,
                category: DiagnosticCategory::Error,
                message_text:
                    diagnostic_messages::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
                        .to_string(),
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2693/TS2585: Symbol only refers to a type, but is used as a value.
    ///
    /// For ES2015+ types (Promise, Map, Set, Symbol, etc.), emits TS2585 with a suggestion
    /// to change the target library. For other types, emits TS2693 without the lib suggestion.
    pub fn error_type_only_value_at(&mut self, name: &str, idx: NodeIndex) {
        if std::env::var_os("TSZ_DEBUG_PARSER_RECOVERY").is_some() && name == "yield" {
            trace!(
                target: "tsz_debug",
                name,
                file = %self.ctx.file_name,
                idx = ?idx,
                parse_errors = self.has_parse_errors(),
                "tsz-debug: error_type_only_value_at"
            );
        }
        use tsz_binder::lib_loader;

        // Don't emit TS2693 for identifiers used as import equals module references.
        // `import r = undefined` already gets TS2503 from check_namespace_import.
        if self.ctx.arena.get_extended(idx).is_some_and(|ext| {
            self.ctx.arena.get(ext.parent).is_some_and(|p| {
                p.kind == tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            })
        }) {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            // Check if this is an ES2015+ type that requires specific lib support
            let is_es2015_type = lib_loader::is_es2015_plus_type(name);
            let allow_in_parse_recovery =
                self.has_type_only_value_in_parse_recovery_context(name, idx);

            // In syntax-error files, TS2693 often cascades from parser recovery and
            // diverges from tsc's primary-diagnostic set. Keep TS2585 behavior intact.
            // Exception: recovered primitive array type literals in value position
            // (`number[]` in expression context) should still emit TS2693.
            let allow_keyword_array_recovery = self
                .ctx
                .arena
                .source_files
                .first()
                .and_then(|sf| {
                    let start = usize::try_from(loc.start).ok()?;
                    let src = sf.text.as_ref();
                    let pattern = format!("{name}[]");
                    src.get(start..)
                        .map(|tail: &str| tail.starts_with(pattern.as_str()))
                })
                .unwrap_or(false);
            let allow_any_in_parse_recovery = name == "any";
            if self.has_parse_errors()
                && !is_es2015_type
                && !allow_keyword_array_recovery
                && !allow_any_in_parse_recovery
                && !allow_in_parse_recovery
            {
                return;
            }

            let (code, message) = if is_es2015_type {
                // TS2585: Type only refers to a type, suggest changing target library
                (
                    diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO,
                    format_message(
                        diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO,
                        &[name],
                    ),
                )
            } else if let Some(type_only_kind) = self.get_type_only_import_export_kind(idx) {
                match type_only_kind {
                    TypeOnlyKind::Import => (
                        diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE,
                        format_message(
                            diagnostic_messages::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE,
                            &[name],
                        ),
                    ),
                    TypeOnlyKind::Export => (
                        diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE,
                        format_message(
                            diagnostic_messages::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE,
                            &[name],
                        ),
                    ),
                }
            } else {
                // TS2693: Generic type-only error
                (
                    diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                    format_message(
                        diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                        &[name],
                    ),
                )
            };

            self.ctx.diagnostics.push(Diagnostic {
                code,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Determine if the identifier at `idx` resolves to a symbol that was
    /// explicitly imported with `import type` or exported with `export type`.
    /// Returns `Some(TypeOnlyKind::Import)` for TS1361 or
    /// `Some(TypeOnlyKind::Export)` for TS1362 when applicable.
    fn get_type_only_import_export_kind(&self, idx: NodeIndex) -> Option<TypeOnlyKind> {
        use tsz_binder::symbol_flags;

        let sym_id = self.resolve_identifier_symbol(idx)?;
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;

        // Only applies to alias symbols explicitly marked type-only
        if (symbol.flags & symbol_flags::ALIAS) == 0 || !symbol.is_type_only {
            return None;
        }

        // Walk up from the symbol's declaration to determine if it came from
        // an import or export statement.
        for &decl in &symbol.declarations {
            if decl.is_none() {
                continue;
            }
            let mut current = decl;
            let mut guard = 0;
            while guard < 16 {
                guard += 1;
                let node = self.ctx.arena.get(current)?;
                if node.kind == syntax_kind_ext::IMPORT_DECLARATION {
                    return Some(TypeOnlyKind::Import);
                }
                if node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                    return Some(TypeOnlyKind::Export);
                }
                let ext = self.ctx.arena.get_extended(current)?;
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }
        }

        None
    }

    /// Parser-recovery exceptions for TS2693/TS2585.
    ///
    /// Some grammar-recovery scenarios continue checking and should still emit
    /// type/value mismatch diagnostics even with parse errors.
    fn has_type_only_value_in_parse_recovery_context(&self, name: &str, idx: NodeIndex) -> bool {
        // Recovery for async-generator computed members (`async * [yield] ...`) should still
        // report TS2693.
        if name != "yield" {
            return false;
        }

        let mut guard = 0;
        let mut current = Some(idx);
        let mut seen_computed_property_name = false;

        while let Some(current_idx) = current {
            if guard > 64 {
                break;
            }
            guard += 1;

            let Some(ext) = self.ctx.arena.get_extended(current_idx) else {
                break;
            };

            let Some(parent) = self.ctx.arena.get(ext.parent) else {
                break;
            };

            if parent.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                seen_computed_property_name = true;
            } else if parent.kind == syntax_kind_ext::METHOD_DECLARATION
                && seen_computed_property_name
            {
                return true;
            }

            current = Some(ext.parent);
        }

        if std::env::var_os("TSZ_DEBUG_PARSER_RECOVERY").is_some() && seen_computed_property_name {
            trace!(
                target: "tsz_debug",
                name,
                file = %self.ctx.file_name,
                computed_context = seen_computed_property_name,
                "tsz-debug: computed property diagnostic context"
            );
        }

        false
    }

    /// Report TS2749: Symbol refers to a value, but is used as a type.
    pub fn error_value_only_type_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2709: Cannot use namespace '{0}' as a type.
    pub fn error_namespace_used_as_type_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message =
                format_message(diagnostic_messages::CANNOT_USE_NAMESPACE_AS_A_TYPE, &[name]);
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_TYPE,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2708: Cannot use namespace '{0}' as a value.
    pub fn error_namespace_used_as_value_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_USE_NAMESPACE_AS_A_VALUE,
                &[name],
            );
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::CANNOT_USE_NAMESPACE_AS_A_VALUE,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS18050: The value 'X' cannot be used here.
    /// Emitted when a value (like a variable or literal) is used where it's not permitted.
    pub fn error_value_cannot_be_used_here_at(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message =
                format_message(diagnostic_messages::THE_VALUE_CANNOT_BE_USED_HERE, &[name]);
            self.ctx.diagnostics.push(Diagnostic {
                code: diagnostic_codes::THE_VALUE_CANNOT_BE_USED_HERE,
                category: DiagnosticCategory::Error,
                message_text: message,
                start: loc.start,
                length: loc.length(),
                file: self.ctx.file_name.clone(),
                related_information: Vec::new(),
            });
        }
    }
}
