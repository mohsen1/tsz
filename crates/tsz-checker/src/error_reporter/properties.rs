//! Property-related error reporting (TS2339, TS2741, TS2540, TS7053).

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Property Errors
    // =========================================================================

    /// Report a property missing error using solver diagnostics with source tracking.
    pub fn error_property_missing_at(
        &mut self,
        prop_name: &str,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        // Suppress cascade errors from unresolved types
        if source == TypeId::ERROR
            || target == TypeId::ERROR
            || source == TypeId::ANY
            || target == TypeId::ANY
            || source == TypeId::UNKNOWN
            || target == TypeId::UNKNOWN
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.property_missing(prop_name, source, target, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a property not exist error using solver diagnostics with source tracking.
    /// If a similar property name is found on the type, emits TS2551 ("Did you mean?")
    /// instead of TS2339.
    pub fn error_property_not_exist_at(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        idx: NodeIndex,
    ) {
        use tsz_solver::type_queries;

        // Suppress error if type is ERROR/ANY or an Error type wrapper
        // This prevents cascading errors when accessing properties on error types
        // NOTE: We do NOT suppress for UNKNOWN - accessing properties on unknown should error (TS2339)
        if type_id == TypeId::ERROR
            || type_id == TypeId::ANY
            || type_queries::is_error_type(self.ctx.types, type_id)
        {
            return;
        }

        // Suppress cascaded TS2339 from failed generic inference when the receiver
        // remains a union that still contains unresolved type parameters.
        // This keeps follow-on property errors from obscuring the primary root cause
        // (typically assignability/inference diagnostics).
        if type_queries::is_union_type(self.ctx.types, type_id)
            && type_queries::contains_type_parameters_db(self.ctx.types, type_id)
        {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let suppress_did_you_mean =
                self.has_syntax_parse_errors() || self.class_extends_any_base(type_id);

            // On files with syntax parse errors, TypeScript generally avoids TS2551
            // suggestion diagnostics and sticks with TS2339 to reduce cascades.
            let suggestion = if suppress_did_you_mean {
                None
            } else {
                self.find_similar_property(prop_name, type_id)
            };

            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);

            let diag = if let Some(ref suggestion) = suggestion {
                builder.property_not_exist_did_you_mean(
                    prop_name,
                    type_id,
                    suggestion,
                    loc.start,
                    loc.length(),
                )
            } else {
                builder.property_not_exist(prop_name, type_id, loc.start, loc.length())
            };
            // Use push_diagnostic for deduplication
            self.ctx
                .push_diagnostic(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report an excess property error using solver diagnostics with source tracking.
    pub fn error_excess_property_at(&mut self, prop_name: &str, target: TypeId, idx: NodeIndex) {
        // Suppress cascade errors from unresolved types
        if target == TypeId::ERROR || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.excess_property(prop_name, target, loc.start, loc.length());
            // Use push_diagnostic for deduplication
            self.ctx
                .push_diagnostic(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a "Cannot assign to readonly property" error using solver diagnostics with source tracking.
    pub fn error_readonly_property_at(&mut self, prop_name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.readonly_property(prop_name, loc.start, loc.length());
            self.ctx
                .diagnostics
                .push(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report TS2542: Index signature in type '{0}' only permits reading.
    pub fn error_readonly_index_signature_at(
        &mut self,
        object_type: tsz_solver::TypeId,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let type_name = self.format_type(object_type);
            let message = format_message(
                diagnostic_messages::INDEX_SIGNATURE_IN_TYPE_ONLY_PERMITS_READING,
                &[&type_name],
            );
            let diag = Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::INDEX_SIGNATURE_IN_TYPE_ONLY_PERMITS_READING,
            );
            self.ctx.diagnostics.push(diag);
        }
    }

    /// Report TS2803: Cannot assign to private method. Private methods are not writable.
    pub fn error_private_method_not_writable(&mut self, prop_name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_ASSIGN_TO_PRIVATE_METHOD_PRIVATE_METHODS_ARE_NOT_WRITABLE,
                &[prop_name],
            );
            let diag = Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::CANNOT_ASSIGN_TO_PRIVATE_METHOD_PRIVATE_METHODS_ARE_NOT_WRITABLE,
            );
            self.ctx.diagnostics.push(diag);
        }
    }

    /// Report no index signature error.
    pub(crate) fn error_no_index_signature_at(
        &mut self,
        index_type: TypeId,
        object_type: TypeId,
        idx: NodeIndex,
    ) {
        // TS7053 is a noImplicitAny error - suppress without it
        if !self.ctx.no_implicit_any() {
            return;
        }
        // Suppress when types are unresolved
        if index_type == TypeId::ANY || index_type == TypeId::ERROR || index_type == TypeId::UNKNOWN
        {
            return;
        }
        if object_type == TypeId::ANY
            || object_type == TypeId::ERROR
            || object_type == TypeId::UNKNOWN
        {
            return;
        }
        if self.is_element_access_on_this_or_super_with_any_base(idx) {
            return;
        }

        if let Some(atom) =
            tsz_solver::type_queries::get_string_literal_value(self.ctx.types, index_type)
        {
            let prop_name = self.ctx.types.resolve_atom_ref(atom);
            let prop_name_str: &str = &prop_name;
            let suppress_did_you_mean =
                self.has_syntax_parse_errors() || self.class_extends_any_base(object_type);

            let suggestion = if suppress_did_you_mean {
                None
            } else {
                self.find_similar_property(prop_name_str, object_type)
            };

            if suggestion.is_some() {
                // If there's a suggestion, TypeScript emits TS2551 instead of TS7053
                self.error_property_not_exist_at(prop_name_str, object_type, idx);
                return;
            }
        }

        let mut formatter = self.ctx.create_type_formatter();
        let index_str = formatter.format(index_type);
        let object_str = formatter.format(object_type);
        let message = format!(
            "Element implicitly has an 'any' type because expression of type '{index_str}' can't be used to index type '{object_str}'."
        );

        self.error_at_node(idx, &message, diagnostic_codes::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_CANT_BE_USED_TO_IN);
    }

    /// TypeScript suppresses TS7053 for `this[...]`/`super[...]` when the class extends an `any` base.
    fn is_element_access_on_this_or_super_with_any_base(&mut self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return false;
        };
        let Some(parent) = self.ctx.arena.get(ext.parent) else {
            return false;
        };
        if parent.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(parent) else {
            return false;
        };
        if access.name_or_argument != idx {
            return false;
        }
        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        let is_this_or_super = expr_node.kind == SyntaxKind::SuperKeyword as u16
            || expr_node.kind == SyntaxKind::ThisKeyword as u16;
        if !is_this_or_super {
            return false;
        }

        let Some(class_info) = self.ctx.enclosing_class.clone() else {
            return false;
        };
        let Some(class_decl) = self.ctx.arena.get_class_at(class_info.class_idx) else {
            return false;
        };
        let Some(heritage_clauses) = &class_decl.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let Some(&type_idx) = clause.types.nodes.first() else {
                continue;
            };
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            if self.get_type_of_node(expr_idx) == TypeId::ANY {
                return true;
            }
        }

        false
    }
}
