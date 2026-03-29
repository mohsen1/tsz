//! Parameter type utilities, type construction, and type resolution methods
//! for `CheckerState`.

use crate::query_boundaries::type_checking_utilities as query;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, node::PropertyDeclData};
use tsz_solver::TypeId;

/// Result from resolving literal string keys against an object type.
pub(crate) struct LiteralKeysResult {
    /// The computed result type (union/intersection of found key types).
    /// `None` only when the lookup itself failed (e.g., object was unknown).
    pub result_type: Option<TypeId>,
    /// Keys that were not found as properties on the object type.
    /// When non-empty, the caller should emit TS2339 for each.
    pub missing_keys: Vec<String>,
}

impl<'a> CheckerState<'a> {
    // ============================================================================
    // Section 52: Parameter Type Utilities
    // ============================================================================

    fn contextual_rest_tuple_parameter_type(
        &mut self,
        expected: TypeId,
        index: usize,
        is_rest: bool,
    ) -> Option<TypeId> {
        let shape = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        )?;
        let rest_param = shape.params.last().filter(|param| param.rest)?;
        if is_rest {
            // For rest parameters in function expressions, preserve the original
            // type (including type parameters like `Args extends any[]`). The
            // constraint-resolved type would lose the generic identity, causing
            // the rest param to be typed as `any[]` instead of `Args`.
            return Some(rest_param.type_id);
        }
        let rest_param_type = self.contextual_rest_parameter_source_type(rest_param.type_id);

        if let Some(tuple_elements) =
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, rest_param_type)
        {
            // Variadic tuples (rest element followed by tail elements, e.g.
            // `[...((n: number) => void)[], (x: any) => void]`) require
            // arg_count-aware mapping to distinguish rest vs tail positions.
            // Return None so the solver's `extract_param_type_at_for_call`
            // handles them with proper variadic expansion.
            let rest_pos = tuple_elements.iter().position(|e| e.rest);
            let has_tail_after_rest = rest_pos.is_some_and(|pos| pos + 1 < tuple_elements.len());
            if has_tail_after_rest {
                return None;
            }

            if let Some(element) = tuple_elements.get(index) {
                return Some(element.type_id);
            }
            if let Some(last) = tuple_elements.last()
                && last.rest
            {
                return Some(last.type_id);
            }
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, rest_param_type)
        {
            let mut element_types = Vec::new();
            for member in members {
                let Some(tuple_elements) =
                    tsz_solver::type_queries::get_tuple_elements(self.ctx.types, member)
                else {
                    continue;
                };
                if let Some(element) = tuple_elements.get(index) {
                    element_types.push(element.type_id);
                    continue;
                }
                if let Some(last) = tuple_elements.last()
                    && last.rest
                {
                    element_types.push(last.type_id);
                }
            }
            if !element_types.is_empty() {
                return Some(self.ctx.types.factory().union(element_types));
            }
        }

        tsz_solver::type_queries::get_array_element_type(self.ctx.types, rest_param_type)
    }

    fn contextual_rest_parameter_source_type(&mut self, rest_param_type: TypeId) -> TypeId {
        let mut source_type = rest_param_type;
        if (crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, source_type)
            || crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                source_type,
            ))
            && let Some(constraint) = crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                source_type,
            )
            && constraint != TypeId::UNKNOWN
            && constraint != TypeId::ERROR
        {
            source_type = self.evaluate_contextual_type(constraint);
        }
        source_type
    }

    fn should_skip_contextual_signature_fallback_for_parameter(
        &mut self,
        expected: TypeId,
        index: usize,
        arg_count: Option<usize>,
    ) -> bool {
        if tsz_solver::is_union_type(self.ctx.types, expected)
            || tsz_solver::is_intersection_type(self.ctx.types, expected)
        {
            return true;
        }

        let Some(shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        ) else {
            return false;
        };
        let Some(rest_param) = shape.params.last().filter(|param| param.rest) else {
            return false;
        };
        let rest_param_type = self.contextual_rest_parameter_source_type(rest_param.type_id);
        let rest_start = shape.params.len().saturating_sub(1);
        index >= rest_start
            && arg_count.is_some()
            && (crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                rest_param_type,
            ) || crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                rest_param_type,
            ))
    }

    pub(crate) fn parameter_symbol_ids(
        &self,
        param_idx: NodeIndex,
        param_name: NodeIndex,
    ) -> [Option<SymbolId>; 2] {
        let name_sym = self.ctx.binder.get_node_symbol(param_name);
        let param_sym = self.ctx.binder.get_node_symbol(param_idx);
        if name_sym.is_some() && name_sym == param_sym {
            [name_sym, None]
        } else {
            [name_sym, param_sym]
        }
    }

    pub(crate) fn resolve_jsdoc_import_member(
        &self,
        module_specifier: &str,
        member_name: &str,
    ) -> Option<SymbolId> {
        self.resolve_cross_file_export_from_file(
            module_specifier,
            member_name,
            Some(self.ctx.current_file_idx),
        )
        .or_else(|| {
            self.ctx
                .binder
                .resolve_import_with_reexports_type_only(module_specifier, member_name)
                .map(|(sym_id, _)| sym_id)
        })
        .or_else(|| self.resolve_cross_file_export(module_specifier, member_name))
    }

    pub(crate) fn effective_class_property_declared_type(
        &mut self,
        member_idx: NodeIndex,
        prop: &PropertyDeclData,
    ) -> Option<TypeId> {
        if prop.type_annotation.is_some() {
            if self.is_js_file() {
                // In JS/checkJs, property type syntax still reports TS8010, but it
                // should not drive later class-property semantics such as constructor
                // assignment checks or member-access narrowing.
                return Some(TypeId::ANY);
            }
            return Some(self.get_type_from_type_node(prop.type_annotation));
        }

        if self.is_js_file() {
            self.jsdoc_type_annotation_for_node(member_idx)
        } else {
            None
        }
    }

    /// Cache parameter types for function parameters.
    ///
    /// This function extracts and caches the types of function parameters,
    /// either from provided type annotations or from explicit type nodes.
    /// For parameters without explicit type annotations, `UNKNOWN` is used
    /// (not `ANY`) to maintain better type safety.
    ///
    /// ## Parameters:
    /// - `params`: Slice of parameter node indices
    /// - `param_types`: Optional pre-computed parameter types (e.g., from contextual typing)
    ///
    /// ## Examples:
    /// ```typescript
    /// // Explicit types: cached from type annotation
    /// function foo(x: string, y: number) {}
    ///
    /// // No types: cached as UNKNOWN
    /// function bar(a, b) {}
    ///
    /// // Contextual types: cached from provided types
    /// const fn = (x: string) => number;
    /// const cb: typeof fn = (x) => x.length;  // x typed from context
    /// ```
    pub(crate) fn cache_parameter_types(
        &mut self,
        params: &[NodeIndex],
        param_types: Option<&[Option<TypeId>]>,
    ) {
        let factory = self.ctx.types.factory();
        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let symbol_ids = self.parameter_symbol_ids(param_idx, param.name);
            let Some(primary_sym_id) = symbol_ids.into_iter().flatten().next() else {
                continue;
            };
            self.push_symbol_dependency(primary_sym_id, true);
            let type_id = if let Some(types) = param_types {
                // param_types already have optional undefined applied
                types.get(i).and_then(|t| *t)
            } else if param.type_annotation.is_some() {
                let mut t = self.get_type_from_type_node(param.type_annotation);
                // Under strictNullChecks, optional parameters (with `?`) include
                // `undefined` in their type.  Parameters with only a default value
                // (no `?`) do NOT — the default guarantees a value at runtime.
                if param.question_token
                    && self.ctx.strict_null_checks()
                    && t != TypeId::ANY
                    && t != TypeId::UNKNOWN
                    && t != TypeId::ERROR
                {
                    t = factory.union2(t, TypeId::UNDEFINED);
                }
                Some(t)
            } else {
                // Parameters without type annotations get implicit 'any' type.
                // TypeScript uses 'any' (with TS7006 when noImplicitAny is enabled).
                //
                // In JS files, check the parent function's JSDoc @param {Type} annotations
                // first. This is how tsc handles JS: @param types are the primary source of
                // parameter type information, taking precedence over contextual types.
                let jsdoc_type = if self.is_js_file() {
                    let pname = self.parameter_name_for_error(param.name);
                    let mut current = param_idx;
                    let mut found = None;
                    // First try @param {Type} name annotations
                    for _ in 0..4 {
                        if let Some(ext) = self.ctx.arena.get_extended(current)
                            && ext.parent.is_some()
                        {
                            current = ext.parent;
                            if let Some(comment_start) =
                                self.get_jsdoc_comment_pos_for_function(current)
                                && let Some(func_jsdoc) = self.get_jsdoc_for_function(current)
                                && let Some(t) = self.resolve_jsdoc_param_type_with_pos(
                                    &func_jsdoc,
                                    &pname,
                                    Some(comment_start),
                                )
                            {
                                found = Some(t);
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    // If no @param type, check for @type {FunctionType} on the parent
                    // function declaration and extract parameter type by position
                    if found.is_none() {
                        let mut current2 = param_idx;
                        for _ in 0..4 {
                            if let Some(ext) = self.ctx.arena.get_extended(current2)
                                && ext.parent.is_some()
                            {
                                current2 = ext.parent;
                                if let Some(parent_node) = self.ctx.arena.get(current2)
                                    && parent_node.kind
                                        == tsz_parser::syntax_kind_ext::FUNCTION_DECLARATION
                                {
                                    if let Some(func_type) =
                                        self.jsdoc_type_annotation_for_node(current2)
                                    {
                                        use tsz_solver::ContextualTypeContext;
                                        let evaluated = self.evaluate_contextual_type(func_type);
                                        let ctx_helper = ContextualTypeContext::with_expected(
                                            self.ctx.types,
                                            evaluated,
                                        );
                                        found = ctx_helper.get_parameter_type(i);
                                    }
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    found
                } else {
                    None
                };
                Some(jsdoc_type.unwrap_or(TypeId::ANY))
            };
            self.pop_symbol_dependency();

            if let Some(type_id) = type_id {
                for sym_id in self
                    .parameter_symbol_ids(param_idx, param.name)
                    .into_iter()
                    .flatten()
                {
                    // When called without pre-computed param_types (None path),
                    // don't overwrite a parameter type that was already cached by
                    // get_type_of_function (which computes types from initializer
                    // expressions in JS files). Only overwrite if the existing
                    // cached type is absent or is a placeholder (ERROR).
                    if param_types.is_none()
                        && let Some(&existing) = self.ctx.symbol_types.get(&sym_id)
                        && existing != TypeId::ERROR
                    {
                        continue;
                    }
                    self.cache_symbol_type(sym_id, type_id);
                }
            }
        }
    }

    pub(crate) fn contextual_parameter_type_from_enclosing_function(
        &mut self,
        param_idx: NodeIndex,
    ) -> Option<TypeId> {
        let mut param_idx = param_idx;
        let mut param_node = self.ctx.arena.get(param_idx)?;
        if self.ctx.arena.get_parameter(param_node).is_none() {
            let ext = self.ctx.arena.get_extended(param_idx)?;
            let parent_idx = ext.parent;
            let parent_node = self.ctx.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::PARAMETER {
                param_idx = parent_idx;
                param_node = parent_node;
            } else if parent_node.kind == syntax_kind_ext::BINDING_ELEMENT {
                let ext2 = self.ctx.arena.get_extended(parent_idx)?;
                let pattern_idx = ext2.parent;
                let pattern_node = self.ctx.arena.get(pattern_idx)?;
                if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || pattern_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                {
                    let ext3 = self.ctx.arena.get_extended(pattern_idx)?;
                    let maybe_param_idx = ext3.parent;
                    let maybe_param_node = self.ctx.arena.get(maybe_param_idx)?;
                    if maybe_param_node.kind == syntax_kind_ext::PARAMETER {
                        param_idx = maybe_param_idx;
                        param_node = maybe_param_node;
                    }
                }
            }
        }
        let param = self.ctx.arena.get_parameter(param_node)?;

        let mut current = param_idx;
        let mut function_idx = NodeIndex::NONE;
        for _ in 0..4 {
            let ext = self.ctx.arena.get_extended(current)?;
            current = ext.parent;
            let parent = self.ctx.arena.get(current)?;
            if matches!(
                parent.kind,
                syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::SET_ACCESSOR
                    | syntax_kind_ext::GET_ACCESSOR
            ) {
                function_idx = current;
                break;
            }
        }

        if function_idx.is_none() {
            return None;
        }

        let parameters = if let Some(func) = self
            .ctx
            .arena
            .get_function(self.ctx.arena.get(function_idx)?)
        {
            &func.parameters.nodes
        } else if let Some(method) = self
            .ctx
            .arena
            .get_method_decl(self.ctx.arena.get(function_idx)?)
        {
            &method.parameters.nodes
        } else {
            return None;
        };

        let param_position = parameters.iter().position(|&idx| idx == param_idx)?;
        let this_atom = self.ctx.types.intern_string("this");
        let contextual_index = parameters[..param_position]
            .iter()
            .filter(|&&idx| {
                self.ctx
                    .arena
                    .get(idx)
                    .and_then(|node| self.ctx.arena.get_parameter(node))
                    .and_then(|p| self.ctx.arena.get(p.name))
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_none_or(|ident| {
                        self.ctx.types.intern_string(&ident.escaped_text) != this_atom
                    })
            })
            .count();

        let contextual_type = self
            .ctx
            .contextual_type
            .or_else(|| {
                self.ctx
                    .binder
                    .get_node_symbol(function_idx)
                    .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
                    .filter(|&ty| ty != TypeId::ANY && ty != TypeId::UNKNOWN && ty != TypeId::ERROR)
            })
            .or_else(|| {
                let function_ext = self.ctx.arena.get_extended(function_idx)?;
                let parent_idx = function_ext.parent;
                let parent = self.ctx.arena.get(parent_idx)?;
                let variable_decl = self.ctx.arena.get_variable_declaration(parent)?;
                (variable_decl.initializer == function_idx)
                    .then(|| variable_decl.type_annotation.is_some())
                    .and_then(|has_annotation| {
                        if has_annotation {
                            Some(self.get_type_from_type_node(variable_decl.type_annotation))
                        } else {
                            self.jsdoc_type_annotation_for_node(parent_idx)
                        }
                    })
            })
            .or_else(|| {
                self.is_js_file()
                    .then(|| self.jsdoc_type_annotation_for_node(function_idx))
                    .flatten()
            })?;
        let contextual_type = self.evaluate_contextual_type(contextual_type);
        let helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            contextual_type,
            self.ctx.compiler_options.no_implicit_any,
        );

        let mut ty = if param.dot_dot_dot_token {
            helper.get_rest_parameter_type(contextual_index)?
        } else {
            helper.get_parameter_type(contextual_index)?
        };

        let js_optional = if self.is_js_file() {
            self.get_jsdoc_for_function(function_idx)
                .is_some_and(|jsdoc| {
                    let jsdoc_param_names: Vec<String> = Self::extract_jsdoc_param_names(&jsdoc)
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect();
                    let pname = self.effective_jsdoc_param_name(
                        param.name,
                        &jsdoc_param_names,
                        contextual_index,
                    );
                    !Self::jsdoc_has_required_param_tag(&jsdoc, &pname)
                })
        } else {
            false
        };

        if (param.question_token || js_optional)
            && self.ctx.strict_null_checks()
            && ty != TypeId::ANY
            && ty != TypeId::ERROR
            && ty != TypeId::UNDEFINED
            && !tsz_solver::type_contains_undefined(self.ctx.types, ty)
        {
            ty = self.ctx.types.factory().union2(ty, TypeId::UNDEFINED);
        }

        Some(ty)
    }

    pub(crate) fn contextual_parameter_type_with_env_from_expected(
        &mut self,
        expected: TypeId,
        index: usize,
        is_rest: bool,
    ) -> Option<TypeId> {
        let expected = self.normalize_contextual_signature_with_env(expected);
        if expected == TypeId::ERROR {
            return None;
        }
        if let Some(rest_tuple_type) =
            self.contextual_rest_tuple_parameter_type(expected, index, is_rest)
        {
            return Some(rest_tuple_type);
        }
        let helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            expected,
            self.ctx.compiler_options.no_implicit_any,
        );

        if is_rest {
            helper.get_rest_parameter_type(index).or_else(|| {
                if self
                    .should_skip_contextual_signature_fallback_for_parameter(expected, index, None)
                {
                    return None;
                }
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    expected,
                )
                .and_then(|shape| {
                    shape
                        .params
                        .get(index)
                        .map(|param| param.type_id)
                        .or_else(|| {
                            let last = shape.params.last()?;
                            last.rest.then_some(last.type_id)
                        })
                })
            })
        } else {
            helper.get_parameter_type(index).or_else(|| {
                if self
                    .should_skip_contextual_signature_fallback_for_parameter(expected, index, None)
                {
                    return None;
                }
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    expected,
                )
                .and_then(|shape| shape.params.get(index).map(|param| param.type_id))
            })
        }
    }

    pub(crate) fn contextual_parameter_type_for_call_with_env_from_expected(
        &mut self,
        expected: TypeId,
        index: usize,
        arg_count: usize,
    ) -> Option<TypeId> {
        let expected = self.normalize_contextual_signature_with_env(expected);
        if expected == TypeId::ERROR {
            return None;
        }
        if crate::query_boundaries::common::index_access_types(self.ctx.types, expected).is_some()
            || crate::query_boundaries::common::type_application(self.ctx.types, expected).is_some()
        {
            let evaluated = self.evaluate_type_with_env(expected);
            if evaluated != expected {
                return self.contextual_parameter_type_for_call_with_env_from_expected(
                    evaluated, index, arg_count,
                );
            }
        }
        let evaluated_expected = self.evaluate_contextual_type(expected);
        if evaluated_expected != expected {
            return self.contextual_parameter_type_for_call_with_env_from_expected(
                evaluated_expected,
                index,
                arg_count,
            );
        }
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, expected)
        {
            let union_has_direct_call_signatures =
                crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, expected)
                    .is_some();
            let evaluated_members: Vec<_> = members
                .iter()
                .map(|&member| (member, self.evaluate_type_with_env(member)))
                .collect();
            let has_evaluated_members = evaluated_members
                .iter()
                .any(|(member, evaluated)| member != evaluated);
            if has_evaluated_members || !union_has_direct_call_signatures {
                let contextual_members: Vec<_> = evaluated_members
                    .into_iter()
                    .filter_map(|(member, evaluated_member)| {
                        let target_member = if evaluated_member != member {
                            evaluated_member
                        } else {
                            member
                        };
                        if evaluated_member != member {
                            self.contextual_parameter_type_for_call_with_env_from_expected(
                                target_member,
                                index,
                                arg_count,
                            )
                            .or_else(|| {
                                self.contextual_mixed_overload_param_type_for_call(
                                    target_member,
                                    index,
                                    arg_count,
                                )
                            })
                        } else if !union_has_direct_call_signatures {
                            self.contextual_mixed_overload_param_type_for_call(
                                target_member,
                                index,
                                arg_count,
                            )
                        } else {
                            None
                        }
                    })
                    .collect();
                return match contextual_members.len() {
                    0 => None,
                    1 => Some(contextual_members[0]),
                    _ => Some(
                        self.ctx
                            .types
                            .factory()
                            .union_preserve_members(contextual_members),
                    ),
                };
            }
        }
        if let Some(rest_tuple_type) =
            self.contextual_rest_tuple_parameter_type(expected, index, false)
        {
            return Some(rest_tuple_type);
        }
        if self.should_skip_contextual_signature_fallback_for_parameter(
            expected,
            index,
            Some(arg_count),
        ) {
            return None;
        }
        let helper = tsz_solver::ContextualTypeContext::with_expected_and_options(
            self.ctx.types,
            expected,
            self.ctx.compiler_options.no_implicit_any,
        );

        helper
            .get_parameter_type_for_call(index, arg_count)
            .or_else(|| {
                if self.should_skip_contextual_signature_fallback_for_parameter(
                    expected,
                    index,
                    Some(arg_count),
                ) {
                    return None;
                }
                crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    expected,
                )
                .and_then(|shape| {
                    let required = shape.params.iter().filter(|param| !param.optional).count();
                    let last = shape.params.last();
                    let accepts_arity = arg_count >= required
                        && (arg_count <= shape.params.len()
                            || last.is_some_and(|param| param.rest));
                    accepts_arity.then_some(shape).and_then(|shape| {
                        shape
                            .params
                            .get(index)
                            .map(|param| param.type_id)
                            .or_else(|| {
                                let last = shape.params.last()?;
                                last.rest.then_some(last.type_id)
                            })
                    })
                })
            })
    }

    pub(crate) fn normalize_contextual_signature_with_env(&mut self, expected: TypeId) -> TypeId {
        fn should_preserve_contextual_param_type(
            db: &dyn tsz_solver::TypeDatabase,
            ty: TypeId,
        ) -> bool {
            // Delegate to solver query: checks if any union member is constructor-like
            crate::query_boundaries::common::is_constructor_like_type(db, ty)
        }

        fn is_tuple_like_rest_param(db: &dyn tsz_solver::TypeDatabase, ty: TypeId) -> bool {
            tsz_solver::type_queries::get_tuple_elements(db, ty).is_some()
                || crate::query_boundaries::common::union_members(db, ty).is_some_and(|members| {
                    !members.is_empty()
                        && members.iter().all(|member| {
                            tsz_solver::type_queries::get_tuple_elements(db, *member).is_some()
                        })
                })
        }

        if let Some(constraint) =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, expected)
            && constraint != expected
            && constraint != TypeId::UNKNOWN
            && constraint != TypeId::ERROR
        {
            return self.normalize_contextual_signature_with_env(constraint);
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, expected)
        {
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| self.normalize_contextual_signature_with_env(member))
                .collect();
            if normalized_members
                .iter()
                .zip(members.iter())
                .any(|(normalized, original)| normalized != original)
            {
                return self
                    .ctx
                    .types
                    .factory()
                    .union_preserve_members(normalized_members);
            }
            return expected;
        }

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, expected)
        {
            let normalized_members: Vec<_> = members
                .iter()
                .map(|&member| self.normalize_contextual_signature_with_env(member))
                .collect();
            if normalized_members
                .iter()
                .zip(members.iter())
                .any(|(normalized, original)| normalized != original)
            {
                return self.ctx.types.factory().intersection(normalized_members);
            }
            return expected;
        }

        let Some(mut shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        ) else {
            return expected;
        };

        let mut changed = false;
        for param in &mut shape.params {
            let resolved = self.resolve_type_query_type(param.type_id);
            if param.rest {
                let evaluated_with_env = self.evaluate_type_with_env(resolved);
                let became_more_concrete = evaluated_with_env != param.type_id
                    && (is_tuple_like_rest_param(self.ctx.types, evaluated_with_env)
                        || !crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            evaluated_with_env,
                        ));
                if became_more_concrete {
                    param.type_id = evaluated_with_env;
                    changed = true;
                    continue;
                }

                if is_tuple_like_rest_param(self.ctx.types, param.type_id)
                    || crate::query_boundaries::common::is_type_parameter_like(
                        self.ctx.types,
                        param.type_id,
                    )
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        param.type_id,
                    )
                {
                    continue;
                }
            }

            let evaluated = if should_preserve_contextual_param_type(self.ctx.types, resolved) {
                resolved
            } else {
                self.evaluate_type_with_env(resolved)
            };
            if evaluated != param.type_id {
                param.type_id = evaluated;
                changed = true;
            }
        }

        if changed {
            self.ctx.types.factory().function(shape)
        } else {
            expected
        }
    }

    /// Assign contextual types to destructuring parameters (binding patterns).
    ///
    /// When a function has a contextual type (e.g., from a callback position),
    /// destructuring parameters need to have their bindings inferred from
    /// the contextual parameter type.
    ///
    /// This function only processes parameters without explicit type annotations,
    /// as TypeScript respects explicit annotations over contextual inference.
    ///
    /// ## Examples:
    /// ```typescript
    /// declare function map<T, U>(arr: T[], fn: (item: T) => U): U[];
    ///
    /// // x and y types come from contextual type T
    /// map(arr, ({ x, y }) => x + y);
    ///
    /// // Explicit annotation takes precedence
    /// map(arr, ({ x, y }: { x: string; y: number }) => x + y);
    /// ```
    pub(crate) fn assign_contextual_types_to_destructuring_params(
        &mut self,
        params: &[NodeIndex],
        param_types: &[Option<TypeId>],
    ) {
        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };

            if param.type_annotation.is_some() {
                continue;
            }

            // Only process binding patterns (destructuring)
            let is_binding_pattern = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;

            if !is_binding_pattern {
                continue;
            }

            // Get the contextual type for this parameter position
            let contextual_type = param_types
                .get(i)
                .and_then(|t| *t)
                .filter(|&t| t != TypeId::UNKNOWN && t != TypeId::ERROR);

            if let Some(mut ctx_type) = contextual_type {
                if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, ctx_type)
                    && crate::query_boundaries::common::type_parameter_constraint(
                        self.ctx.types,
                        ctx_type,
                    )
                    .is_none()
                {
                    continue;
                }
                // When the parameter has a default value (e.g., `{ x } = {}`),
                // strip `undefined` from the contextual type since the default
                // guarantees the destructured value is not undefined. Without
                // this, `T | undefined` causes false TS2339 on destructured
                // property access.
                if param.initializer.is_some() {
                    ctx_type = tsz_solver::remove_undefined(self.ctx.types, ctx_type);
                }
                // Assign the contextual type to the binding pattern elements
                let request = crate::context::TypingRequest::with_contextual_type(ctx_type);
                self.assign_binding_pattern_symbol_types_with_request(
                    param.name, ctx_type, &request,
                );
            }
        }
    }

    /// Record destructured parameter binding groups for correlated narrowing.
    ///
    /// This enables cases like:
    /// `function f({ data, isSuccess }: Result) { if (isSuccess) data... }`
    /// where narrowing one binding should narrow sibling bindings from the same source union.
    pub(crate) fn record_destructured_parameter_binding_groups(
        &mut self,
        params: &[NodeIndex],
        param_types: &[Option<TypeId>],
    ) {
        use crate::query_boundaries::state::checking as query;

        for (i, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };

            let is_binding_pattern = name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
            if !is_binding_pattern {
                continue;
            }

            let Some(param_type) = param_types.get(i).and_then(|t| *t) else {
                continue;
            };
            if param_type == TypeId::UNKNOWN || param_type == TypeId::ERROR {
                continue;
            }

            let mut resolved_for_union = self.evaluate_type_with_env(param_type);
            if query::union_members(self.ctx.types, resolved_for_union).is_none()
                && let Some(constraint) =
                    query::type_parameter_constraint(self.ctx.types, resolved_for_union)
            {
                resolved_for_union = self.evaluate_type_with_env(constraint);
            }
            if query::union_members(self.ctx.types, resolved_for_union).is_none() {
                continue;
            }

            // Parameters with binding patterns are treated as stable for correlated
            // narrowing, matching TypeScript's alias-aware flow behavior.
            self.record_destructured_binding_group(
                param.name,
                resolved_for_union,
                true,
                name_node.kind,
            );
        }
    }

    pub(crate) fn record_contextual_tuple_parameter_groups(
        &mut self,
        params: &[NodeIndex],
        contextual_type: Option<TypeId>,
    ) {
        use crate::context::DestructuredBindingInfo;
        use crate::query_boundaries::state::checking as state_query;

        let Some(expected) = contextual_type else {
            return;
        };
        let Some(shape) = crate::query_boundaries::checkers::call::get_contextual_signature(
            self.ctx.types,
            expected,
        ) else {
            return;
        };
        let Some(rest_param) = shape.params.last().filter(|param| param.rest) else {
            return;
        };

        let mut source_type = self.evaluate_type_with_env(rest_param.type_id);
        if state_query::union_members(self.ctx.types, source_type).is_none()
            && let Some(constraint) =
                state_query::type_parameter_constraint(self.ctx.types, source_type)
        {
            source_type = self.evaluate_type_with_env(constraint);
        }

        let has_tuple_shape =
            tsz_solver::type_queries::get_tuple_elements(self.ctx.types, source_type).is_some()
                || state_query::union_members(self.ctx.types, source_type).is_some_and(|members| {
                    members.iter().all(|&member| {
                        tsz_solver::type_queries::get_tuple_elements(self.ctx.types, member)
                            .is_some()
                    })
                });
        if !has_tuple_shape {
            return;
        }

        let group_id = self.ctx.next_binding_group_id;
        self.ctx.next_binding_group_id += 1;

        for (index, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            if name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                continue;
            }

            for sym_id in self
                .parameter_symbol_ids(param_idx, param.name)
                .into_iter()
                .flatten()
            {
                self.ctx.destructured_bindings.insert(
                    sym_id,
                    DestructuredBindingInfo {
                        source_type,
                        property_name: String::new(),
                        element_index: index as u32,
                        group_id,
                        is_const: true,
                        is_rest: false,
                    },
                );
            }
        }
    }

    // ============================================================================
    // Section 53: Type and Symbol Utilities
    // ============================================================================

    /// Widen a literal type to its primitive type.
    ///
    /// This function converts literal types to their corresponding primitive types,
    /// which is used for type widening in various contexts:
    /// - Variable declarations without type annotations
    /// - Property assignments
    /// - Return type inference
    ///
    /// ## Examples:
    /// ```typescript
    /// // Literal types are widened to primitives:
    /// let x = "hello";  // Type: string (not "hello")
    /// let y = 42;       // Type: number (not 42)
    /// let z = true;     // Type: boolean (not true)
    /// ```
    pub(crate) fn widen_literal_type(&self, type_id: TypeId) -> TypeId {
        crate::query_boundaries::common::widen_type(self.ctx.types, type_id)
    }

    /// Widen a type for diagnostic display purposes.
    ///
    /// Like `widen_literal_type` but preserves boolean literal intrinsics
    /// (`true`/`false`), so narrowed types like `string | false` display
    /// correctly instead of being widened to `string | boolean`.
    pub(crate) fn widen_type_for_display(&self, type_id: TypeId) -> TypeId {
        crate::query_boundaries::common::widen_type_for_display(self.ctx.types, type_id)
    }

    /// Widen a mutable binding initializer type (let/var semantics).
    ///
    /// In addition to primitive literal widening, TypeScript widens enum member
    /// initializers (`let x = E.A`) to the parent enum type (`E`), not the
    /// specific member.
    pub(crate) fn widen_initializer_type_for_mutable_binding(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::type_queries;

        // Check if this is an enum member type that should widen to parent enum
        if let Some(def_id) = type_queries::get_enum_def_id(self.ctx.types, type_id) {
            // Check if this DefId is an enum member (has a parent enum)
            let parent_def_id = self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .and_then(|env| env.get_enum_parent(def_id));

            if let Some(parent_def_id) = parent_def_id {
                // This is an enum member - widen to parent enum type
                if let Some(parent_sym_id) = self.ctx.def_to_symbol_id(parent_def_id) {
                    return self.get_type_of_symbol(parent_sym_id);
                }
            }
        }

        // Fallback: check via symbol flags (legacy path)
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
        {
            return self.get_type_of_symbol(symbol.parent);
        }
        self.widen_literal_type(type_id)
    }

    /// Widen only enum member types to their parent enum type.
    ///
    /// Unlike `widen_initializer_type_for_mutable_binding`, this does NOT widen
    /// literal types (e.g., `2` stays `2`, not `number`). This is used in operator
    /// error messages where tsc preserves literal types but widens enum members.
    pub(crate) fn widen_enum_member_type(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::type_queries;

        // Check if this is an enum member type that should widen to parent enum
        if let Some(def_id) = type_queries::get_enum_def_id(self.ctx.types, type_id) {
            let parent_def_id = self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .and_then(|env| env.get_enum_parent(def_id));

            if let Some(parent_def_id) = parent_def_id
                && let Some(parent_sym_id) = self.ctx.def_to_symbol_id(parent_def_id)
            {
                return self.get_type_of_symbol(parent_sym_id);
            }
        }

        // Fallback: check via symbol flags (legacy path)
        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
        {
            return self.get_type_of_symbol(symbol.parent);
        }

        // Do NOT widen literal types - return as-is
        type_id
    }

    /// Check if a type is an enum member type (not the parent enum type).
    ///
    /// Enum member types (e.g., `Colors.Red`) should widen to the parent enum type
    /// when assigned to mutable bindings, even if they're not "fresh" literals.
    pub(crate) fn is_enum_member_type_for_widening(&self, type_id: TypeId) -> bool {
        use tsz_solver::type_queries;

        if let Some(def_id) = type_queries::get_enum_def_id(self.ctx.types, type_id) {
            // Check if this DefId has a parent (meaning it's a member, not the enum itself)
            return self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .is_some_and(|env| env.get_enum_parent(def_id).is_some());
        }
        false
    }

    /// Check if an expression produces a "fresh" literal type that should be widened.
    ///
    /// In TypeScript, literal types created from literal expressions are "fresh" and get
    /// widened when assigned to mutable bindings (let/var). Literal types from other
    /// sources (variable references, type annotations, narrowing) are "non-fresh" and
    /// should NOT be widened.
    ///
    /// ## Examples:
    /// ```typescript
    /// let x = "foo";          // "foo" is fresh → widened to string
    /// let a: "foo" = "foo";
    /// let y = a;              // a's type is non-fresh → y: "foo" (not widened)
    /// let z = a || "bar";     // result from || is non-fresh → z: "foo" (not widened)
    /// ```
    pub(crate) fn is_fresh_literal_expression(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        let kind = node.kind;

        // Direct literal tokens are always fresh
        if kind == SyntaxKind::StringLiteral as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::BigIntLiteral as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::NullKeyword as u16
            || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            return true;
        }

        // Parenthesized expressions inherit freshness from inner expression
        if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = self.ctx.arena.get_parenthesized(node)
        {
            return self.is_fresh_literal_expression(paren.expression);
        }

        // Prefix unary (+/-) on numeric/bigint literals are fresh
        if kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(prefix) = self.ctx.arena.get_unary_expr(node)
        {
            let op = prefix.operator;
            if op == SyntaxKind::PlusToken as u16 || op == SyntaxKind::MinusToken as u16 {
                return self.is_fresh_literal_expression(prefix.operand);
            }
        }

        // Conditional expressions: fresh if either branch produces a fresh type.
        // E.g., `cond ? true : undefined` has a fresh `true` branch, so the
        // result type `true | undefined` should be widened to `boolean | undefined`.
        if kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
            && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
        {
            return self.is_fresh_literal_expression(cond.when_true)
                || self.is_fresh_literal_expression(cond.when_false);
        }

        // Object and array literals need widening (property types get widened)
        if kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            || kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        {
            return true;
        }

        // Template expressions (with substitutions) produce string, which doesn't need widening
        // but we mark them fresh for consistency
        if kind == syntax_kind_ext::TEMPLATE_EXPRESSION {
            return true;
        }

        // Everything else (identifiers, call expressions, binary expressions, etc.)
        // produces non-fresh types that should NOT be widened
        false
    }

    /// Map an expanded argument index back to the original argument node index.
    ///
    /// This handles spread arguments that expand to multiple elements.
    /// When a spread argument has a tuple type, it expands to multiple positional
    /// arguments. This function maps from the expanded index back to the original
    /// argument node for error reporting purposes.
    ///
    /// ## Parameters:
    /// - `args`: Slice of argument node indices
    /// - `expanded_index`: Index in the expanded argument list
    ///
    /// ## Returns:
    /// - `Some(NodeIndex)`: The original argument node index
    /// - `None`: If the index doesn't map to a valid argument
    ///
    /// ## Examples:
    /// ```typescript
    /// function foo(a: string, b: number, c: boolean) {}
    /// const tuple = ["hello", 42, true] as const;
    /// // Spread expands to 3 arguments: foo(...tuple)
    /// // expanded_index 0, 1, 2 all map to the spread argument node
    /// ```
    pub(crate) fn map_expanded_arg_index_to_original(
        &self,
        args: &[NodeIndex],
        expanded_index: usize,
    ) -> Option<NodeIndex> {
        let mut current_expanded_index = 0;

        for &arg_idx in args {
            if let Some(arg_node) = self.ctx.arena.get(arg_idx) {
                // Check if this is a spread element
                if arg_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    && let Some(spread_data) = self.ctx.arena.get_spread(arg_node)
                {
                    // Try to get the cached type, fall back to looking up directly
                    let spread_type = self
                        .ctx
                        .node_types
                        .get(&spread_data.expression.0)
                        .copied()
                        .unwrap_or(TypeId::ANY);
                    let spread_type = self.resolve_type_for_property_access_simple(spread_type);

                    // If it's a tuple type, it expands to multiple elements
                    if let Some(elems_id) = query::tuple_list_id(self.ctx.types, spread_type) {
                        let elems = self.ctx.types.tuple_list(elems_id);
                        let end_index = current_expanded_index + elems.len();
                        if expanded_index >= current_expanded_index && expanded_index < end_index {
                            // The error is within this spread - report at the spread node
                            return Some(arg_idx);
                        }
                        current_expanded_index = end_index;
                        continue;
                    }
                }
            }

            // Non-spread or non-tuple spread: takes one slot
            if expanded_index == current_expanded_index {
                return Some(arg_idx);
            }
            current_expanded_index += 1;
        }

        None
    }

    /// Simple type resolution for property access - doesn't trigger new type computation.
    ///
    /// This function resolves type applications to their base type without
    /// triggering expensive type computation. It's used in contexts where we
    /// just need the base type for inspection, not full type resolution.
    ///
    /// ## Examples:
    /// ```typescript
    /// type Box<T> = { value: T };
    /// // Box<string> resolves to Box for property access inspection
    /// ```
    fn resolve_type_for_property_access_simple(&self, type_id: TypeId) -> TypeId {
        query::application_base(self.ctx.types, type_id).unwrap_or(type_id)
    }

    pub(crate) fn lookup_symbol_with_name(
        &self,
        sym_id: SymbolId,
        name_hint: Option<&str>,
    ) -> Option<(&tsz_binder::Symbol, &tsz_parser::parser::node::NodeArena)> {
        let name_hint = name_hint.map(str::trim).filter(|name| !name.is_empty());

        if let Some(symbol) = self.ctx.binder.symbols.get(sym_id)
            && name_hint.is_none_or(|name| symbol.escaped_name == name)
        {
            let arena = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .map_or(self.ctx.arena, |arena| arena.as_ref());
            return Some((symbol, arena));
        }

        if let Some(name) = name_hint {
            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(symbol) = lib_ctx.binder.symbols.get(sym_id)
                    && symbol.escaped_name == name
                {
                    return Some((symbol, lib_ctx.arena.as_ref()));
                }
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.escaped_name == name
            {
                let arena = self
                    .ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .map_or(self.ctx.arena, |arena| arena.as_ref());
                return Some((symbol, arena));
            }
            return None;
        }

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            let arena = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .map_or(self.ctx.arena, |arena| arena.as_ref());
            return Some((symbol, arena));
        }

        for lib_ctx in &self.ctx.lib_contexts {
            if let Some(symbol) = lib_ctx.binder.symbols.get(sym_id) {
                return Some((symbol, lib_ctx.arena.as_ref()));
            }
        }

        None
    }

    /// Check if a symbol is value-only (has value but not type).
    ///
    /// This function distinguishes between symbols that can only be used as values
    /// vs. symbols that can be used as types. This is important for:
    /// - Import/export checking
    /// - Type position validation
    /// - Value expression validation
    ///
    /// ## Examples:
    /// ```typescript
    /// // Value-only symbols:
    /// const x = 42;  // x is value-only
    ///
    /// // Not value-only:
    /// type T = string;  // T is type-only
    /// interface Box {}  // Box is both type and value
    /// class Foo {}  // Foo is both type and value
    /// ```
    pub(crate) fn symbol_is_value_only(&self, sym_id: SymbolId, name_hint: Option<&str>) -> bool {
        let (symbol, arena) = match self.lookup_symbol_with_name(sym_id, name_hint) {
            Some(result) => result,
            None => return false,
        };

        // Fast path using symbol flags: if symbol has TYPE flag, it's not value-only
        // This handles classes, interfaces, enums, type aliases, etc.
        // TYPE flag includes: CLASS | INTERFACE | ENUM | ENUM_MEMBER | TYPE_LITERAL | TYPE_PARAMETER | TYPE_ALIAS
        let has_type_flag = (symbol.flags & symbol_flags::TYPE) != 0;
        if has_type_flag {
            return false;
        }

        // Modules/namespaces can be used as types in some contexts, but not if they're
        // merged with functions or other values (e.g., function+namespace declaration merging)
        // In such cases, the function/value takes precedence and TS2749 should be emitted
        let has_module = (symbol.flags & symbol_flags::MODULE) != 0;
        let has_function = (symbol.flags & symbol_flags::FUNCTION) != 0;
        // Exclude both FUNCTION and MODULE flags when checking for "other" value flags.
        // VALUE_MODULE is part of VALUE, but a symbol that only has module flags
        // (VALUE_MODULE | NAMESPACE_MODULE) should be treated as a pure namespace.
        let has_other_value = (symbol.flags
            & (symbol_flags::VALUE & !symbol_flags::FUNCTION & !symbol_flags::MODULE))
            != 0;

        // Pure namespace (MODULE only, no function/value flags) is not value-only
        if has_module && !has_function && !has_other_value {
            return false;
        }

        // Check declarations as a secondary source of truth (for cases where flags might not be set correctly)
        if self.symbol_has_type_declaration(symbol, arena) {
            return false;
        }

        // If the symbol is type-only (from `import type`), it's not value-only
        // In type positions, type-only imports should be allowed
        if symbol.is_type_only {
            return false;
        }

        // Finally, check if this is purely a value symbol (has VALUE but not TYPE)
        let has_value = (symbol.flags & symbol_flags::VALUE) != 0;
        let has_type = (symbol.flags & symbol_flags::TYPE) != 0;
        has_value && !has_type
    }

    /// Check if an alias resolves to a value-only symbol.
    ///
    /// This function follows alias chains to determine if the ultimate target
    /// is a value-only symbol. This is used for validating import/export aliases
    /// and type position checks.
    ///
    /// ## Examples:
    /// ```typescript
    /// // Original declarations
    /// const x = 42;
    /// type T = string;
    ///
    /// // Aliases
    /// import { x as xAlias } from "./mod";  // xAlias resolves to value-only
    /// import { type T as TAlias } from "./mod";  // TAlias is type-only
    /// ```
    pub(crate) fn alias_resolves_to_value_only(
        &self,
        sym_id: SymbolId,
        name_hint: Option<&str>,
    ) -> bool {
        let (symbol, _arena) = match self.lookup_symbol_with_name(sym_id, name_hint) {
            Some(result) => result,
            None => return false,
        };

        if symbol.flags & symbol_flags::ALIAS == 0 {
            return false;
        }

        // If the alias symbol itself is type-only, it doesn't resolve to value-only
        if symbol.is_type_only {
            return false;
        }

        let mut visited = Vec::new();
        let target = match self.resolve_alias_symbol(sym_id, &mut visited) {
            Some(target) => target,
            None => return false,
        };

        // symbol_is_value_only already checks TYPE flags and declarations
        // No need for redundant declaration check here
        let target_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        self.symbol_is_value_only(target, Some(target_name))
    }

    fn symbol_has_type_declaration(
        &self,
        symbol: &tsz_binder::Symbol,
        arena: &tsz_parser::parser::node::NodeArena,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        for &decl in &symbol.declarations {
            if decl.is_none() {
                continue;
            }
            let Some(node) = arena.get(decl) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => return true,
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => return true,
                k if k == syntax_kind_ext::CLASS_DECLARATION => return true,
                k if k == syntax_kind_ext::ENUM_DECLARATION => return true,
                _ => {}
            }
        }

        false
    }

    // ============================================================================
    // Section 54: Literal Key and Element Access Utilities
    // ============================================================================

    /// Extract literal keys from a type as string and number atom vectors.
    ///
    /// This function is used for element access type inference when the index
    /// type contains literal types. It extracts string and number literal values
    /// from single literals or unions of literals.
    ///
    /// ## Parameters:
    /// - `index_type`: The type to extract literal keys from
    ///
    /// ## Returns:
    /// - `Some((string_keys, number_keys))`: Tuple of string and number literal keys
    /// - `None`: If the type is not a literal or union of literals
    ///
    /// ## Examples:
    /// ```typescript
    /// // Single literal:
    /// type T1 = "foo";  // Returns: (["foo"], [])
    ///
    /// // Union of literals:
    /// type T2 = "a" | "b" | 1 | 2;  // Returns: (["a", "b"], [1.0, 2.0])
    ///
    /// // Non-literal type:
    /// type T3 = string;  // Returns: None
    /// ```
    pub(crate) fn get_literal_key_union_from_type(
        &self,
        index_type: TypeId,
    ) -> Option<(Vec<tsz_common::interner::Atom>, Vec<f64>)> {
        match query::literal_key_kind(self.ctx.types, index_type) {
            query::LiteralKeyKind::StringLiteral(atom) => Some((vec![atom], Vec::new())),
            query::LiteralKeyKind::NumberLiteral(num) => Some((Vec::new(), vec![num])),
            query::LiteralKeyKind::Union(members) => {
                let mut string_keys = Vec::with_capacity(members.len());
                let mut number_keys = Vec::new();
                for &member in &members {
                    match query::literal_key_kind(self.ctx.types, member) {
                        query::LiteralKeyKind::StringLiteral(atom) => string_keys.push(atom),
                        query::LiteralKeyKind::NumberLiteral(num) => number_keys.push(num),
                        _ => return None,
                    }
                }
                Some((string_keys, number_keys))
            }
            query::LiteralKeyKind::Other => {
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    index_type,
                )
                .and_then(|constraint| {
                    (constraint != index_type)
                        .then(|| self.get_literal_key_union_from_type(constraint))
                        .flatten()
                })
            }
        }
    }

    /// Get element access type for literal string keys.
    ///
    /// This function computes the type of element access when the index is a
    /// string literal or union of string literals. It handles both property
    /// access and numeric array indexing (when strings represent numeric indices).
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `keys`: Slice of string literal keys to look up
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The union of all property/element types
    /// - `None`: If any property is not found or if keys is empty
    ///
    /// ## Examples:
    /// ```typescript
    /// const obj = { a: 1, b: "hello" };
    /// type T = obj["a" | "b"];  // number | string
    ///
    /// const arr = [1, 2, 3];
    /// type U = arr["0" | "1"];  // number (treated as numeric index)
    /// ```
    pub(crate) fn get_element_access_type_for_literal_keys(
        &mut self,
        object_type: TypeId,
        keys: &[tsz_common::interner::Atom],
        is_write_context: bool,
    ) -> LiteralKeysResult {
        use crate::query_boundaries::common::PropertyAccessResult;

        if keys.is_empty() {
            return LiteralKeysResult {
                result_type: None,
                missing_keys: Vec::new(),
            };
        }

        // Resolve type references (Ref, TypeQuery, etc.) before property access lookup
        let resolved_type = self.resolve_type_for_property_access(object_type);
        if resolved_type == TypeId::ANY {
            return LiteralKeysResult {
                result_type: Some(TypeId::ANY),
                missing_keys: Vec::new(),
            };
        }
        if resolved_type == TypeId::ERROR {
            return LiteralKeysResult {
                result_type: None,
                missing_keys: Vec::new(),
            };
        }

        let numeric_as_index = self.is_array_like_type(resolved_type);
        let mut types = Vec::with_capacity(keys.len());
        let mut missing_keys = Vec::new();

        for &key in keys {
            let name = self.ctx.types.resolve_atom(key);
            if numeric_as_index && let Some(index) = self.get_numeric_index_from_string(&name) {
                let element_type =
                    self.get_element_access_type(resolved_type, TypeId::NUMBER, Some(index));
                types.push(element_type);
                continue;
            }

            match self.ctx.types.property_access_type(resolved_type, &name) {
                PropertyAccessResult::Success {
                    type_id,
                    write_type,
                    ..
                } => {
                    // In write context (assignment target), use the write/setter type.
                    let effective = if is_write_context {
                        write_type.unwrap_or(type_id)
                    } else {
                        type_id
                    };
                    types.push(effective);
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    types.push(property_type.unwrap_or(TypeId::UNKNOWN));
                }
                // IsUnknown: Return immediately — the caller has node context and
                // will report TS2571 error.
                PropertyAccessResult::IsUnknown => {
                    return LiteralKeysResult {
                        result_type: None,
                        missing_keys: Vec::new(),
                    };
                }
                // PropertyNotFound: Track the missing key instead of bailing out.
                // tsc emits TS2339 per missing key, not TS7053 for the whole union.
                PropertyAccessResult::PropertyNotFound { .. } => {
                    missing_keys.push(name.to_string());
                }
            }
        }

        // In write context, the value must be assignable to ALL possible property types
        // (intersection), since we don't know which key will be used at runtime.
        // In read context, the result is ANY of the property types (union).
        let result_type = if types.is_empty() {
            None
        } else if is_write_context {
            let intersection = tsz_solver::utils::intersection_or_single(self.ctx.types, types);
            Some(self.evaluate_type_with_env(intersection))
        } else {
            Some(tsz_solver::utils::union_or_single(self.ctx.types, types))
        };

        LiteralKeysResult {
            result_type,
            missing_keys,
        }
    }

    /// Get element access type for literal number keys.
    ///
    /// This function computes the type of element access when the index is a
    /// number literal or union of number literals. It handles array/tuple
    /// indexing with literal numeric values.
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `keys`: Slice of numeric literal keys to look up
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The union of all element types
    /// - `None`: If keys is empty
    ///
    /// ## Examples:
    /// ```typescript
    /// const arr = [1, "hello", true];
    /// type T = arr[0 | 1];  // number | string
    ///
    /// const tuple = [1, 2] as const;
    /// type U = tuple[0 | 1];  // 1 | 2
    /// ```
    pub(crate) fn get_element_access_type_for_literal_number_keys(
        &mut self,
        object_type: TypeId,
        keys: &[f64],
        is_write_context: bool,
    ) -> Option<TypeId> {
        if keys.is_empty() {
            return None;
        }

        let mut types = Vec::with_capacity(keys.len());
        for &value in keys {
            if let Some(index) = self.get_numeric_index_from_number(value) {
                types.push(self.get_element_access_type(object_type, TypeId::NUMBER, Some(index)));
            } else {
                return Some(self.get_element_access_type(object_type, TypeId::NUMBER, None));
            }
        }

        // In write context, intersect (value must satisfy all possible indices).
        if is_write_context {
            let intersection = tsz_solver::utils::intersection_or_single(self.ctx.types, types);
            Some(self.evaluate_type_with_env(intersection))
        } else {
            Some(tsz_solver::utils::union_or_single(self.ctx.types, types))
        }
    }

    /// Check if a type is array-like (supports numeric indexing).
    ///
    /// This function determines if a type supports numeric element access,
    /// including arrays, tuples, and unions/intersections of array-like types.
    ///
    /// ## Array-like Types:
    /// - Array types: `T[]`, `Array<T>`
    /// - Tuple types: `[T1, T2, ...]`
    /// - Readonly arrays: `readonly T[]`, `ReadonlyArray<T>`
    /// - Unions where all members are array-like
    /// - Intersections where any member is array-like
    ///
    /// ## Examples:
    /// ```typescript
    /// // Array-like types:
    /// type A = number[];
    /// type B = [string, number];
    /// type C = readonly boolean[];
    /// type D = A | B;  // Union of array-like types
    ///
    /// // Not array-like:
    /// type E = { [key: string]: number };  // Index signature, not array-like
    /// ```
    pub(crate) fn is_array_like_type(&self, object_type: TypeId) -> bool {
        let object_type = self.ctx.types.evaluate_type(object_type);
        // Check for array/tuple types directly
        if crate::query_boundaries::checkers::iterable::is_array_type(self.ctx.types, object_type) {
            return true;
        }

        match query::classify_array_like(self.ctx.types, object_type) {
            query::ArrayLikeKind::Array(_) | query::ArrayLikeKind::Tuple => true,
            query::ArrayLikeKind::Readonly(inner) => self.is_array_like_type(inner),
            query::ArrayLikeKind::Union(members) => members
                .iter()
                .all(|&member| self.is_array_like_type(member)),
            query::ArrayLikeKind::Intersection(members) => members
                .iter()
                .any(|&member| self.is_array_like_type(member)),
            query::ArrayLikeKind::Other => self.type_has_array_like_heritage(object_type),
        }
    }

    fn type_has_array_like_heritage(&self, type_id: TypeId) -> bool {
        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id).or_else(|| {
            // Delegate to solver query for object symbol extraction
            crate::query_boundaries::common::object_symbol(self.ctx.types, type_id)
        });
        let Some(sym_id) = sym_id else {
            return false;
        };
        let mut visited = Vec::new();
        self.symbol_has_array_like_heritage(sym_id, &mut visited)
    }

    fn symbol_has_array_like_heritage(
        &self,
        sym_id: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> bool {
        if visited.contains(&sym_id) {
            return false;
        }
        visited.push(sym_id);

        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            visited.pop();
            return false;
        };

        if Self::is_builtin_array_like_name(symbol.escaped_name.as_str()) {
            visited.pop();
            return true;
        }

        let mut decls = symbol.declarations.clone();
        let value_decl = symbol.value_declaration;
        if value_decl != NodeIndex::NONE && !decls.contains(&value_decl) {
            decls.push(value_decl);
        }

        for decl_idx in decls {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let heritage_clauses = if let Some(interface) = self.ctx.arena.get_interface(node) {
                interface.heritage_clauses.as_ref()
            } else if let Some(class_decl) = self.ctx.arena.get_class(node) {
                class_decl.heritage_clauses.as_ref()
            } else {
                None
            };

            let Some(heritage_clauses) = heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    let expr_idx = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        self.ctx
                            .arena
                            .get_type_ref(type_node)
                            .map(|type_ref| type_ref.type_name)
                            .unwrap_or(type_idx)
                    } else {
                        type_idx
                    };

                    if let Some(base_name) = self.heritage_name_text(expr_idx)
                        && Self::is_builtin_array_like_name(base_name.as_str())
                    {
                        visited.pop();
                        return true;
                    }

                    if let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx)
                        && self.symbol_has_array_like_heritage(base_sym_id, visited)
                    {
                        visited.pop();
                        return true;
                    }
                }
            }
        }

        visited.pop();
        false
    }

    fn is_builtin_array_like_name(name: &str) -> bool {
        matches!(
            name.rsplit('.').next().unwrap_or(name),
            "Array" | "ReadonlyArray" | "ConcatArray"
        )
    }

    /// Check if an index signature error should be reported for element access.
    ///
    /// This function determines whether a "No index signature" error should be
    /// emitted for element access on an object type. This happens when:
    /// - The object type doesn't have an appropriate index signature
    /// - The index type is a literal or union of literals
    /// - The access is not valid property access
    ///
    /// ## Parameters:
    /// - `object_type`: The type of the object being accessed
    /// - `index_type`: The type of the index expression
    /// - `literal_index`: Optional explicit numeric index
    ///
    /// ## Returns:
    /// - `true`: Report "No index signature" error
    /// - `false`: Don't report (has index signature, or any/unknown type)
    ///
    /// ## Examples:
    /// ```typescript
    /// const obj = { a: 1, b: 2 };
    /// obj["c"];  // Error: No index signature with parameter of type '"c"'
    ///
    /// const obj2: { [key: string]: number } = { a: 1 };
    /// obj2["c"];  // OK: Has string index signature
    /// ```
    pub(crate) fn should_report_no_index_signature(
        &self,
        object_type: TypeId,
        index_type: TypeId,
        literal_index: Option<usize>,
    ) -> bool {
        if object_type == TypeId::ANY
            || object_type == TypeId::UNKNOWN
            || object_type == TypeId::ERROR
        {
            return false;
        }

        // `unknown` index type can't trigger TS7053 — it's not usable as an index.
        if index_type == TypeId::UNKNOWN {
            return false;
        }

        // For type parameters with a concrete (non-generic) constraint, check the
        // constraint's indexability. tsc reports TS7053 when the constraint lacks
        // the needed index signature (e.g., T extends Item, obj[string] → TS7053).
        // Only do this when the index type is concrete (e.g., `string`, a literal).
        // When the index is itself a type parameter (e.g., K extends keyof T), the
        // generic element resolution path already handles it via deferred IndexAccess.
        // Also skip when the constraint contains type parameters (e.g., Record<K, number>)
        // since indexability depends on the instantiation.
        let check_type = if let Some(constraint) =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, object_type)
        {
            if tsz_solver::visitor::is_type_parameter(self.ctx.types, index_type)
                || tsz_solver::visitor::contains_type_parameters(self.ctx.types, constraint)
            {
                // Constraint is generic or index is generic — can't determine
                // indexability until instantiation. Don't report TS7053.
                return false;
            }
            constraint
        } else {
            object_type
        };

        if check_type == TypeId::ANY || check_type == TypeId::UNKNOWN || check_type == TypeId::ERROR
        {
            return false;
        }

        // `any` index type: tsc reports TS7053 when noImplicitAny is on and the
        // object lacks an index signature. Treat `any` as wanting both string and
        // number indexing — if the object supports neither, a diagnostic should fire.
        let (wants_string, wants_number) = if index_type == TypeId::ANY {
            (true, true)
        } else {
            let index_key_kind = self.get_index_key_kind(index_type);
            let wants_number = literal_index.is_some()
                || index_key_kind
                    .as_ref()
                    .is_some_and(|(_, wants_number)| *wants_number);
            let wants_string = index_key_kind
                .as_ref()
                .is_some_and(|(wants_string, _)| *wants_string);
            (wants_string, wants_number)
        };
        if !wants_number && !wants_string {
            return false;
        }

        let unwrapped_type = query::unwrap_readonly_for_lookup(self.ctx.types, check_type);

        !self.is_element_indexable(unwrapped_type, wants_string, wants_number)
    }

    /// Determine what kind of index key a type represents.
    ///
    /// This function analyzes a type to determine if it can be used for string
    /// or numeric indexing. Returns a tuple of (`wants_string`, `wants_number`).
    ///
    /// ## Returns:
    /// - `Some((true, false))`: String index (e.g., `"foo"`, `string`)
    /// - `Some((false, true))`: Number index (e.g., `42`, `number`)
    /// - `Some((true, true))`: Both string and number (e.g., `"a" | 1 | 2`)
    /// - `None`: Not an index type
    ///
    /// ## Examples:
    /// ```typescript
    /// type A = "foo";        // (true, false) - string literal
    /// type B = 42;           // (false, true) - number literal
    /// type C = string;       // (true, false) - string type
    /// type D = "a" | "b";    // (true, false) - union of strings
    /// type E = "a" | 1;      // (true, true) - mixed literals
    /// ```
    pub(crate) fn get_index_key_kind(&self, index_type: TypeId) -> Option<(bool, bool)> {
        match query::classify_index_key(self.ctx.types, index_type) {
            query::IndexKeyKind::String
            | query::IndexKeyKind::StringLiteral
            | query::IndexKeyKind::TemplateLiteralString => Some((true, false)),
            query::IndexKeyKind::Number | query::IndexKeyKind::NumberLiteral => Some((false, true)),
            // `${number}` is a numeric string type — valid for both string and number
            // index signatures. Arrays have number index signatures, and objects may
            // have string index signatures, so this type can index both.
            query::IndexKeyKind::NumericStringLike => Some((true, true)),
            query::IndexKeyKind::Union(members) => {
                let mut wants_string = false;
                let mut wants_number = false;
                for member in members {
                    let (member_string, member_number) = self.get_index_key_kind(member)?;
                    wants_string |= member_string;
                    wants_number |= member_number;
                }
                Some((wants_string, wants_number))
            }
            query::IndexKeyKind::Other => {
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    index_type,
                )
                .and_then(|constraint| {
                    (constraint != index_type).then(|| self.get_index_key_kind(constraint))
                })
                .flatten()
            }
        }
    }

    /// Check if a type key supports element indexing.
    ///
    /// This function determines if a type supports element access with the
    /// specified index kind (string, number, or both).
    ///
    /// ## Parameters:
    /// - `object_key`: The type key to check
    /// - `wants_string`: Whether string indexing is needed
    /// - `wants_number`: Whether numeric indexing is needed
    ///
    /// ## Returns:
    /// - `true`: The type supports the requested indexing
    /// - `false`: The type does not support the requested indexing
    ///
    /// ## Examples:
    /// ```typescript
    /// // Array supports numeric indexing:
    /// const arr: number[] = [1, 2, 3];
    /// arr[0];  // OK
    ///
    /// // Object with string index supports string indexing:
    /// const obj: { [key: string]: number } = {};
    /// obj["foo"];  // OK
    ///
    /// // Object without index signature doesn't support indexing:
    /// const plain: { a: number } = { a: 1 };
    /// plain["b"];  // Error: No index signature
    /// ```
    pub(crate) fn is_element_indexable(
        &self,
        object_type: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        match query::classify_element_indexable(self.ctx.types, object_type) {
            query::ElementIndexableKind::Array
            | query::ElementIndexableKind::Tuple
            | query::ElementIndexableKind::StringLike => wants_number,
            query::ElementIndexableKind::ObjectWithIndex {
                has_string,
                has_number,
            } => (wants_string && has_string) || (wants_number && (has_number || has_string)),
            query::ElementIndexableKind::Union(members) => members
                .iter()
                .all(|&member| self.is_element_indexable(member, wants_string, wants_number)),
            query::ElementIndexableKind::Intersection(members) => members
                .iter()
                .any(|&member| self.is_element_indexable(member, wants_string, wants_number)),
            query::ElementIndexableKind::Other => false,
        }
    }
}
