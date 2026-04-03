//! Advanced Type Node Handlers
//!
//! This module contains handlers for advanced/derived type constructs:
//! - Type operators (readonly, keyof, unique)
//! - Indexed access types (T[K], Person["name"])
//! - Type queries (typeof X)
//! - Mapped types ({ [P in K]: T })

use super::type_node::TypeNodeChecker;
use super::type_node_helpers::{
    get_string_literal_from_type_index, is_typeof_global_this_type_node,
};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
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
                // TS1354: 'readonly' type modifier is only permitted on array and tuple literal types.
                if let Some(operand_node) = self.ctx.arena.get(type_op.type_node) {
                    let operand_kind = operand_node.kind;
                    if operand_kind != syntax_kind_ext::ARRAY_TYPE
                        && operand_kind != syntax_kind_ext::TUPLE_TYPE
                    {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.ctx.error(
                            node.pos,
                            node.end.saturating_sub(node.pos),
                            diagnostic_messages::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES.to_string(),
                            diagnostic_codes::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES,
                        );
                    }
                }
                return factory.readonly_type(inner_type);
            }

            // Handle keyof operator
            if operator == SyntaxKind::KeyOfKeyword as u16 {
                return factory.keyof(inner_type);
            }

            // Handle unique operator
            if operator == SyntaxKind::UniqueKeyword as u16 {
                // unique is handled differently - it's a type modifier for symbols
                // For now, just return the inner type
                return inner_type;
            }

            // Unknown operator - return inner type
            inner_type
        } else {
            TypeId::ERROR
        }
    }

    // =========================================================================
    // Indexed Access Types
    // =========================================================================

    /// Handle indexed access type nodes (e.g., `Person["name"]`, `T[K]`).
    pub(super) fn get_type_from_indexed_access_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR;
        };
        let factory = self.ctx.types.factory();

        if let Some(indexed_access) = self.ctx.arena.get_indexed_access_type(node) {
            let object_type = self.check(indexed_access.object_type);
            let index_type = self.check(indexed_access.index_type);

            // TS2538: Check if the index type is valid (string, number, symbol, or literal thereof)
            if let Some(invalid_member) = self.get_invalid_index_type_member(index_type)
                && let Some(inode) = self.ctx.arena.get(indexed_access.index_type)
            {
                let mut formatter = self.ctx.create_type_formatter();
                let index_type_str = formatter.format(invalid_member);
                let message = crate::diagnostics::format_message(
                    crate::diagnostics::diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    &[&index_type_str],
                );
                self.ctx
                    .error(inode.pos, inode.end - inode.pos, message, 2538);
            }

            if let Some(inode) = self.ctx.arena.get(indexed_access.index_type)
                && let Some(index_value) = self
                    .get_number_value_from_type_node(indexed_access.index_type)
                    .or_else(|| {
                        tsz_solver::type_queries::get_number_literal_value(
                            self.ctx.types,
                            index_type,
                        )
                    })
                && index_value.is_finite()
                && index_value.fract() == 0.0
                && index_value < 0.0
            {
                let object_for_tuple_check = self.resolve_object_for_tuple_check(object_type);
                if tsz_solver::type_queries::is_tuple_type(self.ctx.types, object_for_tuple_check) {
                    let message = crate::diagnostics::diagnostic_messages::
                        A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE
                        .to_string();
                    self.ctx.error(
                        inode.pos,
                        inode.end - inode.pos,
                        message,
                        crate::diagnostics::diagnostic_codes::A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE,
                    );
                    return TypeId::ERROR;
                }
            }

            // TS2493/TS2339: Check positive out-of-bounds index on tuple/union-of-tuples
            if let Some(inode) = self.ctx.arena.get(indexed_access.index_type)
                && let Some(index_value) = self
                    .get_number_value_from_type_node(indexed_access.index_type)
                    .or_else(|| {
                        tsz_solver::type_queries::get_number_literal_value(
                            self.ctx.types,
                            index_type,
                        )
                    })
                && index_value.is_finite()
                && index_value.fract() == 0.0
                && index_value >= 0.0
            {
                let index = index_value as usize;
                let object_for_tuple_check = self.resolve_object_for_tuple_check(object_type);
                // Single tuple out of bounds → TS2493
                if let Some(tuple_elements) =
                    crate::query_boundaries::type_computation::access::tuple_elements(
                        self.ctx.types,
                        object_for_tuple_check,
                    )
                {
                    let has_rest = tuple_elements.iter().any(|e| e.rest);
                    if !has_rest && index >= tuple_elements.len() {
                        let mut formatter = self.ctx.create_type_formatter();
                        let tuple_type_str = formatter.format(object_for_tuple_check);
                        let message = format!(
                            "Tuple type '{}' of length '{}' has no element at index '{}'.",
                            tuple_type_str,
                            tuple_elements.len(),
                            index,
                        );
                        self.ctx.error(
                            inode.pos,
                            inode.end - inode.pos,
                            message,
                            crate::diagnostics::diagnostic_codes::TUPLE_TYPE_OF_LENGTH_HAS_NO_ELEMENT_AT_INDEX,
                        );
                    }
                }
                // Union of tuples all out of bounds → TS2339
                // But suppress if object type is ANY/ERROR (circular reference implicit any)
                else if object_type != TypeId::ANY
                    && object_type != TypeId::ERROR
                    && !tsz_solver::is_error_type(self.ctx.types, object_type)
                    && let Some(members) = crate::query_boundaries::common::union_members(
                        self.ctx.types,
                        object_for_tuple_check,
                    )
                {
                    let all_out_of_bounds = !members.is_empty()
                        && members.iter().all(|&m| {
                            if let Some(elems) =
                                crate::query_boundaries::type_computation::access::tuple_elements(
                                    self.ctx.types,
                                    m,
                                )
                            {
                                let has_rest = elems.iter().any(|e| e.rest);
                                !has_rest && index >= elems.len()
                            } else {
                                false
                            }
                        });
                    if all_out_of_bounds {
                        let mut formatter = self.ctx.create_type_formatter();
                        let type_str = formatter.format(object_type);
                        let message = crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                            &[&index.to_string(), &type_str],
                        );
                        self.ctx.error(
                            inode.pos,
                            inode.end - inode.pos,
                            message,
                            crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        );
                    }
                }
            }

            // Special case: `(typeof globalThis)['key']` where key is a block-scoped
            // variable (let/const). Since typeof globalThis resolves to ANY, the solver
            // would return ANY without error. But tsc rejects block-scoped access through
            // typeof globalThis, so we intercept here.
            if object_type == TypeId::ANY
                && is_typeof_global_this_type_node(self.ctx.arena, indexed_access.object_type)
            {
                // In type position, the index is a LiteralType wrapping a string literal
                if let Some(key) =
                    get_string_literal_from_type_index(self.ctx.arena, indexed_access.index_type)
                    && let Some(sym_id) = self.ctx.binder.file_locals.get(key.as_str())
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && symbol.flags & tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                    && symbol.flags & tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE == 0
                {
                    if let Some(idx_node) = self.ctx.arena.get(indexed_access.index_type) {
                        let message = crate::diagnostics::format_message(
                                crate::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                                &[key.as_str(), "typeof globalThis"],
                            );
                        self.ctx.error(
                            idx_node.pos,
                            idx_node.end - idx_node.pos,
                            message,
                            crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                        );
                    }
                    return TypeId::ERROR;
                }
            }

            // TS2339: Check if the property exists on the object type for string literal index access
            // This handles cases like `Color["Red"]` where "Red" is not a property of the Color type
            if let Some(key) =
                get_string_literal_from_type_index(self.ctx.arena, indexed_access.index_type)
            {
                // Skip for type parameters, generic types, and deferred types - let the
                // property access validation at the actual access site handle those cases
                let resolved_object = self.resolve_object_for_tuple_check(object_type);
                let is_type_param = crate::query_boundaries::common::is_type_parameter_like(
                    self.ctx.types,
                    resolved_object,
                );

                // Suppress TS2339 when the object type is ANY or ERROR - this prevents
                // cascading errors when a variable has implicit any due to circular reference
                // (TS7022/TS7024 already reported for the circularity)
                let is_error_or_any = object_type == TypeId::ANY
                    || object_type == TypeId::ERROR
                    || tsz_solver::is_error_type(self.ctx.types, object_type);

                // Suppress TS2339 for generic application types (e.g., Options<State, Actions>)
                // where the type arguments are type parameters. When the object type is generic,
                // we can't determine if the property exists until the type is instantiated.
                let is_generic_application =
                    crate::query_boundaries::common::is_generic_application_with_type_params(
                        self.ctx.types,
                        resolved_object,
                    );

                // Suppress TS2339 when the index type itself contains type parameters.
                // This handles cases like `Options<State, Actions>[Key]` where Key is a type parameter.
                let index_has_type_params =
                    crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        index_type,
                    );

                // Suppress TS2339 when the object type is a Lazy type that may resolve to a generic type.
                // This handles cases where the interface reference needs to be resolved first.
                let is_lazy_with_potential_generic =
                    tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, resolved_object)
                        .is_some()
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            object_type,
                        );

                if !is_type_param
                    && !is_error_or_any
                    && !is_generic_application
                    && !index_has_type_params
                    && !is_lazy_with_potential_generic
                {
                    let prop_result =
                        crate::query_boundaries::property_access::resolve_property_access(
                            self.ctx.types,
                            resolved_object,
                            &key,
                        );

                    // If property not found and no index signature exists, emit TS2339
                    use crate::query_boundaries::common::PropertyAccessResult;
                    if matches!(prop_result, PropertyAccessResult::PropertyNotFound { .. }) {
                        // Check if there's an index signature that allows this key
                        let has_index_sig = crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            resolved_object,
                        )
                        .map_or(false, |shape| {
                            shape.string_index.is_some()
                                || (shape.number_index.is_some() && key.parse::<f64>().is_ok())
                        });

                        if !has_index_sig {
                            if let Some(idx_node) = self.ctx.arena.get(indexed_access.index_type) {
                                let mut formatter = self.ctx.create_type_formatter();
                                let type_str = formatter.format(object_type);
                                let message = crate::diagnostics::format_message(
                                    crate::diagnostics::diagnostic_messages::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                                    &[&key, &type_str],
                                );
                                self.ctx.error(
                                    idx_node.pos,
                                    idx_node.end - idx_node.pos,
                                    message,
                                    crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE,
                                );
                            }
                        }
                    }
                }
            }

            factory.index_access(object_type, index_type)
        } else {
            TypeId::ERROR
        }
    }

    /// Resolve object type for tuple-related checks (unwrap readonly, follow Lazy).
    fn resolve_object_for_tuple_check(&self, object_type: TypeId) -> TypeId {
        let unwrapped =
            crate::query_boundaries::common::unwrap_readonly(self.ctx.types, object_type);
        if let Some(def_id) = tsz_solver::lazy_def_id(self.ctx.types, unwrapped) {
            let resolved = self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .and_then(|env| env.get_def(def_id))
                .or_else(|| self.ctx.definition_store.get_body(def_id))
                .unwrap_or(unwrapped);
            crate::query_boundaries::common::unwrap_readonly(self.ctx.types, resolved)
        } else {
            unwrapped
        }
    }

    fn get_number_value_from_type_node(&self, idx: NodeIndex) -> Option<f64> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == syntax_kind_ext::LITERAL_TYPE {
            let data = self.ctx.arena.get_literal_type(node)?;
            return self.get_number_value_from_type_node(data.literal);
        }

        if node.kind == SyntaxKind::NumericLiteral as u16 {
            return self
                .ctx
                .arena
                .get_literal(node)
                .and_then(|literal| literal.value);
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.get_number_value_from_type_node(paren.expression);
        }

        if node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            let data = self.ctx.arena.get_unary_expr(node)?;
            let operand = self.get_number_value_from_type_node(data.operand)?;
            return match data.operator {
                k if k == SyntaxKind::MinusToken as u16 => Some(-operand),
                k if k == SyntaxKind::PlusToken as u16 => Some(operand),
                _ => None,
            };
        }

        None
    }

    /// Get the specific type that makes this type invalid as an index type (TS2538).
    fn get_invalid_index_type_member(&self, type_id: TypeId) -> Option<TypeId> {
        tsz_solver::type_queries::get_invalid_index_type_member(self.ctx.types, type_id)
    }

    // =========================================================================
    // Type Query (typeof)
    // =========================================================================

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

        // Prefer the already-computed value-space type at this query site when available.
        // This preserves flow-sensitive narrowing for `typeof expr` in type positions.
        if let Some(&expr_type) = self.ctx.node_types.get(&type_query.expr_name.0)
            && expr_type != TypeId::ERROR
        {
            return expr_type;
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
            return param_type;
        }

        // For qualified names (e.g., typeof M.F2), resolve the symbol through
        // the binder's export tables. Simple identifiers are already handled by
        // the node_types cache above, but qualified names need member resolution.
        if let Some(sym_id) = self.resolve_type_query_symbol(type_query.expr_name) {
            // TS2693: typeof requires a value binding. If the resolved symbol is
            // type-only (e.g., an interface or type alias without a value component),
            // emit an error instead of creating a TypeQuery.
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let flags = symbol.flags;
                let has_value = flags & tsz_binder::symbol_flags::VALUE != 0;
                let is_type_only = (flags & tsz_binder::symbol_flags::TYPE != 0) && !has_value;
                if is_type_only {
                    let escaped_name = symbol.escaped_name.clone();
                    self.emit_type_query_type_only_error(&escaped_name, type_query.expr_name);
                    return TypeId::ERROR;
                }
            }

            // For simple identifiers, try flow-sensitive narrowing. When `typeof c`
            // appears inside a type alias within a control flow guard (e.g.,
            // `if (typeof c === 'string') { type C = { [k: string]: typeof c }; }`),
            // the declared type should be narrowed by the control flow context.
            //
            // First try the symbol_types cache, then fall back to resolving
            // the type annotation from the variable declaration.
            let mut declared_type: Option<TypeId> = self
                .ctx
                .symbol_types
                .get(&sym_id)
                .copied()
                .filter(|&t| t != TypeId::ANY && t != TypeId::ERROR);

            if declared_type.is_none() {
                // symbol_types may not be populated yet (early phase).
                // Extract and resolve the type annotation from the declaration.
                let type_ann_idx = self.ctx.binder.get_symbol(sym_id).and_then(|symbol| {
                    let decl = symbol.value_declaration;
                    if decl.is_none() {
                        return None;
                    }
                    let decl_node = self.ctx.arena.get(decl)?;
                    if decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
                        if var_decl.type_annotation.is_some() {
                            return Some(var_decl.type_annotation);
                        }
                    }
                    None
                });
                if let Some(ann_idx) = type_ann_idx {
                    let resolved = self.check(ann_idx);
                    if resolved != TypeId::ANY && resolved != TypeId::ERROR {
                        declared_type = Some(resolved);
                    }
                }
            }

            if let Some(declared_type) = declared_type
                && declared_type != TypeId::ANY
                && declared_type != TypeId::ERROR
            {
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
                            current = self.ctx.arena.get_extended(parent).map(|ext| ext.parent);
                        }
                        None
                    });

                if let Some(flow_node) = flow_node {
                    let analyzer = crate::FlowAnalyzer::with_node_types(
                        self.ctx.arena,
                        self.ctx.binder,
                        self.ctx.types,
                        &self.ctx.node_types,
                    )
                    .with_flow_cache(&self.ctx.flow_analysis_cache)
                    .with_switch_reference_cache(&self.ctx.flow_switch_reference_cache)
                    .with_numeric_atom_cache(&self.ctx.flow_numeric_atom_cache)
                    .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
                    .with_type_environment(&self.ctx.type_environment)
                    .with_narrowing_cache(&self.ctx.narrowing_cache)
                    .with_call_type_predicates(&self.ctx.call_type_predicates)
                    .with_flow_buffers(
                        &self.ctx.flow_worklist,
                        &self.ctx.flow_in_worklist,
                        &self.ctx.flow_visited,
                        &self.ctx.flow_results,
                    )
                    .with_symbol_last_assignment_pos(&self.ctx.symbol_last_assignment_pos)
                    .with_destructured_bindings(&self.ctx.destructured_bindings);

                    let narrowed =
                        analyzer.get_flow_type(type_query.expr_name, declared_type, flow_node);
                    if narrowed != TypeId::ERROR {
                        return narrowed;
                    }
                }
            }

            let factory = self.ctx.types.factory();
            return factory.type_query(tsz_solver::SymbolRef(sym_id.0));
        }

        // For qualified/generic typeof expressions (typeof A.B, typeof A<B>),
        // check if the root identifier exists. If not, emit TS2304.
        if name_opt.is_none() {
            use tsz_parser::parser::syntax_kind_ext;
            let mut root_idx = type_query.expr_name;
            while let Some(node) = self.ctx.arena.get(root_idx) {
                if node.kind == syntax_kind_ext::QUALIFIED_NAME
                    && let Some(qn) = self.ctx.arena.get_qualified_name(node)
                {
                    root_idx = qn.left;
                    continue;
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
                let factory = self.ctx.types.factory();
                return factory.type_query(tsz_solver::SymbolRef(sym_id.0));
            }
            // Skip TS2304 for well-known globals that may not be in local binder scope
            // but are valid in typeof position (undefined, NaN, Infinity, globalThis, etc.)
            let is_global_name = matches!(
                name,
                "undefined" | "NaN" | "Infinity" | "globalThis" | "arguments"
            );
            if is_global_name {
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
            let sym_id = self.ctx.binder.file_locals.get(name)?;
            return Some(sym_id);
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
            let message = "Mapped object type implicitly has an 'any' template type.";
            self.ctx
                .error(node.pos, node.end - node.pos, message.to_string(), 7039);
            return TypeId::ANY;
        }

        // Delegate to TypeLowering with extended resolvers (enum flags + lib search)
        self.lower_with_resolvers(idx, true, false)
    }
}
