//! Helper methods for assignability error reporting.
//! Extracted from `assignability.rs` for maintainability.

use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::state::{CheckerState, MemberAccessLevel};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Report a type not assignable error with detailed elaboration.
    ///
    /// This method uses the solver's "explain" API to determine WHY the types
    /// are incompatible (e.g., missing property, incompatible property types,
    /// etc.) and produces a richer diagnostic with that information.
    ///
    /// **Architecture Note**: This follows the "Check Fast, Explain Slow" pattern.
    /// The `is_assignable_to` check is fast (boolean). This explain call is slower
    /// but produces better error messages. Only call this after a failed check.
    pub fn error_type_not_assignable_with_reason_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        self.diagnose_assignment_failure(source, target, idx);
    }

    /// Report a type not assignable error with detailed elaboration, preserving
    /// the provided anchor exactly instead of walking to an assignment anchor.
    pub fn error_type_not_assignable_with_reason_at_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
    }

    /// Report constructor accessibility mismatch error.
    pub(crate) fn error_constructor_accessibility_not_assignable(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_level: Option<MemberAccessLevel>,
        target_level: Option<MemberAccessLevel>,
        idx: NodeIndex,
    ) {
        let Some(loc) = self.get_source_location(idx) else {
            return;
        };

        let source_type = self.format_type_diagnostic(source);
        let target_type = self.format_type_diagnostic(target);
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_type, &target_type],
        );
        let detail = format!(
            "Cannot assign a '{}' constructor type to a '{}' constructor type.",
            Self::constructor_access_name(source_level),
            Self::constructor_access_name(target_level),
        );

        let diag = Diagnostic::error(
            self.ctx.file_name.clone(),
            loc.start,
            loc.length(),
            message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        )
        .with_related(self.ctx.file_name.clone(), loc.start, loc.length(), detail);
        self.ctx.push_diagnostic(diag);
    }

    /// Check if the diagnostic anchor node traces back to an assignment target
    /// whose variable declaration has an intersection type annotation.
    ///
    /// For `y = a;` where `y: { a: string } & { b: string }`:
    ///   anchor (`ExpressionStatement`) → expression (`BinaryExpression`) → left (Identifier)
    ///   → symbol → `value_declaration` (`VariableDeclaration`) → `type_annotation` (`IntersectionType`)
    pub(super) fn anchor_target_has_intersection_annotation(&self, anchor_idx: NodeIndex) -> bool {
        self.anchor_target_intersection_check_inner(anchor_idx)
            .unwrap_or(false)
    }

    /// Inner helper returning `Option` so we can use `?` for early returns.
    fn anchor_target_intersection_check_inner(&self, anchor_idx: NodeIndex) -> Option<bool> {
        use tsz_parser::parser::syntax_kind_ext;

        let anchor_node = self.ctx.arena.get(anchor_idx)?;

        // Walk from anchor to the assignment target identifier
        let target_ident_idx = if anchor_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            let expr_stmt = self.ctx.arena.get_expression_statement(anchor_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                binary.left
            } else {
                return Some(false);
            }
        } else {
            return Some(false);
        };

        // Check if the target is an identifier
        let ident_node = self.ctx.arena.get(target_ident_idx)?;
        if ident_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return Some(false);
        }

        // Resolve identifier to symbol
        let sym_id = self.resolve_identifier_symbol(target_ident_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Get value declaration
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;

        // Check if it's a variable declaration with an intersection type annotation
        if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
            if var_decl.type_annotation.is_some() {
                let type_node = self.ctx.arena.get(var_decl.type_annotation)?;
                return Some(type_node.kind == syntax_kind_ext::INTERSECTION_TYPE);
            }
        }

        Some(false)
    }

    pub(super) fn missing_required_properties_from_index_signature_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<Vec<tsz_common::interner::Atom>> {
        use tsz_solver::objects::index_signatures::{IndexKind, IndexSignatureResolver};

        if tsz_solver::type_queries::is_type_parameter_like(self.ctx.types, source) {
            return None;
        }

        let source_env_evaluated = self.evaluate_type_with_env(source);
        let source_evaluated = self.evaluate_type_for_assignability(source);
        let target_env_evaluated = self.evaluate_type_with_env(target);
        let target_evaluated = self.evaluate_type_for_assignability(target);

        let resolver = IndexSignatureResolver::new(self.ctx.types);
        let source_has_index = [source, source_env_evaluated, source_evaluated]
            .into_iter()
            .any(|candidate| {
                resolver.has_index_signature(candidate, IndexKind::String)
                    || resolver.has_index_signature(candidate, IndexKind::Number)
            });
        if !source_has_index {
            return None;
        }

        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let judged = self.judge_evaluate(resolved);
            [
                direct,
                resolved,
                judged,
                target_env_evaluated,
                target_evaluated,
            ]
            .into_iter()
            .find(|candidate| {
                tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
            })?
        };

        let source_shape = {
            let direct = source;
            let resolved = self.resolve_type_for_property_access(direct);
            let judged = self.judge_evaluate(resolved);
            [
                direct,
                resolved,
                judged,
                source_env_evaluated,
                source_evaluated,
            ]
            .into_iter()
            .find_map(|candidate| {
                tsz_solver::type_queries::get_object_shape(self.ctx.types, candidate)
            })
        };
        let target_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_with_shape)?;

        if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
            return None;
        }

        let mut missing: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|prop| !prop.optional)
            .filter(|prop| {
                !source_shape.as_ref().is_some_and(|shape| {
                    shape
                        .properties
                        .iter()
                        .any(|source_prop| source_prop.name == prop.name)
                })
            })
            .map(|prop| prop.name)
            .collect();
        missing.sort_by(|left, right| {
            self.ctx
                .types
                .resolve_atom_ref(*left)
                .cmp(&self.ctx.types.resolve_atom_ref(*right))
        });

        (!missing.is_empty()).then_some(missing)
    }

    /// TS2820: Try to build a "Did you mean" diagnostic for string literal mismatches.
    ///
    /// When the source is a string literal and the target is (or contains) a union of
    /// string literals with a close Levenshtein match, returns a TS2820 diagnostic
    /// instead of the generic TS2322.
    pub(super) fn try_string_literal_suggestion_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
        start: u32,
        length: u32,
    ) -> Option<Diagnostic> {
        let source_str = if let Some(source_atom) =
            tsz_solver::type_queries::get_string_literal_value(self.ctx.types, source)
        {
            self.ctx.types.resolve_atom_ref(source_atom).to_string()
        } else {
            self.get_string_literal_from_ast(anchor_idx)?
        };
        let suggestion = self.find_string_literal_suggestion(&source_str, target)?;
        let src_display = format!("\"{source_str}\"");
        let tgt_display = self.format_type_diagnostic(target);
        let suggestion_quoted = format!("\"{suggestion}\"");
        let message = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
            &[&src_display, &tgt_display, &suggestion_quoted],
        );
        Some(Diagnostic::error(
            self.ctx.file_name.clone(),
            start,
            length,
            message,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
        ))
    }

    /// Extract a string literal value from an AST node, if it is a string literal
    /// or contains one as a direct initializer.
    pub(super) fn get_string_literal_from_ast(&self, idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let prop = self.ctx.arena.get_property_assignment(node)?;
                self.get_string_literal_from_ast(prop.initializer)
            }
            k if k == tsz_scanner::SyntaxKind::Identifier as u16 => {
                if let Some(ext) = self.ctx.arena.get_extended(idx)
                    && let Some(parent) = self.ctx.arena.get(ext.parent)
                    && parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    && let Some(prop) = self.ctx.arena.get_property_assignment(parent)
                {
                    return self.get_string_literal_from_ast(prop.initializer);
                }
                None
            }
            _ => None,
        }
    }
}
