//! Statement checking helpers: enum member value validation, namespace
//! prototype conflict detection, and declaration serialization checks.
//!
//! Extracted from `statement_callback_bridge.rs` to keep module size manageable.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn maybe_report_non_serializable_inferred_declaration_type(
        &mut self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
        name: &str,
        inferred_type: TypeId,
    ) {
        if !self.ctx.emit_declarations() || self.ctx.is_declaration_file() || name.is_empty() {
            return;
        }
        if !self.is_declaration_type_emitted_without_annotation(decl_idx) {
            return;
        }
        if !crate::query_boundaries::state::type_environment::declaration_type_references_cyclic_structure(
            self,
            inferred_type,
        ) {
            return;
        }

        self.error_at_node(
            name_idx,
            &format!(
                "The inferred type of '{name}' references a type with a cyclic structure which cannot be trivially serialized. A type annotation is necessary."
            ),
            5088,
        );
    }

    pub(super) fn maybe_report_exported_function_anonymous_class_return_private_members(
        &mut self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
        name: &str,
        return_type: TypeId,
    ) {
        if !self.ctx.emit_declarations() || self.ctx.is_declaration_file() || name.is_empty() {
            return;
        }
        if !self.is_declaration_type_emitted_without_annotation(decl_idx) {
            return;
        }
        let Some(instance_type) = self.instance_type_from_constructor_type(return_type) else {
            return;
        };
        if self.instance_type_is_from_anonymous_class(instance_type) {
            self.report_instance_type_private_members_as_ts4094(name_idx, instance_type);
        }
    }

    fn is_declaration_type_emitted_without_annotation(&self, decl_idx: NodeIndex) -> bool {
        let parent_kind = self
            .ctx
            .arena
            .get_extended(decl_idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent))
            .map(|parent| parent.kind);

        match parent_kind {
            Some(kind) if kind == syntax_kind_ext::SOURCE_FILE => {
                !self.ctx.binder.is_external_module()
                    || self.is_declaration_exported(self.ctx.arena, decl_idx)
            }
            Some(kind) if kind == syntax_kind_ext::EXPORT_DECLARATION => true,
            Some(kind) if kind == syntax_kind_ext::MODULE_BLOCK => {
                self.is_declaration_exported(self.ctx.arena, decl_idx)
            }
            _ => false,
        }
    }
}

impl<'a> CheckerState<'a> {
    /// Check if a namespace/module declaration contains any value declarations
    /// (const, let, var, function, class, enum) as opposed to only types.
    pub(super) fn namespace_has_value_declarations(&self, module_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(module_idx) else {
            return false;
        };
        let Some(module) = self.ctx.arena.get_module(node) else {
            return false;
        };
        if module.body.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(module.body) else {
            return false;
        };
        // Namespace bodies are ModuleBlock, not Block
        let stmts = if let Some(module_block) = self.ctx.arena.get_module_block(body_node) {
            module_block.statements.as_ref().map(|s| s.nodes.as_slice())
        } else {
            self.ctx
                .arena
                .get_block(body_node)
                .map(|block| block.statements.nodes.as_slice())
        };
        let Some(stmts) = stmts else {
            return false;
        };
        for &stmt_idx in stmts {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION =>
                {
                    return true;
                }
                // Handle ExportDeclaration wrapping a value declaration
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_decl) = self.ctx.arena.get_export_decl_at(stmt_idx) {
                        let clause_kind = self
                            .ctx
                            .arena
                            .get(export_decl.export_clause)
                            .map(|n| n.kind);
                        if clause_kind.is_some_and(|ck| {
                            ck == syntax_kind_ext::VARIABLE_STATEMENT
                                || ck == syntax_kind_ext::FUNCTION_DECLARATION
                                || ck == syntax_kind_ext::CLASS_DECLARATION
                                || ck == syntax_kind_ext::ENUM_DECLARATION
                        }) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// TS18033: Check that computed enum member initializers are assignable to `number`.
    ///
    /// For non-const, non-ambient enums, when a member initializer doesn't evaluate
    /// to a compile-time constant, tsc checks that the expression's type is assignable
    /// to `number`. If not, it emits TS18033.
    ///
    /// tsc's evaluator (`evaluate()`) tries to reduce each initializer to a concrete
    /// value. If evaluation succeeds (returns a number or string), no TS18033. If
    /// evaluation fails (returns undefined), tsc runs `checkTypeAssignableTo(type, number)`
    /// and emits TS18033 on failure.
    pub(super) fn check_computed_enum_member_values(&mut self, enum_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.ctx.arena.get_enum(node) else {
            return;
        };

        // Skip const enums (they use different errors: TS2474/TS2475)
        if self
            .ctx
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
        {
            return;
        }

        // Skip ambient enums (they use TS1066)
        if self.ctx.arena.is_declare(&enum_data.modifiers) {
            return;
        }

        for &member_idx in &enum_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(member_data) = self.ctx.arena.get_enum_member(member_node) else {
                continue;
            };

            let init_idx = member_data.initializer;
            if init_idx.is_none() {
                continue;
            }

            // Model tsc's evaluator: would evaluation succeed for this expression?
            // Returns: Some(true) = would succeed, Some(false) = would fail,
            // None = can't determine (e.g., cross-file import).
            let eval_result = self.enum_initializer_evaluation_status(init_idx);

            if eval_result == Some(true) {
                continue;
            }

            // Compute the expression's type for the assignability check.
            let init_type = self.compute_type_of_node(init_idx);

            if init_type == TypeId::ANY || init_type == TypeId::ERROR {
                continue;
            }

            // For unknown cases (imports), use type heuristic: if the type is
            // assignable to number or string, tsc's evaluator would likely succeed
            // (the import resolves to a const with a literal value).
            if eval_result.is_none()
                && (self
                    .computed_enum_member_relation_outcome(init_type, TypeId::NUMBER)
                    .related
                    || self
                        .computed_enum_member_relation_outcome(init_type, TypeId::STRING)
                        .related)
            {
                continue;
            }

            // Evaluation would fail (or unknown with non-number/string type).
            // Emit TS18033 if the type is not assignable to number.
            if !self
                .computed_enum_member_relation_outcome(init_type, TypeId::NUMBER)
                .related
            {
                // tsc displays widened types in TS18033: 'string' not '"bar"'
                let widened =
                    crate::query_boundaries::common::widen_literal_type(self.ctx.types, init_type);
                let source_str = self.format_type(widened);
                let target_str = self.format_type(TypeId::NUMBER);
                self.error_at_node_msg(
                    init_idx,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_AS_REQUIRED_FOR_COMPUTED_ENUM_MEMBER_VALUES,
                    &[&source_str, &target_str],
                );
            }
        }
    }
}

// =========================================================================
// Namespace-Class Merge Helpers
// =========================================================================

impl<'a> CheckerState<'a> {
    /// TS2300: Detect `prototype` exports in a namespace that merges with a class.
    ///
    /// When `declare class Foo {}` and `declare namespace Foo { namespace prototype { ... } }`
    /// exist, tsc reports "Duplicate identifier 'prototype'" on the namespace export.
    pub(super) fn check_namespace_prototype_conflict(&mut self, module_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;

        // Get the symbol for this namespace declaration
        let Some(&sym_id) = self.ctx.binder.node_symbols.get(&module_idx.0) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };

        // Check if this symbol also has a class declaration (class-namespace merge)
        let has_class = symbol.flags & tsz_binder::symbol_flags::CLASS != 0;
        if !has_class {
            return;
        }

        // Check if the namespace exports a member named "prototype"
        let Some(exports) = symbol.exports.as_ref() else {
            return;
        };
        let prototype_member_id =
            exports.iter().find_map(
                |(name, &id)| {
                    if name == "prototype" { Some(id) } else { None }
                },
            );
        let Some(prototype_member_id) = prototype_member_id else {
            return;
        };

        // Report TS2300 on the prototype member's declaration name node
        if let Some(proto_sym) = self.ctx.binder.get_symbol(prototype_member_id) {
            let decl_node = proto_sym.value_declaration;
            if decl_node != NodeIndex::NONE {
                let error_node = self
                    .get_declaration_name_node(decl_node)
                    .unwrap_or(decl_node);
                self.error_at_node_msg(
                    error_node,
                    diagnostic_codes::DUPLICATE_IDENTIFIER,
                    &["prototype"],
                );
            }
        }
    }
}
