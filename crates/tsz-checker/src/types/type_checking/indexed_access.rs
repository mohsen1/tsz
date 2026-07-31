use crate::query_boundaries::type_checking as type_checking_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

mod concrete_index_error;
mod deferred_conditional_index;
mod error_contagion;
mod generic_tuple_chain;
mod indexed_access_helpers;
mod infer_node_walk;
mod mapped_key_check;
mod object_format;

use indexed_access_helpers::{
    generic_constrained_index, indexed_access_object_alias_application_exceeds_depth,
    is_broad_index_type, is_unconstrained_type_param_object,
    remapped_mapped_type_template_index_should_report_ts2536, same_object_key_space,
    same_type_param_name,
};

/// Default-on; `TSZ_DISABLE_NESTED_INDEXED_ACCESS_CONSTRAINT_REDUCTION=1` is the
/// kill switch for the arbitrarily-deep deferred-indexed-access base-constraint
/// reduction in [`CheckerState::check_indexed_access_type`]. The env read is
/// cached so the (hot) per-indexed-access-node check is a plain bool load.
fn nested_indexed_access_constraint_reduction_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !std::env::var("TSZ_DISABLE_NESTED_INDEXED_ACCESS_CONSTRAINT_REDUCTION")
            .is_ok_and(|v| v == "1")
    })
}

impl<'a> CheckerState<'a> {
    /// Check if two AST nodes have the same text representation.
    fn nodes_have_same_text(&self, a: NodeIndex, b: NodeIndex) -> bool {
        let a_node = self.ctx.arena.get(a);
        let b_node = self.ctx.arena.get(b);
        match (a_node, b_node) {
            (Some(an), Some(bn)) if an.kind == bn.kind => {
                // Identifiers
                if let (Some(ai), Some(bi)) = (
                    self.ctx.arena.get_identifier(an),
                    self.ctx.arena.get_identifier(bn),
                ) {
                    return ai.escaped_text == bi.escaped_text;
                }
                // Literal types (e.g., LiteralType wrapping a string literal)
                if let (Some(alt), Some(blt)) = (
                    self.ctx.arena.get_literal_type(an),
                    self.ctx.arena.get_literal_type(bn),
                ) {
                    return self.nodes_have_same_text(alt.literal, blt.literal);
                }
                // String/number literals directly
                if let (Some(al), Some(bl)) = (
                    self.ctx.arena.get_literal(an),
                    self.ctx.arena.get_literal(bn),
                ) {
                    return al.text == bl.text;
                }
                false
            }
            _ => false,
        }
    }

    fn typeof_global_this_indexed_key_is_missing(&self, key: &str) -> bool {
        if key == "globalThis" {
            return false;
        }
        let Some(sym_id) = self.ctx.binder.file_locals.get(key) else {
            return true;
        };
        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
            symbol.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
                && !symbol.has_any_flags(tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE)
        })
    }

    pub(crate) fn is_keyof_for_current_object(
        &mut self,
        ty: TypeId,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        crate::query_boundaries::state::checking::keyof_operands_through_filters(self.ctx.types, ty)
            .into_iter()
            .any(|operand| {
                let evaluated_operand = self.evaluate_type_with_env(operand);
                same_object_key_space(self.ctx.types, operand, object_type)
                    || same_object_key_space(self.ctx.types, operand, object_type_for_check)
                    || same_object_key_space(self.ctx.types, evaluated_operand, object_type)
                    || same_object_key_space(
                        self.ctx.types,
                        evaluated_operand,
                        object_type_for_check,
                    )
            })
    }

    /// Resolve a type parameter's constraint from its AST declaration when the TypeId
    /// doesn't carry one. This handles cases where type parameters lose their constraints
    /// during type application argument resolution (e.g., `M[Event]` inside `Id<M[Event]>`).
    pub(crate) fn resolve_index_constraint_from_declaration(
        &mut self,
        index_node_idx: NodeIndex,
        _object_node_idx: NodeIndex,
    ) -> Option<TypeId> {
        let index_name = self.simple_type_reference_name(index_node_idx)?;

        let mut current = self
            .ctx
            .arena
            .get_extended(index_node_idx)
            .map(|ext| ext.parent);
        while let Some(parent_idx) = current {
            let parent_node = self.ctx.arena.get(parent_idx)?;
            // Extract type_parameters NodeList from any generic declaration kind
            let type_params: Option<&tsz_parser::parser::base::NodeList> = match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent_node)
                        .and_then(|f| f.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::METHOD_SIGNATURE
                    || k == syntax_kind_ext::CALL_SIGNATURE
                    || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                {
                    self.ctx
                        .arena
                        .get_signature(parent_node)
                        .and_then(|s| s.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(parent_node)
                    .and_then(|i| i.type_parameters.as_ref()),
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_class(parent_node)
                        .and_then(|c| c.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                    .ctx
                    .arena
                    .get_type_alias(parent_node)
                    .and_then(|ta| ta.type_parameters.as_ref()),
                k if k == syntax_kind_ext::FUNCTION_TYPE
                    || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
                {
                    self.ctx
                        .arena
                        .get_function_type(parent_node)
                        .and_then(|ft| ft.type_parameters.as_ref())
                }
                _ => None,
            };

            if let Some(tp_list) = type_params {
                for &tp_idx in &tp_list.nodes {
                    let Some(tp_node) = self.ctx.arena.get(tp_idx) else {
                        continue;
                    };
                    let Some(tp) = self.ctx.arena.get_type_parameter(tp_node) else {
                        continue;
                    };
                    let Some(name_node) = self.ctx.arena.get(tp.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    if ident.escaped_text == index_name && tp.constraint != NodeIndex::NONE {
                        let constraint_type = self.get_type_from_type_node(tp.constraint);
                        if constraint_type != TypeId::ERROR {
                            return Some(constraint_type);
                        }
                    }
                }
            }
            // Mapped type key parameter: `[K in C]: ...` — extract constraint C
            if parent_node.kind == syntax_kind_ext::MAPPED_TYPE
                && let Some(mapped) = self.ctx.arena.get_mapped_type(parent_node)
                && let Some(tp_node) = self.ctx.arena.get(mapped.type_parameter)
                && let Some(tp) = self.ctx.arena.get_type_parameter(tp_node)
                && let Some(name_node) = self.ctx.arena.get(tp.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && ident.escaped_text == index_name
                && tp.constraint != NodeIndex::NONE
            {
                let constraint_type = self.get_type_from_type_node(tp.constraint);
                if constraint_type != TypeId::ERROR {
                    return Some(constraint_type);
                }
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        None
    }

    /// Check if the indexed access `T[K]` is inside the true branch of a conditional type
    /// `K extends keyof T ? ... : ...`. In the true branch, `K` is narrowed to `keyof T`,
    /// so the index is valid.
    fn is_in_conditional_keyof_narrowing_context(
        &mut self,
        node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
        _index_type: TypeId,
    ) -> bool {
        let index_name = self.simple_type_reference_name(
            self.ctx
                .arena
                .get(node_idx)
                .and_then(|n| self.ctx.arena.get_indexed_access_type(n))
                .map(|iat| iat.index_type)
                .unwrap_or(NodeIndex::NONE),
        );

        let mut current = self.ctx.arena.parent_of(node_idx);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::CONDITIONAL_TYPE
                && let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
            {
                // Check if the indexed access is in the true branch
                // (the node_idx must be a descendant of cond.true_type)
                let in_true_branch = self.is_descendant_of(node_idx, cond.true_type);
                if in_true_branch {
                    // Check if the check type matches the index type
                    let check_name = self.simple_type_reference_name(cond.check_type);
                    if check_name.is_some() && check_name == index_name {
                        // Check if the extends type is `keyof T` for our object
                        let extends_type = self.get_type_from_type_node(cond.extends_type);
                        if self.is_keyof_for_current_object(
                            extends_type,
                            object_type,
                            object_type_for_check,
                        ) {
                            return true;
                        }
                        // Also check if extends type is keyof applied to the object
                        if let Some(extends_node) = self.ctx.arena.get(cond.extends_type)
                            && let Some(type_op) = self.ctx.arena.get_type_operator(extends_node)
                            && type_op.operator == SyntaxKind::KeyOfKeyword as u16
                        {
                            let keyof_target_type = self.get_type_from_type_node(type_op.type_node);
                            if same_object_key_space(self.ctx.types, keyof_target_type, object_type)
                                || same_object_key_space(
                                    self.ctx.types,
                                    keyof_target_type,
                                    object_type_for_check,
                                )
                            {
                                return true;
                            }
                        }
                    }
                    // Also check for `infer X extends C` patterns in the extends type.
                    // When the extends type contains `infer Head extends DistributedKeyOf<ObjT>`,
                    // the inferred type parameter `Head` is constrained to `keyof ObjT` in the
                    // true branch. If our index type matches such an infer parameter, suppress
                    // TS2536.
                    if let Some(ref idx_name) = index_name
                        && self.extends_type_has_infer_keyof_constraint(
                            cond.extends_type,
                            idx_name,
                            object_type,
                            object_type_for_check,
                        )
                    {
                        return true;
                    }
                }
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        false
    }

    /// Check if the extends type of a conditional contains an `infer X extends C` pattern
    /// where `X` matches `target_name` and `C` resolves to `keyof ObjT`.
    fn extends_type_has_infer_keyof_constraint(
        &mut self,
        extends_node_idx: NodeIndex,
        target_name: &str,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        // Collect all infer type nodes from the extends type subtree.
        // We use a stack-based approach since there's no generic node_children method.
        let infer_nodes = self.collect_infer_nodes_in_subtree(extends_node_idx);
        for infer_node_idx in infer_nodes {
            let Some(node) = self.ctx.arena.get(infer_node_idx) else {
                continue;
            };
            let Some(infer_data) = self.ctx.arena.get_infer_type(node) else {
                continue;
            };
            let Some(tp_node) = self.ctx.arena.get(infer_data.type_parameter) else {
                continue;
            };
            let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(tp_data.name) else {
                continue;
            };
            let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };
            if ident.escaped_text != target_name || tp_data.constraint == NodeIndex::NONE {
                continue;
            }
            // The constraint exists — check if it resolves to keyof ObjT
            let constraint_type = self.get_type_from_type_node(tp_data.constraint);
            let constraint_eval = self.evaluate_type_with_env(constraint_type);
            if self.is_keyof_for_current_object(constraint_type, object_type, object_type_for_check)
                || self.is_keyof_for_current_object(
                    constraint_eval,
                    object_type,
                    object_type_for_check,
                )
            {
                return true;
            }
            // Also check assignability: constraint might be
            // DistributedKeyOf<ObjT> which evaluates to keyof ObjT
            let keyof_object = self.ctx.types.evaluate_keyof(object_type_for_check);
            if self
                .indexed_access_key_space_relation_outcome(constraint_eval, keyof_object)
                .related
            {
                return true;
            }
        }
        false
    }

    /// Check if `node_a` is a descendant of `node_b` in the AST.
    fn is_descendant_of(&self, node_a: NodeIndex, node_b: NodeIndex) -> bool {
        let mut current = Some(node_a);
        while let Some(idx) = current {
            if idx == node_b {
                return true;
            }
            current = self.ctx.arena.parent_of(idx);
        }
        false
    }

    /// Check if the index type parameter has a `keyof` constraint targeting the object type,
    /// resolved from the AST declaration. Returns true if `K extends keyof T` for the current
    /// object T.
    fn index_has_keyof_constraint_from_declaration(
        &mut self,
        index_node_idx: NodeIndex,
        object_node_idx: NodeIndex,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        if let Some(constraint_type) =
            self.resolve_index_constraint_from_declaration(index_node_idx, object_node_idx)
        {
            // Check if the constraint is `keyof T` for our object
            if self.is_keyof_for_current_object(constraint_type, object_type, object_type_for_check)
            {
                return true;
            }
            // Also check if the constraint is directly assignable to keyof of the object
            // (handles cases like `K extends string` indexing `Record<string, V>`)
        }
        false
    }

    /// Check an indexed access type (T[K]).
    pub(crate) fn check_indexed_access_type(&mut self, node_idx: NodeIndex) {
        // tsc does not re-type-check declaration files (.d.ts) — they are trusted
        // as correct declarations. Checking them generates false positives because
        // the type constraint relationships (e.g. keyof Readonly<T> = keyof T) are
        // not always reconstructible from type IDs without full evaluation.
        if self.ctx.is_declaration_file() {
            return;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };
        let Some(data) = self.ctx.arena.get_indexed_access_type(node) else {
            return;
        };

        if self.indexed_access_literal_property_exists_in_alias_union(
            data.object_type,
            data.index_type,
        ) {
            return;
        }
        if self
            .type_literal_dispatch_index_is_declared_key_subset(data.object_type, data.index_type)
        {
            return;
        }

        let index_type = self.get_type_from_type_node(data.index_type);
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if crate::types_domain::type_node_helpers::is_typeof_global_this_type_node(
            self.ctx.arena,
            data.object_type,
        ) && let Some(key) =
            crate::types_domain::type_node_helpers::get_string_literal_from_type_index(
                self.ctx.arena,
                data.index_type,
            )
            && self.typeof_global_this_indexed_key_is_missing(&key)
        {
            return;
        }

        if index_type != TypeId::ERROR
            && self.type_literal_ast_key_space_accepts_index(data.object_type, index_type)
        {
            return;
        }
        if index_type != TypeId::ERROR
            && self.nested_type_literal_index_access_allows_index(
                data.object_type,
                data.index_type,
                index_type,
            )
        {
            return;
        }

        let object_type = self.get_type_from_type_node(data.object_type);

        if indexed_access_object_alias_application_exceeds_depth(self, data.object_type) {
            self.error_at_node(
                data.object_type,
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
            return;
        }

        if object_type == TypeId::ERROR
            && index_type != TypeId::ERROR
            && index_type != TypeId::NEVER
            && let Some(object_node) = self.ctx.arena.get(data.object_type)
            && self
                .ctx
                .arena
                .get_indexed_access_type(object_node)
                .is_some()
            && let Some(raw_object_text) = self.node_text(data.object_type)
        {
            let nested_base_type = self
                .ctx
                .arena
                .get_indexed_access_type(object_node)
                .map(|nested| self.get_type_from_type_node(nested.object_type));
            if let Some(base_type) = nested_base_type
                && self.indexed_access_constraint_values_allow_index(base_type, index_type)
                && !self.ast_index_constraint_keyof_targets_foreign_indexed_object(
                    data.object_type,
                    data.index_type,
                )
            {
                return;
            }
            if self.nested_type_literal_index_access_allows_index(
                data.object_type,
                data.index_type,
                index_type,
            ) {
                return;
            }
            // Clean up object type text: normalize stray whitespace (so the
            // display matches tsc's printer), strip enclosing parens, and drop
            // any trailing index access syntax that may leak from the node span.
            let normalized_object_text =
                object_format::normalize_indexed_access_object_text(&raw_object_text);
            let object_type_str = {
                let trimmed = normalized_object_text.trim();
                let trimmed = trimmed.strip_prefix('(').unwrap_or(trimmed);
                let trimmed = trimmed.strip_suffix(')').unwrap_or(trimmed);
                if let Some(pos) = trimmed.find(")[") {
                    trimmed[..pos].trim().to_string()
                } else {
                    trimmed.trim().to_string()
                }
            };
            let object_type_str = if object_type_str.is_empty() || object_type_str.contains('[') {
                self.format_type(object_type)
            } else {
                object_type_str
            };
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &object_type_str],
            );
            self.error_at_node(
                node_idx,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }

        if object_type == TypeId::ERROR
            || index_type == TypeId::ERROR
            || object_type == TypeId::ANY
            || index_type == TypeId::NEVER
        {
            return;
        }

        // TS4105: Private or protected member cannot be accessed on a type parameter.
        // When the object type is (or contains) a type parameter and the index is a
        // string literal naming a private/protected property on the constraint, tsc
        // emits this error. The check fires per-type-parameter in unions but NOT for
        // intersection constraints (tsc skips those).
        if let Some(prop_atom) =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
        {
            let property_name = self.ctx.types.resolve_atom(prop_atom);
            self.check_ts4105_private_on_type_parameter(node_idx, object_type, &property_name);
        }

        let mut index_constraint =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, index_type);
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, index_type)
            && index_constraint.is_none()
            && let Some(ast_constraint) =
                self.resolve_index_constraint_from_declaration(data.index_type, data.object_type)
        {
            index_constraint = Some(ast_constraint);
        }
        let error_anchor = node_idx;
        let concrete_error_anchor = data.index_type;
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, object_type)
            && index_constraint.is_some_and(|constraint| {
                constraint == object_type
                    || same_type_param_name(self.ctx.types, constraint, object_type)
            })
        {
            let obj_type_str = self.format_type(object_type);
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }

        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, object_type)
            && self.generic_index_mentions_transformed_current_type_param(index_type, object_type)
            && !self.transformed_index_key_space_indexes_object(
                index_type,
                index_constraint,
                object_type,
            )
        {
            let obj_type_str = self.format_type(object_type);
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, object_type)
            && crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, index_type)
            && index_constraint.is_some_and(|constraint| {
                crate::query_boundaries::key_constraints::is_symbol_only_key_constraint(
                    self.ctx.types,
                    constraint,
                )
            })
            && !self.is_valid_index_for_type_param(index_type, object_type)
        {
            let obj_type_str = self.format_type(object_type);
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }

        // Fast path: when the index is a type parameter and the object type node
        // is a type literal, compute keyof from AST property names only (no
        // value-type evaluation needed). This avoids eagerly resolving complex
        // member types (e.g., generic type applications) just to check key validity.
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, index_type)
            && let Some(keyof_type) = self.type_literal_keyof_from_node(data.object_type)
        {
            let check_index = index_constraint.unwrap_or(index_type);
            let check_index_eval = self.evaluate_type_with_env(check_index);
            if self
                .indexed_access_key_space_relation_outcome(check_index_eval, keyof_type)
                .related
            {
                return;
            }
        }

        let mut object_type_for_check = self.evaluate_type_with_env(object_type);
        // Indexing `never` is always valid (produces `never`), so suppress TS2536.
        // This handles cases like `(A & B)['kind']` where `A & B` reduces to `never`
        // due to conflicting discriminant properties.
        if object_type_for_check == TypeId::NEVER {
            return;
        }
        object_type_for_check = crate::query_boundaries::common::type_parameter_constraint(
            self.ctx.types,
            object_type_for_check,
        )
        .unwrap_or(object_type_for_check);
        if let Some((base_object_type, access_index_type)) =
            crate::query_boundaries::common::index_access_types(
                self.ctx.types,
                object_type_for_check,
            )
        {
            if let Some(base_constraint) =
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    base_object_type,
                )
            {
                // Single-level `K[idx]`: the access base is itself a type
                // parameter, so substitute its constraint and evaluate.
                let constrained_access = type_checking_query::type_checking_index_access(
                    self.ctx.types,
                    base_constraint,
                    access_index_type,
                );
                let evaluated_constrained_access =
                    self.evaluate_type_for_assignability(constrained_access);
                if evaluated_constrained_access != TypeId::ERROR {
                    object_type_for_check = evaluated_constrained_access;
                }
            } else if nested_indexed_access_constraint_reduction_enabled() {
                // Arbitrarily-deep deferred indexed access (`T[K1][K2][K3]…`):
                // the access base is itself an indexed access, not a bare type
                // parameter, so the single-level branch above does not apply and
                // the key space stays deferred. A literal outer key — e.g.
                // `T[K1][K2][K3]` with `K3 extends keyof T[keyof T][keyof
                // T[keyof T]]`, which reduces to a concrete literal — then cannot
                // be validated, yielding a spurious TS2536 (the depth-≥3
                // generalization of the depth-2 #13720 recovery). Reduce every
                // reachable type parameter to its constraint and evaluate,
                // mirroring tsc's `getApparentType` /
                // `getConstraintOfIndexedAccessType`; full parameter substitution
                // leaves no free parameters, so the result is the concrete
                // apparent base. It is used only for key-space validation here —
                // `object_type` keeps the original surface for diagnostics, and a
                // genuinely-missing key still fails the relation below, so a real
                // TS2536 is preserved.
                let reduced =
                    crate::query_boundaries::type_computation::complex::instantiate_type_params_to_constraints(
                        self.ctx.types,
                        object_type_for_check,
                    );
                if reduced != object_type_for_check {
                    let evaluated = self.evaluate_type_for_assignability(reduced);
                    if evaluated != TypeId::ERROR && evaluated != TypeId::ANY {
                        object_type_for_check = evaluated;
                    }
                }
            }
        }
        if crate::query_boundaries::common::is_generic_application(
            self.ctx.types,
            object_type_for_check,
        ) {
            let expanded_object = self.evaluate_application_type(object_type_for_check);
            if expanded_object != TypeId::ERROR && expanded_object != TypeId::ANY {
                object_type_for_check = expanded_object;
            }
        }
        if index_type == TypeId::ANY {
            // tsc defers indexed-access validation in generic contexts.
            // When the object type still contains type parameters (or IS
            // a type parameter), `any` as an index is fine — the concrete
            // check will happen at instantiation time.
            if object_type_for_check == TypeId::ANY
                || crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    object_type_for_check,
                )
                || crate::query_boundaries::common::is_index_access_type(
                    self.ctx.types,
                    object_type_for_check,
                )
            {
                return;
            }
            if !self.is_element_indexable_by_any_key(object_type_for_check) {
                // tsc keeps the index syntactically generic when the AST node
                // is a bare type-parameter reference, even when our resolution
                // evaluated the parameter to `any` (typically via a constraint
                // that itself resolved through a property with `any` type).
                // Defer rejection to instantiation time — mirroring tsc's
                // `getActualTypeOfIndexedAccess` deferral.
                if crate::query_boundaries::type_checking_utilities::ast_index_node_is_in_scope_type_parameter(
                    self.ctx.arena,
                    self.ctx.binder,
                    &self.ctx.type_parameter_scope,
                    data.index_type,
                ) {
                    return;
                }
                let message_2538 = format_message(
                    diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    &["any"],
                );
                self.error_at_index_type_span(
                    error_anchor,
                    &message_2538,
                    diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                );
                return;
            }
            return;
        }
        let keyof_object = if let Some(mapped_id) =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, object_type_for_check)
        {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            let mapped_constraint = mapped.constraint;
            let keyof = self.evaluate_mapped_constraint_with_resolution(mapped_constraint);

            // When the index is `keyof T` and the mapped type iterates over `keyof T`
            // (same T), the index is always valid. Check both the raw constraint and
            // the evaluated result for structural equivalence via same_object_key_space.
            if let Some(index_operand) =
                crate::query_boundaries::state::checking::keyof_target(self.ctx.types, index_type)
            {
                if let Some(constraint_operand) =
                    crate::query_boundaries::state::checking::keyof_target(
                        self.ctx.types,
                        mapped_constraint,
                    )
                    && same_object_key_space(self.ctx.types, index_operand, constraint_operand)
                {
                    return;
                }
                // Also check against the evaluated keyof result
                if let Some(keyof_operand) =
                    crate::query_boundaries::state::checking::keyof_target(self.ctx.types, keyof)
                    && same_object_key_space(self.ctx.types, index_operand, keyof_operand)
                {
                    return;
                }
            }
            if self.index_constraint_keyof_matches_mapped_constraint(
                index_constraint,
                mapped_constraint,
                keyof,
            ) {
                return;
            }

            keyof
        } else {
            // Resolve the receiver's index-signature key aliases first (e.g. the
            // lib global `PropertyKey`, only resolvable at use time) so the
            // resolver-less `evaluate_keyof` query classifies the full key space —
            // notably the `symbol` arm. Without this, `keyof { [k: PropertyKey]:
            // V }` drops `symbol` and a symbol index yields a spurious TS2536
            // (#14315).
            let normalized = self.resolve_receiver_index_signature_keys(object_type_for_check);
            self.ctx.types.evaluate_keyof(normalized)
        };
        let is_self_derived_key_space = |candidate: TypeId| {
            crate::query_boundaries::common::index_access_types(self.ctx.types, candidate)
                .is_some_and(|(derived_object, derived_index)| {
                    crate::query_boundaries::common::is_type_parameter_like(
                        self.ctx.types,
                        index_type,
                    ) && !crate::query_boundaries::common::is_type_parameter_like(
                        self.ctx.types,
                        derived_object,
                    ) && (derived_index == index_type
                        || same_type_param_name(self.ctx.types, derived_index, index_type))
                })
        };
        let is_self_derived_keyof_space = |candidate: TypeId| {
            crate::query_boundaries::state::checking::keyof_target(self.ctx.types, candidate)
                .and_then(|target| {
                    crate::query_boundaries::common::index_access_types(self.ctx.types, target)
                })
                .is_some_and(|(derived_object, derived_index)| {
                    crate::query_boundaries::common::is_type_parameter_like(
                        self.ctx.types,
                        index_type,
                    ) && !crate::query_boundaries::common::is_type_parameter_like(
                        self.ctx.types,
                        derived_object,
                    ) && (derived_index == index_type
                        || same_type_param_name(self.ctx.types, derived_index, index_type))
                })
        };
        if is_self_derived_key_space(keyof_object)
            || is_self_derived_key_space(self.evaluate_type_with_env(keyof_object))
            || is_self_derived_keyof_space(keyof_object)
            || is_self_derived_keyof_space(self.evaluate_type_with_env(keyof_object))
        {
            let obj_type_str = self.format_type(object_type);
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }

        // A generic indexed-access chain rooted at a concrete tuple stays deferred
        // in `tsc`: `Table[D1]` with `D1 extends 0 | 1 | 2` has no known element at
        // index `2`, so `Table[D1][0]` is a genuine TS2536. By this point the inner
        // access has already been evaluated to a clean element-value union whose
        // key space accepts `0`, so this verdict must be taken before `keyof_object`
        // is consulted below.
        if self.generic_tuple_chain_index_access_rejects(
            data.object_type,
            data.index_type,
            index_type,
        ) {
            let obj_type_str = self.format_ts2536_object_type(data.object_type, object_type);
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }

        let index_type_for_check = self.evaluate_type_with_env(index_type);
        if let Some(prop_atom) =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)
            && self.ctx.types.resolve_atom(prop_atom) == "length"
            && self.indexed_access_object_allows_length_property(object_type, object_type_for_check)
        {
            return;
        }
        if self.conditional_true_branch_constraint_allows_index(
            node_idx,
            data.object_type,
            data.index_type,
            index_type_for_check,
        ) {
            return;
        }
        // Error-type contagion: when the indexed-access object type references an
        // *unresolved imported alias* (e.g. `TupleParts<T>["required"]` where
        // `TupleParts` comes from a module that failed to resolve — already
        // flagged TS2307), tsc gives the object the permissive `error` apparent
        // type, whose key space is universal, so the access accepts any key (and a
        // further `[K2]` / `[...spread]` over it is also accepted). Suppress
        // TS2536 here to match — the import-failure diagnostic is the only error
        // tsc reports for these positions.
        if self.indexed_access_object_is_unresolved_import_error(object_type, object_type_for_check)
        {
            return;
        }
        // A deferred conditional object base has no key space of its own; tsc
        // resolves it through `getApparentType` to its default constraint (the
        // union of branch results) and validates the index key against that.
        // When the key is in that key space the access is valid (tsc emits no
        // error), even though the access itself stays deferred. The object type
        // is left untouched so an out-of-range key still flows to the existing
        // TS2536 path rather than the concrete property-missing (TS2339) path.
        if self.deferred_conditional_index_is_in_key_space(
            object_type_for_check,
            index_type_for_check,
            index_type,
        ) {
            return;
        }
        if remapped_mapped_type_template_index_should_report_ts2536(
            self.ctx.types,
            object_type_for_check,
            index_type,
            index_type_for_check,
        ) {
            let obj_type_str = self.format_type(object_type);
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }
        let foreign_keyof_indexed_constraint =
            self.index_constraint_keyof_targets_foreign_indexed_object(
                object_type,
                object_type_for_check,
                index_type,
                index_constraint,
            ) || self.ast_index_constraint_keyof_targets_foreign_indexed_object(
                data.object_type,
                data.index_type,
            );
        if foreign_keyof_indexed_constraint {
            let obj_type_str = self
                .node_text(data.object_type)
                .map(|text| object_format::normalize_indexed_access_object_text(&text))
                .unwrap_or_else(|| self.format_type(object_type));
            let index_type_str = self.format_type(index_type);
            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
            return;
        }
        // First check: raw index type against keyof.
        // This handles cases where keyof includes type parameters from mapped types
        // (e.g. keyof ({ [P in T]: P } & ...) = T | ...) and the index IS that parameter.
        if self
            .indexed_access_key_space_relation_outcome(index_type_for_check, keyof_object)
            .related
        {
            return;
        }
        if self.conditional_result_branches_satisfy_constraint(index_type, keyof_object)
            || self
                .conditional_result_branches_satisfy_constraint(index_type_for_check, keyof_object)
        {
            return;
        }
        // When the solver TypeData doesn't carry the constraint (common for type
        // parameters in generic signatures), use the AST-resolved constraint.
        // E.g. `emit<Event extends keyof M>(...args: M[Event])` — Event's
        // constraint `keyof M` is found from the AST but not in the TypeId.
        if let Some(constraint) = index_constraint {
            let constraint_eval = self.evaluate_type_with_env(constraint);
            if self
                .indexed_access_key_space_relation_outcome(constraint_eval, keyof_object)
                .related
            {
                return;
            }
        }
        if self.is_mapped_key_index_for_current_object(
            node_idx,
            data.object_type,
            data.index_type,
            object_type,
            object_type_for_check,
        ) {
            return;
        }
        if self.mapped_object_index_matches_own_key_constraint(
            data.object_type,
            index_type,
            index_type_for_check,
        ) {
            return;
        }
        // When the constraint was resolved from AST, also check if it represents
        // a keyof for the current object type (catches deferred keyof patterns that
        // aren't directly assignable to the computed keyof).
        if let Some(constraint) = index_constraint {
            let evaluated_constraint = self.evaluate_type_with_env(constraint);
            if self.is_keyof_for_current_object(
                evaluated_constraint,
                object_type,
                object_type_for_check,
            ) || self.is_keyof_for_current_object(constraint, object_type, object_type_for_check)
            {
                return;
            }
        }
        // Follow the constraint chain transitively (P -> K -> keyof T) so that
        // e.g. T[P] where P extends K extends keyof T doesn't false-positive.
        // At each level, check assignability to keyof or recognize deferred types.
        let mut index_type_for_check = index_type_for_check;
        for _ in 0..5 {
            let next = crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                index_type_for_check,
            );
            let Some(next_constraint) = next else { break };
            let next_evaluated = self.evaluate_type_with_env(next_constraint);
            if self
                .indexed_access_key_space_relation_outcome(next_evaluated, keyof_object)
                .related
            {
                return;
            }
            // If the constraint resolved to a deferred key space for THIS object,
            // suppress TS2536. For unrelated key spaces (e.g. `F extends keyof D[T]`
            // used as `D[F]`), we must keep checking and report TS2536.
            if self.is_keyof_for_current_object(next_evaluated, object_type, object_type_for_check)
                || self.is_keyof_for_current_object(
                    next_constraint,
                    object_type,
                    object_type_for_check,
                )
            {
                return;
            }
            // Continue following if still a type parameter.
            if !crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                next_evaluated,
            ) {
                index_type_for_check = next_evaluated;
                break;
            }
            index_type_for_check = next_evaluated;
        }
        // When the solver-level type parameter had no constraint but an AST-resolved
        // constraint exists (e.g. mapped type key `k` in `[k in K]: T[k]` where the
        // solver's TypeId for `k` doesn't carry the constraint), follow the chain
        // starting from the AST-resolved constraint. This handles patterns like
        // `[k in K]: T[k]` where `K extends keyof T` — the chain K → keyof T must
        // be followed to suppress the false TS2536.
        if let Some(ast_constraint) = index_constraint {
            let mut chain_start = ast_constraint;
            for _ in 0..5 {
                let evaluated = self.evaluate_type_with_env(chain_start);
                if self.is_keyof_for_current_object(evaluated, object_type, object_type_for_check)
                    || self.is_keyof_for_current_object(
                        chain_start,
                        object_type,
                        object_type_for_check,
                    )
                {
                    return;
                }
                if self
                    .indexed_access_key_space_relation_outcome(evaluated, keyof_object)
                    .related
                {
                    return;
                }
                let next = crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    evaluated,
                );
                let Some(next_constraint) = next else { break };
                chain_start = next_constraint;
            }
        }
        if !self
            .indexed_access_key_space_relation_outcome(index_type_for_check, keyof_object)
            .related
        {
            if let Some((wants_string, wants_number)) =
                self.get_index_key_kind(index_type_for_check)
                && !generic_constrained_index(
                    self.ctx.types,
                    object_type_for_check,
                    index_type,
                    index_constraint,
                )
                && !is_unconstrained_type_param_object(self.ctx.types, object_type_for_check)
                && self.is_element_indexable(object_type_for_check, wants_string, wants_number)
            {
                return;
            }
            if crate::query_boundaries::common::numeric_literal_index_valid_for_object(
                self.ctx.types,
                index_type_for_check,
                object_type_for_check,
            ) {
                return;
            }
            if self.is_numeric_index_on_parameters_utility(data.object_type, index_type_for_check) {
                return;
            }
            if self.canonical_numeric_string_literal_valid_for_object(
                index_type_for_check,
                object_type_for_check,
            ) {
                return;
            }
            if self.union_index_members_valid_for_object(
                index_type_for_check,
                object_type_for_check,
                keyof_object,
            ) {
                return;
            }
            if self.keyof_index_valid_for_string_indexed_object(
                object_type_for_check,
                index_type_for_check,
                index_constraint,
            ) {
                return;
            }
            if let Some(object_type_node) = self.ctx.arena.get(data.object_type)
                && let Some(nested_indexed_access) =
                    self.ctx.arena.get_indexed_access_type(object_type_node)
            {
                let mut constrained_base_type =
                    self.get_type_from_type_node(nested_indexed_access.object_type);
                constrained_base_type = crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    constrained_base_type,
                )
                .unwrap_or(constrained_base_type);

                let nested_index_type =
                    self.get_type_from_type_node(nested_indexed_access.index_type);
                let constrained_base_keyof = self
                    .type_literal_keyof_from_node(nested_indexed_access.object_type)
                    .unwrap_or_else(|| self.ctx.types.evaluate_keyof(constrained_base_type));
                let nested_index_for_check = self.evaluate_type_with_env(nested_index_type);
                let nested_index_constraint_matches =
                    crate::query_boundaries::common::type_parameter_constraint(
                        self.ctx.types,
                        nested_index_for_check,
                    )
                    .is_some_and(|constraint| {
                        let constraint = self.evaluate_type_with_env(constraint);
                        self.indexed_access_key_space_relation_outcome(
                            constraint,
                            constrained_base_keyof,
                        )
                        .related
                    });
                let nested_index_matches_constrained_base = self
                    .indexed_access_key_space_relation_outcome(
                        nested_index_for_check,
                        constrained_base_keyof,
                    )
                    .related
                    || nested_index_constraint_matches
                    || self.is_keyof_for_current_object(
                        nested_index_type,
                        constrained_base_type,
                        constrained_base_type,
                    )
                    || self.is_keyof_for_current_object(
                        nested_index_for_check,
                        constrained_base_type,
                        constrained_base_type,
                    );
                if nested_index_matches_constrained_base {
                    if self.type_literal_member_values_accept_index(
                        nested_indexed_access.object_type,
                        index_type_for_check,
                        index_constraint,
                    ) {
                        return;
                    }
                    let constrained_object_type = if let Some(prop_atom) =
                        crate::query_boundaries::common::string_literal_value(
                            self.ctx.types,
                            nested_index_type,
                        ) {
                        let property_name = self.ctx.types.resolve_atom(prop_atom);
                        match self
                            .resolve_property_access_with_env(constrained_base_type, &property_name)
                        {
                            tsz_solver::operations::property::PropertyAccessResult::Success {
                                type_id,
                                ..
                            } => type_id,
                            _ => self.evaluate_type_with_env(
                                type_checking_query::type_checking_index_access(
                                    self.ctx.types,
                                    constrained_base_type,
                                    nested_index_type,
                                ),
                            ),
                        }
                    } else {
                        // When the nested index is a type parameter (e.g., k in a mapped
                        // type), the solver can't resolve `constraint[k]` directly.
                        // First try index signature lookup, then fall back to evaluation.
                        let evaluated_base = self.evaluate_type_with_env(constrained_base_type);
                        let index_info = self.ctx.types.get_index_signatures(evaluated_base);
                        if let Some(ref sig) = index_info.string_index {
                            sig.value_type
                        } else {
                            self.evaluate_type_with_env(
                                type_checking_query::type_checking_index_access(
                                    self.ctx.types,
                                    constrained_base_type,
                                    nested_index_type,
                                ),
                            )
                        }
                    };
                    // When the constrained object is still a deferred indexed access,
                    // try evaluating it further. If it resolves to a concrete type,
                    // use that for validation. Otherwise, check if the evaluated type
                    // has index signatures or properties that validate the index.
                    let constrained_object_type =
                        if crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            constrained_object_type,
                        ) {
                            let evaluated =
                                self.evaluate_type_for_assignability(constrained_object_type);
                            if evaluated != TypeId::ERROR
                                && !crate::query_boundaries::common::is_index_access_type(
                                    self.ctx.types,
                                    evaluated,
                                )
                            {
                                evaluated
                            } else {
                                constrained_object_type
                            }
                        } else {
                            constrained_object_type
                        };
                    if constrained_object_type != TypeId::ERROR
                        // When the constrained object is still a deferred indexed access
                        // (e.g., T[keyof T] where T is unconstrained), or resolves to
                        // `any` (recursive/circular constraints), property lookups may
                        // spuriously succeed. Skip this block so the error is caught
                        // by the deferred-suppression or final error path below.
                        && constrained_object_type != TypeId::ANY
                        && !crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            constrained_object_type,
                        )
                    {
                        // Check broad index types (string/number/symbol)
                        if is_broad_index_type(self.ctx.types, index_type_for_check)
                            && let Some((wants_string, wants_number)) =
                                self.get_index_key_kind(index_type_for_check)
                            && self.is_element_indexable(
                                constrained_object_type,
                                wants_string,
                                wants_number,
                            )
                        {
                            return;
                        }
                        // Check string literal indices via property access on the
                        // resolved constraint type. This handles generic class instances
                        // (e.g., ZodType<any>) where evaluate_keyof doesn't enumerate
                        // class members.
                        if let Some(prop_atom) =
                            crate::query_boundaries::common::string_literal_value(
                                self.ctx.types,
                                index_type_for_check,
                            )
                        {
                            let property_name = self.ctx.types.resolve_atom(prop_atom);
                            let prop_result = self.resolve_property_access_with_env(
                                constrained_object_type,
                                &property_name,
                            );
                            if matches!(
                                prop_result,
                                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                            ) {
                                return;
                            }
                        }
                        // Fall back to keyof check for non-literal indices.
                        let constrained_keyof =
                            self.ctx.types.evaluate_keyof(constrained_object_type);
                        if self
                            .indexed_access_key_space_relation_outcome(
                                index_type_for_check,
                                constrained_keyof,
                            )
                            .related
                        {
                            return;
                        }
                    }
                }
            }
            // When the index is a concrete string literal (not a type parameter or
            // deferred type), do NOT suppress TS2536 just because the object type
            // is a deferred indexed access — tsc still emits TS2536 for patterns
            // like `T[keyof T]["foo"]` where the literal can't be validated as a
            // key of the unresolved indexed access result.
            let index_is_concrete_literal = crate::query_boundaries::common::string_literal_value(
                self.ctx.types,
                index_type_for_check,
            )
            .is_some();
            // Suppress TS2536 when the index is deferred (conditional, application,
            // keyof, or error) — tsc defers generic-level checks to instantiation time.
            // Check both evaluated and original types since evaluation can partially
            // resolve to ERROR or Conditional.
            let is_deferred_object_type = |ty: TypeId| -> bool {
                ty == TypeId::ERROR
                    || crate::query_boundaries::common::is_conditional_type(self.ctx.types, ty)
                    || crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
                    || crate::query_boundaries::common::is_keyof_type(self.ctx.types, ty)
            };
            let key_space_is_unresolved = |ty: TypeId| -> bool {
                ty == TypeId::ERROR
                    || ty == TypeId::ANY
                    || crate::query_boundaries::common::is_conditional_type(self.ctx.types, ty)
                    || crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
                    || crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
                    // A still-deferred `keyof` (e.g. `keyof (this["arg0"])` or
                    // `keyof T[K]`) is not a usable key space: it cannot reject an
                    // index, so the check defers rather than emitting a spurious
                    // TS2536.
                    || crate::query_boundaries::common::is_keyof_type(self.ctx.types, ty)
            };
            let mut is_deferred_index_type = |ty: TypeId| -> bool {
                ty == TypeId::ERROR
                    || crate::query_boundaries::common::is_conditional_type(self.ctx.types, ty)
                    || crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
                    || self.is_keyof_for_current_object(ty, object_type, object_type_for_check)
            };
            // Suppress TS2536 for deferred types (conditional, application, keyof,
            // error, index-access). tsc defers these checks to instantiation time.
            if !foreign_keyof_indexed_constraint
                && (is_deferred_index_type(index_type_for_check)
                || is_deferred_index_type(index_type)
                || (is_deferred_object_type(object_type_for_check)
                    && key_space_is_unresolved(keyof_object)
                    && !index_is_concrete_literal)
                || (is_deferred_object_type(object_type)
                    && key_space_is_unresolved(keyof_object)
                    && !index_is_concrete_literal)
                || (self.is_deferred_indexed_access_object(object_type_for_check)
                    && key_space_is_unresolved(keyof_object)
                    && !index_is_concrete_literal)
                // Only fall back to checking the pre-resolution object_type when the
                // resolved type is also still an indexed access. If constraint resolution
                // produced a concrete type (e.g., T['value'] → number), trust it.
                || (crate::query_boundaries::common::is_index_access_type(self.ctx.types, object_type_for_check)
                    && self.is_deferred_indexed_access_object(object_type)
                    && key_space_is_unresolved(keyof_object)
                    && !index_is_concrete_literal)
                || crate::query_boundaries::common::is_index_access_type(self.ctx.types, index_type_for_check)
                || crate::query_boundaries::common::is_index_access_type(self.ctx.types, index_type))
            {
                return;
            }
            // The gate above intentionally lets a concrete-literal (or
            // literal-union) index defeat suppression for a deferred object
            // (`!index_is_concrete_literal`). For a deferred *conditional* base
            // that is wrong: tsc validates the literal key against the
            // conditional's base constraint (the union of both branch results).
            // Validate against that branch-union constraint here so a key that
            // lies in every branch is accepted while a missing key still emits
            // TS2536.
            if !foreign_keyof_indexed_constraint
                && self.deferred_conditional_index_key_is_valid(
                    object_type,
                    object_type_for_check,
                    index_type_for_check,
                )
            {
                return;
            }
            // A nested deferred indexed access `Cond<T>[k1][k2]`: the inner
            // `Cond<T>[k1]` is a generic indexed access whose apparent type (the
            // conditional's branch-union constraint indexed by `k1`) carries a
            // concrete key space. Validate the outer literal key `k2` against it,
            // matching tsc's `getConstraintOfIndexedAccessType` — `length`/`0`/
            // array methods are accepted, a missing key still emits TS2536.
            if !foreign_keyof_indexed_constraint
                && self.deferred_indexed_access_conditional_key_is_valid(
                    object_type,
                    object_type_for_check,
                    index_type_for_check,
                )
            {
                return;
            }
            if self.index_has_keyof_constraint_from_declaration(
                data.index_type,
                data.object_type,
                object_type,
                object_type_for_check,
            ) {
                return;
            }
            // Check if we're inside a conditional type's true branch where the condition
            // narrows the index to `keyof T`. E.g., `key extends keyof T ? T[key] : never`.
            if self.is_in_conditional_keyof_narrowing_context(
                node_idx,
                object_type,
                object_type_for_check,
                index_type,
            ) {
                return;
            }

            if let Some(prop_atom) = crate::query_boundaries::common::string_literal_value(
                self.ctx.types,
                index_type_for_check,
            ) {
                let property_name = self.ctx.types.resolve_atom(prop_atom);
                if self.union_restricted_literal_property_is_missing(
                    &property_name,
                    object_type_for_check,
                ) {
                    // Suppress TS2339 for types containing type parameters,
                    // index access types, or deferred types that cannot be resolved.
                    // Check both the resolved type and the original type.
                    let should_suppress =
                        crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            object_type_for_check,
                        ) || crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            object_type_for_check,
                        ) || crate::query_boundaries::common::is_conditional_type(
                            self.ctx.types,
                            object_type_for_check,
                        ) || object_type_for_check == TypeId::UNKNOWN
                            || object_type_for_check == TypeId::ERROR
                            || crate::query_boundaries::common::is_index_access_type(
                                self.ctx.types,
                                object_type,
                            )
                            || crate::query_boundaries::common::contains_type_parameters(
                                self.ctx.types,
                                object_type,
                            )
                            || crate::query_boundaries::diagnostics::contains_index_access_type(
                                self.ctx.types,
                                object_type_for_check,
                            )
                            || crate::query_boundaries::diagnostics::contains_index_access_type(
                                self.ctx.types,
                                object_type,
                            );
                    if !should_suppress {
                        let object_type_str = self
                            .node_text(data.object_type)
                            .map(|text| {
                                let text =
                                    object_format::normalize_indexed_access_object_text(&text);
                                let trimmed = text.trim();
                                let trimmed = trimmed.strip_prefix('(').unwrap_or(trimmed);
                                let trimmed = trimmed.strip_suffix(')').unwrap_or(trimmed);
                                // Strip trailing index access syntax that may leak from
                                // the object_type node span (e.g., "Foo | Bar)['foo']")
                                let trimmed = if let Some(bracket_pos) = trimmed.find(")[") {
                                    trimmed[..bracket_pos].trim()
                                } else if let Some(bracket_pos) = trimmed.find("]['") {
                                    trimmed[..bracket_pos].trim()
                                } else {
                                    trimmed
                                };
                                trimmed.trim().to_string()
                            })
                            .filter(|text| !text.is_empty() && !text.contains('['))
                            .unwrap_or_else(|| self.format_type(object_type));
                        let message = format_message(
                            diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            &[property_name.as_str(), &object_type_str],
                        );
                        self.error_at_node(
                            concrete_error_anchor,
                            &message,
                            diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        );
                    }
                    return;
                }
                // Don't trust property access results on deferred types (indexed
                // access, conditional, generic application) — the solver may
                // spuriously report success on types it can't fully resolve.
                if !crate::query_boundaries::common::is_index_access_type(
                    self.ctx.types,
                    object_type_for_check,
                ) && !crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    object_type_for_check,
                ) && !crate::query_boundaries::common::is_generic_application(
                    self.ctx.types,
                    object_type_for_check,
                ) && matches!(
                    self.resolve_property_access_with_env(object_type_for_check, &property_name),
                    tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                ) {
                    return;
                }
                // For conditional types like Extract<X, Y> (i.e., X extends Y ? X : never),
                // check if the property exists on the check_type or extends_type.
                // When false_type is `never`, the result is always a subtype of check_type
                // (which extends extends_type), so if either has the property it's valid.
                // This handles patterns like Extract<TDef[I], FieldDefinition>["type"]
                // where FieldDefinition has a "type" property.
                if let Some(cond_id) = crate::query_boundaries::common::get_conditional_type_id(
                    self.ctx.types,
                    object_type_for_check,
                ) {
                    let cond = self.ctx.types.conditional_type(cond_id);
                    if cond.false_type == TypeId::NEVER {
                        // Check extends_type first (common for Extract/Filter patterns)
                        let extends_eval = self.evaluate_type_with_env(cond.extends_type);
                        if !crate::query_boundaries::common::is_conditional_type(
                            self.ctx.types,
                            extends_eval,
                        ) && !crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            extends_eval,
                        ) && matches!(
                            self.resolve_property_access_with_env(extends_eval, &property_name),
                            tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                        ) {
                            return;
                        }
                        // Check check_type (handles cases where check_type's constraint
                        // has the property but extends_type doesn't)
                        let check_eval = self.evaluate_type_with_env(cond.check_type);
                        if !crate::query_boundaries::common::is_conditional_type(
                            self.ctx.types,
                            check_eval,
                        ) && !crate::query_boundaries::common::is_generic_application(
                            self.ctx.types,
                            check_eval,
                        ) && !crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            check_eval,
                        ) && matches!(
                            self.resolve_property_access_with_env(check_eval, &property_name),
                            tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                        ) {
                            return;
                        }
                        // Also check the constraint of the check_type (for generic
                        // patterns like TDef[number] where TDef: readonly FieldDefinition[])
                        let check_constraint =
                            crate::query_boundaries::common::type_parameter_constraint(
                                self.ctx.types,
                                check_eval,
                            );
                        if let Some(constraint) = check_constraint {
                            let constraint_eval = self.evaluate_type_with_env(constraint);
                            if !crate::query_boundaries::common::is_conditional_type(
                                self.ctx.types,
                                constraint_eval,
                            ) && !crate::query_boundaries::common::is_generic_application(
                                self.ctx.types,
                                constraint_eval,
                            ) && matches!(
                                self.resolve_property_access_with_env(
                                    constraint_eval,
                                    &property_name
                                ),
                                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                            ) {
                                return;
                            }
                        }
                    }
                }
            }

            // When the original index type is a type parameter (e.g., T extends keyof A
            // used to index B where A != B), don't decompose its constraint into concrete
            // members — emit TS2536 for the type parameter itself. tsc reports
            // "Type 'T' cannot be used to index type 'B'" rather than per-member TS2339.
            let original_index_is_type_param =
                crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, index_type);
            if !original_index_is_type_param
                && self.try_emit_concrete_index_access_error(
                    concrete_error_anchor,
                    object_type_for_check,
                    index_type_for_check,
                    self.type_node_refers_to_type_parameter(data.object_type),
                )
            {
                return;
            }

            // Concrete-tuple base indexed by a generic type-param chain
            // (`Table[D1][0]`, `AddDigitTable[Carry][T][U]`): `tsc` resolves the
            // chain to the tuple's element-value union (`Base[number]`), whose
            // key-space accepts the outer index. The generic recovery above keys
            // off `Base[keyof Base]`, which pollutes the value union with
            // `length`/array-method values for a tuple base and so spuriously
            // rejects the element index. Derive the element-value key-space
            // directly, validating each intermediate index against the tuple's
            // numeric index domain so out-of-range / `keyof`-based inner
            // constraints still emit the genuine `TS2536`.
            if self.generic_tuple_chain_index_access_allows_index(
                data.object_type,
                data.index_type,
                index_type,
            ) {
                return;
            }

            let obj_type_str = self.format_ts2536_object_type(data.object_type, object_type);
            let evaluated_index_type = self.evaluate_type_for_assignability(index_type);
            let prefer_evaluated_index = (evaluated_index_type != TypeId::ERROR
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    index_type,
                ))
                || (evaluated_index_type != index_type
                    && crate::query_boundaries::common::is_keyof_type(self.ctx.types, index_type)
                    && crate::query_boundaries::common::contains_keyof_type(
                        self.ctx.types,
                        evaluated_index_type,
                    ));
            let index_type_str = if prefer_evaluated_index {
                self.format_type(evaluated_index_type)
            } else {
                self.format_type(index_type)
            };

            // Last resort: when the object type is an indexed access Obj[K] where Obj
            // is a concrete type, evaluate the union of all value types and check if
            // the index literal is valid. This handles patterns like:
            //   { [K in keyof Obj]: Obj[K]['name'] }
            // where Obj has an `as` clause or other constructs that prevent the solver
            // from resolving Obj[K] with a generic K.
            if let Some((base_obj, _base_idx)) = crate::query_boundaries::common::index_access_types(
                self.ctx.types,
                object_type_for_check,
            ) {
                if self.indexed_access_constraint_values_allow_index(base_obj, index_type_for_check)
                {
                    return;
                }

                let eval_base = self.evaluate_type_with_env(base_obj);
                let is_concrete = !crate::query_boundaries::common::is_type_parameter_like(
                    self.ctx.types,
                    eval_base,
                ) && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    eval_base,
                ) && !crate::query_boundaries::common::is_index_access_type(
                    self.ctx.types,
                    eval_base,
                ) && !crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    eval_base,
                ) && !crate::query_boundaries::common::is_generic_application(
                    self.ctx.types,
                    eval_base,
                );
                if is_concrete {
                    let keyof_base = self.ctx.types.evaluate_keyof(eval_base);
                    let values_union = self.evaluate_type_with_env(
                        crate::query_boundaries::type_checking::type_checking_index_access(
                            self.ctx.types,
                            eval_base,
                            keyof_base,
                        ),
                    );
                    if values_union != TypeId::ERROR
                        && values_union != TypeId::UNDEFINED
                        && !crate::query_boundaries::common::is_index_access_type(
                            self.ctx.types,
                            values_union,
                        )
                    {
                        // Check if the index is a valid key of the values union
                        let keyof_values = self.ctx.types.evaluate_keyof(values_union);
                        if self
                            .indexed_access_key_space_relation_outcome(
                                index_type_for_check,
                                keyof_values,
                            )
                            .related
                        {
                            return;
                        }
                        // Also try property access for string literal indices
                        if let Some(prop_atom) =
                            crate::query_boundaries::common::string_literal_value(
                                self.ctx.types,
                                index_type_for_check,
                            )
                        {
                            let property_name = self.ctx.types.resolve_atom(prop_atom);
                            if matches!(
                                self.resolve_property_access_with_env(values_union, &property_name),
                                tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                            ) {
                                return;
                            }
                        }
                    }
                }
            }
            if object_type != object_type_for_check
                && let Some((base_obj, _base_idx)) =
                    crate::query_boundaries::common::index_access_types(self.ctx.types, object_type)
                && self.indexed_access_constraint_values_allow_index(base_obj, index_type_for_check)
            {
                return;
            }

            // tsc emits TS2536 only when the index type is itself a type parameter (or the object type is
            // generic/deferred). For a *concrete* object type indexed by a *missing
            // literal* key, tsc instead reports the property as missing (TS2339) —
            // uniformly across object literals, interfaces, classes, and unions. When the index
            // is not a type parameter, the object type carries no type parameters
            // (so type-parameter-like, generic index-access/conditional/application
            // objects are all excluded), the original object node is not itself a
            // type parameter, and the index resolves to a literal key, emit TS2339
            // so anonymous object literals and union/function-typed accesses match
            // tsc instead of falling through to a spurious TS2536.
            // Dedup merges this with any TS2339 the property path already produced
            // for the same key and location.
            if !original_index_is_type_param
                && !self.type_node_refers_to_type_parameter(data.object_type)
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    object_type_for_check,
                )
                // The *unevaluated* object type must also be non-generic. A
                // generic indexed-access base like `T[K1]` (with type-parameter
                // key) evaluates to a concrete union that loses the type
                // parameter, but tsc keeps it deferred and reports the missing
                // literal key as TS2536 (`'"nope"' cannot be used to index type
                // 'T[K1]'`), not TS2339. Falling through to the TS2536 emission
                // below preserves that classification.
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    object_type,
                )
                && let Some(key_atom) =
                    crate::query_boundaries::type_computation::access::literal_property_name(
                        self.ctx.types,
                        index_type_for_check,
                    )
            {
                let key_name = self.ctx.types.resolve_atom(key_atom);
                // Only report the key as missing if it genuinely does not resolve
                // to a property. Some valid accesses (e.g. an enum member via
                // `(typeof Enum)["Member"]`) can reach this fallback through
                // unrelated resolution gaps; emitting here would turn a valid
                // access into a spurious error. A concrete object indexed by a
                // concrete literal key is never a TS2536 in tsc, so do
                // not fall through to the TS2536 emission below.
                if !matches!(
                    self.resolve_property_access_with_env(object_type_for_check, &key_name),
                    tsz_solver::operations::property::PropertyAccessResult::Success { .. }
                ) {
                    let message = format_message(
                        diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        &[key_name.as_str(), &obj_type_str],
                    );
                    self.error_at_node(
                        concrete_error_anchor,
                        &message,
                        diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                    );
                }
                return;
            }

            let message_2536 = format_message(
                diagnostic_messages::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
                &[&index_type_str, &obj_type_str],
            );
            self.error_at_node(
                error_anchor,
                &message_2536,
                diagnostic_codes::TYPE_CANNOT_BE_USED_TO_INDEX_TYPE,
            );
        }
    }
}
