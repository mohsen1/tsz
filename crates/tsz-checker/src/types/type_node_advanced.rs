//! Advanced Type Node Handlers
//!
//! This module contains handlers for advanced/derived type constructs:
//! - Type operators (readonly, keyof, unique)
//! - Indexed access types (T[K], Person["name"])
//! - Type queries (typeof X)
//! - Mapped types ({ [P in K]: T })

mod enum_indexed_access;
mod indexed_access_fast_path;
mod indexed_access_type;
mod type_query_declared_type;

use super::type_node::TypeNodeChecker;
use super::type_node_helpers::{
    get_string_literal_from_type_index, is_type_query_in_non_flow_sensitive_signature_parameter,
    is_typeof_global_this_type_node,
};
use super::unique_symbol_construction::{
    readonly_operator_result, unique_symbol_type_for_operator,
};
use crate::query_boundaries::type_query_construction;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    // =========================================================================
    // Type Operators
    // =========================================================================

    /// Get type from a type operator node (readonly T[], readonly [T, U], unique symbol).
    ///
    /// Handles type modifiers like:
    /// - `readonly T[]` - Creates `ReadonlyType` wrapper
    /// - `unique symbol` - Special marker for unique symbols
    pub(super) fn get_type_from_type_operator(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_scanner::SyntaxKind;
        let factory = self.ctx.types.factory();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        if let Some(type_op) = self.ctx.arena.get_type_operator(node) {
            let operator = type_op.operator;
            let inner_type = self.check(type_op.type_node);

            // Handle readonly operator
            if operator == SyntaxKind::ReadonlyKeyword as u16 {
                let operand_kind = self.ctx.arena.get(type_op.type_node).map(|n| n.kind);
                // TS1354: 'readonly' type modifier is only permitted on array and tuple literal types.
                // A missing operand (`readonly ;`) already produced TS1110 `Type expected`
                // in the parser; tsc does not also report the array/tuple grammar error.
                if operand_kind.is_some()
                    && !operand_kind.is_some_and(syntax_kind_ext::is_array_or_tuple_type)
                    && !self
                        .ctx
                        .arena
                        .is_missing_recovery_identifier(type_op.type_node)
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.ctx.error(
                        node.pos,
                        node.end.saturating_sub(node.pos),
                        diagnostic_messages::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES.to_string(),
                        diagnostic_codes::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES,
                    );
                }
                // Readonly is transparent unless the operand is a syntactic
                // array/tuple literal type; see `readonly_operator_result`.
                return readonly_operator_result(self.ctx, operand_kind, inner_type);
            }

            // Handle keyof operator
            if operator == SyntaxKind::KeyOfKeyword as u16 {
                if let Some(operand_node) = self.ctx.arena.get(type_op.type_node)
                    && operand_node.kind == syntax_kind_ext::TYPE_REFERENCE
                    && let Some(type_ref) = self.ctx.arena.get_type_ref(operand_node)
                    && let Some(imported_operand) =
                        self.import_call_type_reference(type_ref.type_name)
                {
                    let evaluated = self.ctx.types.evaluate_keyof(imported_operand);
                    if evaluated != TypeId::ERROR {
                        return evaluated;
                    }
                }
                // tsc's `getIndexType` resolves a `any`/`unknown`/`never` operand
                // to its fixed key-space result (`string | number | symbol` /
                // `never` / `string | number | symbol`) immediately, rather than
                // building a deferred `IndexType` node — the operator itself
                // never survives to the printer for these operands, so a bare
                // `keyof any` annotation displays as the resolved union, not
                // `keyof any`. Other operands (object/interface/generic) keep
                // the deferred `KeyOf` node so the printer still shows `keyof T`.
                if matches!(inner_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::NEVER) {
                    return self.ctx.types.evaluate_keyof(inner_type);
                }
                return factory.keyof(inner_type);
            }

            // Handle unique operator
            if operator == SyntaxKind::UniqueKeyword as u16 {
                if inner_type == TypeId::SYMBOL {
                    return unique_symbol_type_for_operator(self.ctx, idx, node.pos, node.end);
                }
                return inner_type;
            }

            // Unknown operator - return inner type
            inner_type
        } else {
            TypeId::ERROR
        }
    }

    // =========================================================================
    // Type Query (typeof)
    // =========================================================================

    pub(crate) fn apply_instantiation_expression_type_arguments(
        &mut self,
        expr_type: TypeId,
        type_arguments: &NodeList,
    ) -> TypeId {
        if self
            .instantiation_expression_applicability_error_type(
                expr_type,
                type_arguments.nodes.len(),
            )
            .is_some()
        {
            return TypeId::ERROR;
        }

        let type_args: Vec<TypeId> = type_arguments
            .nodes
            .iter()
            .map(|&arg_idx| self.check(arg_idx))
            .collect();
        if type_args.is_empty() {
            return expr_type;
        }

        let application = self.ctx.types.application(expr_type, type_args);
        let evaluated = crate::query_boundaries::state::type_environment::evaluate_type_with_cache(
            self.ctx.types,
            &*self.ctx,
            application,
            std::iter::empty(),
            false,
            crate::query_boundaries::state::type_environment::EvaluateTypeWithCacheOptions {
                expand_application_display_alias_args: self.ctx.is_declaration_file()
                    || self.ctx.emit_declarations(),
                query_db: Some(self.ctx.types),
                authoritative: true,
                cache_entry_collection:
                    crate::query_boundaries::state::type_environment::CacheEntryCollection::Skip,
            },
        )
        .result;
        if evaluated != TypeId::ERROR && evaluated != application {
            evaluated
        } else {
            application
        }
    }

    fn instantiation_expression_applicability_error_type(
        &self,
        expr_type: TypeId,
        type_argument_count: usize,
    ) -> Option<TypeId> {
        if expr_type == TypeId::ERROR || expr_type == TypeId::ANY {
            return None;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, expr_type)
        {
            let mut invalid = Vec::new();
            let mut saw_applicable = false;
            let mut saw_signature = false;
            for member in members.iter().copied() {
                let has_applicable =
                    self.instantiation_type_has_applicable_signature(member, type_argument_count);
                saw_applicable |= has_applicable;
                let has_signature = self.instantiation_type_has_signature(member);
                saw_signature |= has_signature;
                if !has_applicable && has_signature {
                    invalid.push(member);
                }
            }
            if saw_applicable && invalid.is_empty() {
                return None;
            }
            if saw_applicable {
                return if invalid.len() == 1 {
                    invalid.first().copied()
                } else {
                    Some(self.ctx.types.union(invalid))
                };
            }
            return if !saw_signature || invalid.len() == members.len() {
                Some(expr_type)
            } else if invalid.len() == 1 {
                invalid.first().copied()
            } else {
                Some(self.ctx.types.union(invalid))
            };
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, expr_type)
        {
            if members.iter().copied().any(|member| {
                self.instantiation_type_has_applicable_signature(member, type_argument_count)
            }) {
                return None;
            }
            return Some(expr_type);
        }

        if self.instantiation_type_has_applicable_signature(expr_type, type_argument_count) {
            None
        } else {
            Some(expr_type)
        }
    }

    fn instantiation_type_has_applicable_signature(
        &self,
        type_id: TypeId,
        type_argument_count: usize,
    ) -> bool {
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            && shape.type_params.len() == type_argument_count
        {
            return true;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, type_id)
            && sigs
                .iter()
                .any(|sig| sig.type_params.len() == type_argument_count)
        {
            return true;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, type_id)
            && sigs
                .iter()
                .any(|sig| sig.type_params.len() == type_argument_count)
        {
            return true;
        }
        false
    }

    fn instantiation_type_has_signature(&self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
            .is_some()
        {
            return true;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, type_id)
            && !sigs.is_empty()
        {
            return true;
        }
        if let Some(sigs) =
            crate::query_boundaries::common::construct_signatures_for_type(self.ctx.types, type_id)
            && !sigs.is_empty()
        {
            return true;
        }
        false
    }

    /// Get type from a type query node (typeof X).
    ///
    /// Creates a `TypeQuery` type that captures the type of a value.
    pub(crate) fn get_type_from_type_query(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_lowering::TypeLowering;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return TypeId::ERROR;
        };

        // Route inline `typeof import("...")[.segments]` through the namespace-
        // aware resolver before falling through to lowering. See
        // `try_get_type_from_inline_import_typeof_query` for the full rule.
        if let Some(resolved) = self.try_get_type_from_inline_import_typeof_query(idx) {
            return resolved;
        }

        // Capture type argument node indices early (before borrows prevent access).
        // When present, the base type will be wrapped in Application(base, args)
        // so that constraint checking (TS2344) sees the instantiated type rather
        // than the raw function type. This matches tsc behavior: `typeof fn<Args>`
        // produces an instantiation expression type, not the original function type.
        let type_arguments = type_query.type_arguments.clone();
        let use_flow_sensitive_query =
            !is_type_query_in_non_flow_sensitive_signature_parameter(self.ctx.arena, idx);

        // `default` is a reserved keyword and cannot be used as an identifier in
        // expression position. `typeof default` must always report TS2304 even when
        // the file has an `export default` declaration, because the default-export
        // binding is not a locally-visible value name. This check must come BEFORE
        // the node_types cache lookup, which may have a cached type for the `default`
        // identifier node from a prior expression-space visit.
        if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && self
                .ctx
                .arena
                .get_identifier(expr_node)
                .is_some_and(|id| id.escaped_text == "default")
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let msg = format_message(diagnostic_messages::CANNOT_FIND_NAME, &["default"]);
            self.ctx.error(
                expr_node.pos,
                expr_node.end - expr_node.pos,
                msg,
                diagnostic_codes::CANNOT_FIND_NAME,
            );
            return TypeId::ERROR;
        }

        // Type parameter constraints cannot reference function parameters of the
        // same function via `typeof`. Emit TS2304/TS2552 instead of silently resolving.
        // This check MUST come before the node_types cache lookup, because
        // destructured parameter bindings (e.g., `{a}` in `({a}: {a:T})`) may
        // have their type cached during binding pattern processing. Without this
        // priority, `typeof a` would return the cached type instead of ERROR,
        // causing the type parameter constraint to be self-referential instead
        // of ERROR, which then leads to cascading TS2339 diagnostics.
        if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
            && self
                .ctx
                .type_param_constraint_excluded_params
                .contains(ident.escaped_text.as_str())
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let msg = format_message(
                diagnostic_messages::CANNOT_FIND_NAME,
                &[&ident.escaped_text],
            );
            self.ctx.error(
                expr_node.pos,
                expr_node.end - expr_node.pos,
                msg,
                diagnostic_codes::CANNOT_FIND_NAME,
            );
            return TypeId::ERROR;
        }

        let name_opt = if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name) {
            if expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                self.ctx
                    .arena
                    .get_identifier(expr_node)
                    .map(|id| id.escaped_text.as_str())
            } else {
                None
            }
        } else {
            None
        };

        if name_opt == Some("default") {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let msg = format_message(diagnostic_messages::CANNOT_FIND_NAME, &["default"]);
            let expr_node = self
                .ctx
                .arena
                .get(type_query.expr_name)
                .expect("type_query.expr_name node exists");
            self.ctx.error(
                expr_node.pos,
                expr_node.end - expr_node.pos,
                msg,
                diagnostic_codes::CANNOT_FIND_NAME,
            );
            return TypeId::ERROR;
        }

        // Check typeof_param_scope — resolves `typeof paramName` in return type
        // annotations where the parameter isn't a file-level binding.
        if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name)
            && expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
            && let Some(&param_type) = self.ctx.typeof_param_scope.get(ident.escaped_text.as_str())
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(param_type, type_arguments);
            }
            return param_type;
        }

        // A parameter binding pattern of the enclosing signature declares real
        // locals, and they are in scope for that signature's type positions.
        // `typeof_param_scope` above carries their computed types when the
        // signature's return type is being lowered; this answers the same
        // question structurally for every other traversal, so the query
        // resolves instead of reaching global name resolution and reporting
        // TS2304 — or TS2693, when the binding shadows a global type name such
        // as `string`. Without a seeded type the binding is `any`, which is
        // what an unannotated destructuring parameter contributes anyway.
        if let Some(name) = name_opt
            && crate::types_domain::signature_binding_scope::signature_parameter_declares_binding(
                self.ctx, idx, name,
            )
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(TypeId::ANY, type_arguments);
            }
            return TypeId::ANY;
        }

        if let Some(tuple_type) = self.const_asserted_array_tuple_type_query(type_query.expr_name) {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(tuple_type, type_arguments);
            }
            return tuple_type;
        }

        if let Some(object_type) = self.const_array_to_enum_object_type_query(type_query.expr_name)
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(object_type, type_arguments);
            }
            return object_type;
        }

        if let Some(literal_type) =
            self.const_object_member_literal_type_query(type_query.expr_name)
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(literal_type, type_arguments);
            }
            return literal_type;
        }

        if let Some(property_type) = self.value_property_type_query(type_query.expr_name) {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(property_type, type_arguments);
            }
            return property_type;
        }

        if use_flow_sensitive_query
            && let Some(&expr_type) = self.ctx.node_types.get(&type_query.expr_name.0)
            && expr_type != TypeId::ERROR
            // Don't use a cached enum-union type as the result of `typeof EnumDeclaration`.
            // The flow-sensitive pre-resolution caches the symbol's declared type (the union
            // 0|1|2|3) under the identifier node, but `typeof EnumName` must resolve to the
            // namespace type (the object with member properties). Skip the cache so the ENUM
            // branch below can return the correct TypeQuery instead.
            && crate::query_boundaries::common::enum_def_id(self.ctx.types, expr_type).is_none()
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(expr_type, type_arguments);
            }
            return expr_type;
        }

        if let Some(sym_id) = self.resolve_type_query_symbol(type_query.expr_name) {
            let (sym_flags, type_only_name) =
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .map_or((0 /* no symbol */, None), |s| {
                        let has_value = s.has_any_flags(tsz_binder::symbol_flags::VALUE);
                        let is_type_only =
                            s.has_any_flags(tsz_binder::symbol_flags::TYPE) && !has_value;
                        (s.flags, is_type_only.then(|| s.escaped_name.clone()))
                    });
            if let Some(escaped_name) = type_only_name {
                self.emit_type_query_type_only_error(&escaped_name, type_query.expr_name);
                return TypeId::ERROR;
            }

            // `typeof EnumDeclaration` must resolve to the enum namespace type (the object
            // `{ Up: 0, Down: 1, … }`), not to the union type stored in `symbol_types`.
            // Return the cached namespace type when available, otherwise defer via TypeQuery
            // so the solver can resolve it on demand. This is separate from enum-member types
            // (`Direction.Up`) and variables typed as an enum (`x: Direction`), which both
            // carry the union/literal type and must NOT be intercepted here.
            if sym_flags & tsz_binder::symbol_flags::ENUM != 0
                && sym_flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0
            {
                if let Some(&ns_type) = self.ctx.enum_namespace_types.get(&sym_id) {
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            ns_type,
                            type_arguments,
                        );
                    }
                    return ns_type;
                }
                let factory = self.ctx.types.factory();
                let base = factory.type_query(tsz_solver::SymbolRef(sym_id.0));
                if let Some(type_arguments) = &type_arguments {
                    return self
                        .apply_instantiation_expression_type_arguments(base, type_arguments);
                }
                return base;
            }

            if sym_flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                && sym_flags & tsz_binder::symbol_flags::VALUE != 0
            {
                if let Some(&val_type) = self.ctx.merged_value_types.get(&sym_id) {
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            val_type,
                            type_arguments,
                        );
                    }
                    return val_type;
                }

                if let Some(ann_idx) = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                    let mut decl = symbol.value_declaration;
                    let decl_node = self.ctx.arena.get(decl)?;
                    if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                        decl = self.ctx.arena.get_extended(decl)?.parent;
                    }
                    let decl_node = self.ctx.arena.get(decl)?;
                    if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                        return None;
                    }
                    let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
                    var_decl
                        .type_annotation
                        .is_some()
                        .then_some(var_decl.type_annotation)
                }) {
                    let ann_type = self.check(ann_idx);
                    if ann_type != TypeId::ERROR && ann_type != TypeId::ANY {
                        if let Some(type_arguments) = &type_arguments {
                            return self.apply_instantiation_expression_type_arguments(
                                ann_type,
                                type_arguments,
                            );
                        }
                        return ann_type;
                    }
                }

                if let Some(val_type) = self.compute_safe_merged_value_type_for_type_query(sym_id) {
                    self.ctx.merged_value_types.insert(sym_id, val_type);
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            val_type,
                            type_arguments,
                        );
                    }
                    return val_type;
                }
            }

            // For a value symbol merged with an interface (declaration merging,
            // e.g. lib globals `interface Date {} declare var Date: DateConstructor`
            // or `interface Foo {} declare var Foo: {...}`), `symbol_types[sym_id]`
            // holds the TYPE-space (instance) type, not the VALUE-space type a
            // `typeof` query needs. The value declaration (the `var`) carries the
            // value-space type, so resolve through its annotation first; only fall
            // back to `symbol_types` when the value side has no annotation. Without
            // this, a nested `(typeof Date)['now']` / `[typeof Foo][0]` resolves the
            // deferred query against the instance and reports phantom TS2339s.
            let merged_value_with_interface = sym_flags & tsz_binder::symbol_flags::VALUE != 0
                && sym_flags & tsz_binder::symbol_flags::INTERFACE != 0;

            let mut declared_type: Option<TypeId> = if merged_value_with_interface {
                self.declared_annotation_type_for_type_query_symbol(sym_id)
            } else if sym_flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0
                && self.ctx.symbol_resolution_set.contains(&sym_id)
            {
                None
            } else {
                self.ctx
                    .symbol_types
                    .get(&sym_id)
                    .filter(|&t| t != TypeId::ANY && t != TypeId::ERROR)
            };

            if declared_type.is_none() {
                declared_type = self.declared_annotation_type_for_type_query_symbol(sym_id);
            }

            if let Some(declared_type) = declared_type
                && declared_type != TypeId::ANY
                && declared_type != TypeId::ERROR
            {
                if crate::query_boundaries::common::is_unique_symbol_type(
                    self.ctx.types,
                    declared_type,
                ) {
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            declared_type,
                            type_arguments,
                        );
                    }
                    return declared_type;
                }

                if !use_flow_sensitive_query {
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            declared_type,
                            type_arguments,
                        );
                    }
                    return declared_type;
                }

                // Find a flow node at or above the expression name for narrowing.
                let flow_node = self
                    .ctx
                    .binder
                    .get_node_flow(type_query.expr_name)
                    .or_else(|| {
                        // Walk up parents to find a flow node (type position nodes
                        // often don't have direct flow links).
                        let mut current = self
                            .ctx
                            .arena
                            .get_extended(type_query.expr_name)
                            .map(|ext| ext.parent);
                        while let Some(parent) = current {
                            if parent.is_none() {
                                break;
                            }
                            if let Some(flow) = self.ctx.binder.get_node_flow(parent) {
                                return Some(flow);
                            }
                            current = self.ctx.arena.parent_of(parent);
                        }
                        None
                    });

                if let Some(flow_node) = flow_node {
                    let analyzer = crate::FlowAnalyzer::from_ctx(self.ctx);

                    let narrowed =
                        analyzer.get_flow_type(type_query.expr_name, declared_type, flow_node);
                    if narrowed != TypeId::ERROR {
                        if let Some(type_arguments) = &type_arguments {
                            return self.apply_instantiation_expression_type_arguments(
                                narrowed,
                                type_arguments,
                            );
                        }
                        return narrowed;
                    }
                }
            }

            if let Some(value_type) = self.declared_type_for_type_query_symbol(sym_id) {
                if let Some(type_arguments) = &type_arguments {
                    return self
                        .apply_instantiation_expression_type_arguments(value_type, type_arguments);
                }
                return value_type;
            }

            let factory = self.ctx.types.factory();
            let base = factory.type_query(tsz_solver::SymbolRef(sym_id.0));
            if let Some(type_arguments) = &type_arguments {
                return self.apply_instantiation_expression_type_arguments(base, type_arguments);
            }
            return base;
        }

        // For qualified/generic typeof expressions (typeof A.B, typeof A<B>),
        // check if the root identifier exists. If not, emit TS2304.
        if name_opt.is_none() {
            use tsz_parser::parser::syntax_kind_ext;
            let mut root_idx = type_query.expr_name;
            while let Some(node) = self.ctx.arena.get(root_idx) {
                if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                    if let Some(qn) = self.ctx.arena.get_qualified_name(node) {
                        root_idx = qn.left;
                        continue;
                    }
                    break;
                }
                if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    if let Some(access) = self.ctx.arena.get_access_expr(node) {
                        root_idx = access.expression;
                        continue;
                    }
                    break;
                }
                break;
            }
            if let Some(root_node) = self.ctx.arena.get(root_idx)
                && root_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(root_ident) = self.ctx.arena.get_identifier(root_node)
            {
                let root_name = root_ident.escaped_text.as_str();
                let is_global_name = matches!(
                    root_name,
                    "undefined" | "NaN" | "Infinity" | "globalThis" | "arguments"
                );
                if !is_global_name
                    && self
                        .ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, root_idx)
                        .is_none()
                    && !self.ctx.typeof_param_scope.contains_key(root_name)
                {
                    use crate::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let msg = format_message(diagnostic_messages::CANNOT_FIND_NAME, &[root_name]);
                    self.ctx.error(
                        root_node.pos,
                        root_node.end - root_node.pos,
                        msg,
                        diagnostic_codes::CANNOT_FIND_NAME,
                    );
                    return TypeId::ERROR;
                }
            }
        }

        // For simple identifiers, try full scope resolution (including function params,
        // local variables, etc.) before falling back to lowering.
        if let Some(name) = name_opt {
            if let Some(sym_id) = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, type_query.expr_name)
            {
                // TS2693: typeof requires a value binding (same check as above).
                if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                    let flags = symbol.flags;
                    let has_value = flags & tsz_binder::symbol_flags::VALUE != 0;
                    let is_type_only = (flags & tsz_binder::symbol_flags::TYPE != 0) && !has_value;
                    if is_type_only {
                        self.emit_type_query_type_only_error(name, type_query.expr_name);
                        return TypeId::ERROR;
                    }
                }
                if !use_flow_sensitive_query
                    && let Some(declared_type) = self.declared_type_for_type_query_symbol(sym_id)
                {
                    if let Some(type_arguments) = &type_arguments {
                        return self.apply_instantiation_expression_type_arguments(
                            declared_type,
                            type_arguments,
                        );
                    }
                    return declared_type;
                }
                let factory = self.ctx.types.factory();
                let base = factory.type_query(tsz_solver::SymbolRef(sym_id.0));
                if let Some(type_arguments) = &type_arguments {
                    return self
                        .apply_instantiation_expression_type_arguments(base, type_arguments);
                }
                return base;
            }
            // Skip TS2304 for well-known globals that may not be in local binder scope
            // but are valid in typeof position (undefined, NaN, Infinity, globalThis, etc.)
            let is_global_name = matches!(
                name,
                "undefined" | "NaN" | "Infinity" | "globalThis" | "arguments"
            );
            if name == "globalThis" {
                return self.get_global_this_type(type_query.expr_name);
            } else if is_global_name {
                // Fall through to TypeLowering
            } else {
                // Name not found in any scope — emit TS2304
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let msg = format_message(diagnostic_messages::CANNOT_FIND_NAME, &[name]);
                if let Some(expr_node) = self.ctx.arena.get(type_query.expr_name) {
                    self.ctx.error(
                        expr_node.pos,
                        expr_node.end - expr_node.pos,
                        msg,
                        diagnostic_codes::CANNOT_FIND_NAME,
                    );
                }
                return TypeId::ERROR;
            }
        }

        // A qualified value query `typeof a.b` is exactly `(typeof a)["b"]`. The
        // earlier eager paths (value-property resolution, namespace/enum symbol
        // resolution) have not resolved it, and the `None` type-resolver lowering
        // below cannot resolve a qualified value name — it would collapse the query
        // to `error`, which then leaks into an enclosing `keyof`/mapped/indexed
        // operand (e.g. `keyof typeof a.b` rendering as `keyof error`). Defer to the
        // indexed-access form, which resolves the member lazily — correct even when
        // the value object's members are not materialized at the point the query is
        // first forced — and reports TS2339 for a genuinely missing member.
        if name_opt.is_none()
            && let Some(expr_node) = self.ctx.arena.get(type_query.expr_name)
            && matches!(
                expr_node.kind,
                syntax_kind_ext::QUALIFIED_NAME | syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            )
            && let Some(deferred) = self.deferred_typeof_value_chain(type_query.expr_name)
        {
            if let Some(type_arguments) = &type_arguments {
                return self
                    .apply_instantiation_expression_type_arguments(deferred, type_arguments);
            }
            return deferred;
        }

        // Fall back to TypeLowering with proper value resolvers
        let value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let ident = self.ctx.arena.get_identifier_at(node_idx)?;
            let name = ident.escaped_text.as_str();
            if name == "default" {
                return None;
            }
            let sym_id = self.ctx.binder.file_locals.get(name)?;
            Some(sym_id.0)
        };
        let type_resolver = |_node_idx: NodeIndex| -> Option<u32> { None };
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        );

        lowering.lower_type(idx)
    }

    pub(crate) fn const_object_member_literal_type_query(
        &self,
        expr_name: NodeIndex,
    ) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;

        let (base, property_name_node) = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            let access = self.ctx.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            (access.expression, access.name_or_argument)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.ctx.arena.get_qualified_name(node)?;
            (qualified.left, qualified.right)
        } else {
            return None;
        };

        let property_name = self.property_name_text(property_name_node)?;
        let base = self.ctx.arena.skip_parenthesized_and_assertions(base);
        let base_node = self.ctx.arena.get(base)?;
        if base_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.ctx.binder.resolve_identifier(self.ctx.arena, base)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE) {
            return None;
        }

        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.primary_declaration()?
        };
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            decl_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.ctx.arena.is_const_variable_declaration(decl_idx)
        {
            return None;
        }

        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let assertion_expr = self.ctx.arena.skip_parenthesized(decl.initializer);
        let initializer_is_const_assertion = self
            .ctx
            .arena
            .get(assertion_expr)
            .and_then(|node| self.ctx.arena.get_type_assertion(node))
            .and_then(|assertion| self.ctx.arena.get(assertion.type_node))
            .is_some_and(|type_node| type_node.kind == SyntaxKind::ConstKeyword as u16);
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(decl.initializer);
        let init_node = self.ctx.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return self.array_to_enum_member_literal_type(initializer, &property_name);
        }

        let literal = self.ctx.arena.get_literal_expr(init_node)?;
        for &element in &literal.elements.nodes {
            let element_node = self.ctx.arena.get(element)?;
            if element_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_property_assignment(element_node)?;
                if self.property_name_text(prop.name).as_deref() == Some(property_name.as_str()) {
                    let member_type =
                        self.literal_type_from_const_member_initializer(prop.initializer)?;
                    return Some(if initializer_is_const_assertion {
                        member_type
                    } else {
                        crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            member_type,
                        )
                    });
                }
            } else if element_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                let prop = self.ctx.arena.get_shorthand_property(element_node)?;
                if self.property_name_text(prop.name).as_deref() == Some(property_name.as_str()) {
                    let member_type = self.literal_type_from_const_member_initializer(prop.name)?;
                    return Some(if initializer_is_const_assertion {
                        member_type
                    } else {
                        crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            member_type,
                        )
                    });
                }
            }
        }

        None
    }

    pub(crate) fn const_array_to_enum_object_type_query(
        &self,
        expr_name: NodeIndex,
    ) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_name)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE) {
            return None;
        }

        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.primary_declaration()?
        };
        let mut decl_node = self.ctx.arena.get(decl_idx)?;
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            decl_idx = self.ctx.arena.get_extended(decl_idx)?.parent;
            decl_node = self.ctx.arena.get(decl_idx)?;
        }
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !self.ctx.arena.is_const_variable_declaration(decl_idx)
        {
            return None;
        }

        let decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let literal_names = self.array_to_enum_literal_names(decl.initializer)?;
        if literal_names.is_empty() {
            return None;
        }

        let props = literal_names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let literal_type =
                    type_query_construction::const_query_literal_string_type(self.ctx.types, name);
                let name = self.ctx.types.intern_string(name);
                type_query_construction::const_query_readonly_property(
                    name,
                    literal_type,
                    index as u32,
                )
            })
            .collect();

        Some(type_query_construction::const_query_array_to_enum_object_type(self.ctx.types, props))
    }

    fn array_to_enum_member_literal_type(
        &self,
        initializer: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        self.array_to_enum_literal_names(initializer)?
            .into_iter()
            .find(|name| name == property_name)
            .map(|name| {
                type_query_construction::const_query_literal_string_type(self.ctx.types, &name)
            })
    }

    fn array_to_enum_literal_names(&self, initializer: NodeIndex) -> Option<Vec<String>> {
        let initializer = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer);
        // A same-file bare-identifier callee must additionally resolve to an
        // identity mapped-type declaration; a `util.arrayToEnum` property-access
        // callee is accepted on the shared name match alone.
        if let Some(node) = self.ctx.arena.get(initializer)
            && node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(node)
            && !self.array_to_enum_callee_passes_guard(call.expression)
        {
            return None;
        }
        crate::symbols_domain::name_text::array_to_enum_call_literal_names(
            self.ctx.arena,
            initializer,
        )
    }

    fn array_to_enum_callee_passes_guard(&self, callee: NodeIndex) -> bool {
        let callee = self.ctx.arena.skip_parenthesized_and_assertions(callee);
        let Some(node) = self.ctx.arena.get(callee) else {
            return false;
        };
        if self.ctx.arena.get_identifier(node).is_some() {
            return self.array_to_enum_callee_returns_identity_mapped_type(callee);
        }
        true
    }

    fn array_to_enum_callee_returns_identity_mapped_type(&self, callee: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_array_to_enum_callee_symbol(callee) else {
            return false;
        };
        let arena = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .map(|file_idx| self.ctx.get_arena_for_file(file_idx as u32))
            .unwrap_or(self.ctx.arena);
        let Some(symbol) = self.array_to_enum_cross_file_symbol(sym_id) else {
            return false;
        };
        let mut decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            match symbol.primary_declaration() {
                Some(decl) => decl,
                None => return false,
            }
        };
        let mut decl_node = match arena.get(decl_idx) {
            Some(node) => node,
            None => return false,
        };
        if decl_node.kind == SyntaxKind::Identifier as u16 {
            let Some(parent) = arena.get_extended(decl_idx).map(|ext| ext.parent) else {
                return false;
            };
            decl_idx = parent;
            let Some(parent_node) = arena.get(decl_idx) else {
                return false;
            };
            decl_node = parent_node;
        }

        let return_type = if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let Some(decl) = arena.get_variable_declaration(decl_node) else {
                return false;
            };
            let Some(init_node) = arena.get(decl.initializer) else {
                return false;
            };
            let Some(func) = arena.get_function(init_node) else {
                return false;
            };
            func.type_annotation
        } else {
            let Some(func) = arena.get_function(decl_node) else {
                return false;
            };
            func.type_annotation
        };

        crate::symbols_domain::name_text::type_node_is_identity_mapped_type_in_arena(
            arena,
            return_type,
        )
    }

    fn resolve_array_to_enum_callee_symbol(
        &self,
        callee: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let node = self.ctx.arena.get(callee)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.ctx.binder.resolve_identifier(self.ctx.arena, callee);
        }
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        let left_sym = self.resolve_array_to_enum_callee_symbol(access.expression)?;
        let left_symbol = self.array_to_enum_cross_file_symbol(left_sym)?;
        let right_name = self
            .ctx
            .arena
            .get_identifier_at(access.name_or_argument)
            .map(|ident| ident.escaped_text.as_str())?;
        left_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(right_name))
            .or_else(|| {
                left_symbol
                    .members
                    .as_ref()
                    .and_then(|members| members.get(right_name))
            })
    }

    fn array_to_enum_cross_file_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(symbol) = binder.get_symbol(sym_id)
        {
            return Some(symbol);
        }
        self.ctx.binder.get_symbol(sym_id)
    }

    fn get_global_this_type(&mut self, _error_node: NodeIndex) -> TypeId {
        // The synthetic `typeof globalThis` surface is owned by `CheckerContext`
        // so the same object identity is shared with the `typeof` lowering
        // overrides (`Fn`-typed, hence `&self`). See
        // `CheckerContext::global_this_surface_type`.
        self.ctx.global_this_surface_type()
    }

    /// Emit TS2693 for a type-only symbol used in a typeof type query.
    fn emit_type_query_type_only_error(&mut self, name: &str, expr_name: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let msg = format_message(
            diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            &[name],
        );
        if let Some(expr_node) = self.ctx.arena.get(expr_name) {
            self.ctx.error(
                expr_node.pos,
                expr_node.end - expr_node.pos,
                msg,
                diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
            );
        }
    }

    /// Build the deferred `typeof` representation of a value entity name as a
    /// chain of indexed accesses over a base `TypeQuery`.
    ///
    /// `a` (identifier) lowers to `TypeQuery(a)`; `a.b` to `(typeof a)["b"]`;
    /// `a.b.c` to `((typeof a)["b"])["c"]`. The member resolves lazily through
    /// the indexed-access machinery, which — unlike an eager property access on
    /// the already-resolved value object — does not depend on the object's
    /// members being materialized at the point the query is first forced.
    /// Returns `None` when the root does not resolve to a value symbol.
    fn deferred_typeof_value_chain(&self, expr_name: NodeIndex) -> Option<TypeId> {
        let node = self.ctx.arena.get(expr_name)?;
        let factory = self.ctx.types.factory();
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_value_symbol_in_scope(expr_name)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & tsz_binder::symbol_flags::VALUE == 0 {
                return None;
            }
            return Some(factory.type_query(tsz_solver::SymbolRef(sym_id.0)));
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            let base = self.deferred_typeof_value_chain(qn.left)?;
            let right_node = self.ctx.arena.get(qn.right)?;
            let right_ident = self.ctx.arena.get_identifier(right_node)?;
            let index_type = factory.literal_string(right_ident.escaped_text.as_str());
            return Some(factory.index_access(base, index_type));
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            if access.question_dot_token {
                return None;
            }
            let base = self.deferred_typeof_value_chain(access.expression)?;
            let right_node = self.ctx.arena.get(access.name_or_argument)?;
            let right_ident = self.ctx.arena.get_identifier(right_node)?;
            let index_type = factory.literal_string(right_ident.escaped_text.as_str());
            return Some(factory.index_access(base, index_type));
        }
        None
    }

    /// Resolve the symbol for a type query expression name.
    ///
    /// Handles both simple identifiers and qualified names (e.g., `M.F2`).
    /// For qualified names, walks through namespace exports to find the member.
    fn resolve_type_query_symbol(&self, expr_name: NodeIndex) -> Option<tsz_binder::SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let node = self.ctx.arena.get(expr_name)?;

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if name == "default" {
                return None;
            }
            return self.resolve_value_symbol_in_scope(expr_name);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            // Recursively resolve the left side
            let left_sym = self.resolve_type_query_symbol(qn.left)?;

            // Get the right name
            let right_node = self.ctx.arena.get(qn.right)?;
            let right_ident = self.ctx.arena.get_identifier(right_node)?;
            let right_name = right_ident.escaped_text.as_str();

            // Look through binder + libs for the left symbol's exports
            let lib_binders: Vec<std::sync::Arc<tsz_binder::BinderState>> = self
                .ctx
                .lib_contexts
                .iter()
                .map(|lc| std::sync::Arc::clone(&lc.binder))
                .collect();
            let left_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym, &lib_binders)?;

            if let Some(exports) = left_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(right_name)
            {
                return Some(member_sym);
            }
        }

        None
    }

    // =========================================================================
    // Mapped Types
    // =========================================================================

    /// Check a mapped type ({ [P in K]: T }).
    ///
    /// This function validates the mapped type and emits TS7039 if the type expression
    /// after the colon is missing (e.g., `{[P in "bar"]}` instead of `{[P in "bar"]: string}`).
    ///
    /// Note: TS2322 constraint validation (key type must be assignable to
    /// `string | number | symbol`) is handled by `CheckerState::check_mapped_type_constraint`
    /// in `check_type_node`, which covers both top-level and conditional-nested mapped types.
    pub(super) fn get_type_from_mapped_type(&mut self, idx: NodeIndex) -> TypeId {
        use tsz_parser::parser::NodeIndex as ParserNodeIndex;

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };

        let Some(data) = self.ctx.arena.get_mapped_type(node) else {
            return TypeId::ERROR;
        };

        // TS7039: Mapped object type implicitly has an 'any' template type.
        // This error occurs when the type expression after the colon is missing.
        // Example: type Foo = {[P in "bar"]};  // Missing ": T" after "bar"]
        if data.type_node == ParserNodeIndex::NONE {
            self.ctx.report_mapped_type_missing_template(idx);
            return TypeId::ANY;
        }

        // Delegate to TypeLowering with extended resolvers (enum flags + lib search)
        self.lower_with_resolvers(idx, true, false)
    }
}
