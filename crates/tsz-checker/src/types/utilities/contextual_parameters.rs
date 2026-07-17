//! Contextual parameter and callable-contextual type resolution methods for
//! `CheckerState`. Extracted from `utilities/core.rs` to keep that module
//! under the 2000-LOC checker boundary; behavior is unchanged.

use crate::query_boundaries::checkers::{
    class_properties as class_property_query, parameters as parameter_query,
};
use crate::query_boundaries::common::ContextualTypeContext;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, node::PropertyDeclData};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
            let rest_start = shape.params.len().saturating_sub(1);
            if shape.params.len() == 1 && index > 0 {
                let rest_param_type =
                    self.contextual_rest_parameter_source_type(rest_param.type_id);
                if let Some(tuple_elements) =
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, rest_param_type)
                {
                    if tuple_elements.len() > index {
                        return Some(parameter_query::tuple_type_from_element_slice(
                            self.ctx.types,
                            &tuple_elements[index..],
                        ));
                    }
                    if let Some(last) = tuple_elements.last()
                        && last.rest
                    {
                        return Some(last.type_id);
                    }
                }
            }
            if index < rest_start {
                return Some(parameter_query::contextual_rest_tuple_from_signature_tail(
                    self.ctx.types,
                    &shape.params,
                    index,
                    rest_start,
                    rest_param,
                ));
            }
            // For rest parameters aligned with the contextual rest, preserve the
            // original type (including type parameters like `Args extends any[]`).
            if crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                rest_param.type_id,
            ) || crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                rest_param.type_id,
            ) {
                return Some(rest_param.type_id);
            }
            return None;
        }
        // For non-rest callback parameters, only look into the rest tuple when the
        // parameter index falls within the rest range of the contextual signature.
        // If the index is within the regular (non-rest) params range, return None so
        // the regular parameter extraction path handles it correctly.
        // Example: callback `(a, b, ...x)` vs contextual `(x: number, ...args: T)`:
        //   - `a` at index 0 should get `number` from the regular param `x: number`,
        //     not `any` from the rest constraint — return None here.
        let rest_start = shape.params.len().saturating_sub(1);
        if index < rest_start {
            return None;
        }
        // Within the rest range, map to the tuple element at (index - rest_start)
        // so that positional mapping is correct when there are regular params before
        // the rest param.
        // Example: callback `(b, c)` vs `(a: A, ...args: [B, C])`:
        //   - `b` at callback index 1, rest_start=1 → tuple index 0 → B ✓
        //   - `c` at callback index 2, rest_start=1 → tuple index 1 → C ✓
        let tuple_index = index - rest_start;

        let rest_param_type = self.contextual_rest_parameter_source_type(rest_param.type_id);

        if let Some(tuple_elements) =
            crate::query_boundaries::common::tuple_elements(self.ctx.types, rest_param_type)
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

            if let Some(element) = tuple_elements.get(tuple_index) {
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
                    crate::query_boundaries::common::tuple_elements(self.ctx.types, member)
                else {
                    continue;
                };
                if let Some(element) = tuple_elements.get(tuple_index) {
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
                return Some(parameter_query::union_type(self.ctx.types, element_types));
            }
        }

        crate::query_boundaries::common::array_element_type(self.ctx.types, rest_param_type)
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
        if crate::query_boundaries::common::is_union_type(self.ctx.types, expected)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, expected)
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
        self.resolve_jsdoc_import_member_with_mode(module_specifier, member_name, None)
    }

    /// Like [`Self::resolve_jsdoc_import_member`] but honors an explicit
    /// `resolution-mode` override carried by a JSDoc `@import ... with { ... }`
    /// tag, so the member is looked up against the ESM/CJS conditional export
    /// `tsc` would resolve for that mode.
    pub(crate) fn resolve_jsdoc_import_member_with_mode(
        &self,
        module_specifier: &str,
        member_name: &str,
        resolution_mode: Option<crate::context::ResolutionModeOverride>,
    ) -> Option<SymbolId> {
        self.resolve_cross_file_export_from_file_with_mode(
            module_specifier,
            member_name,
            Some(self.ctx.current_file_idx),
            resolution_mode,
        )
        // Avoid raw binder fallback here: it returns unscoped SymbolIds without
        // file-target registration, which can alias-collide across binders.
        .or_else(|| self.resolve_cross_file_export(module_specifier, member_name))
    }

    pub(crate) fn effective_class_property_declared_type(
        &mut self,
        member_idx: NodeIndex,
        prop: &PropertyDeclData,
    ) -> Option<TypeId> {
        let raw = self.class_property_relation_declared_type(member_idx, prop)?;
        Some(self.wrap_static_readonly_unique_symbol_type(member_idx, prop, raw))
    }

    /// Widen a readonly class field's un-annotated bare `unique symbol` *alias*
    /// initializer to `symbol`, matching tsc's `getWidenedUniqueESSymbolType`
    /// (the `!declaration.type && type.symbol !== getSymbolOfDeclaration` rule it
    /// applies to a field the same as a variable). A freshly minted
    /// `static readonly f = Symbol()` — whose unique symbol's owning symbol *is*
    /// the field — keeps its `typeof f` identity, and an annotated field returns
    /// its annotation type before reaching the initializer path. A mutable field
    /// already widens through the freshness path, so only the readonly branch
    /// needs this. Bare-only (`is_unique_symbol_type`), so a union member is
    /// preserved.
    pub(crate) fn widen_readonly_field_unique_symbol_alias(
        &self,
        member_idx: NodeIndex,
        init_type: TypeId,
    ) -> TypeId {
        if !crate::query_boundaries::common::is_unique_symbol_type(self.ctx.types, init_type) {
            return init_type;
        }
        let Some(sym_ref) =
            crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, init_type)
        else {
            return init_type;
        };
        let Some(field_sym) = self.ctx.binder.get_node_symbol(member_idx) else {
            return init_type;
        };
        if sym_ref.0 == field_sym.0 {
            return init_type;
        }
        TypeId::SYMBOL
    }

    /// Like [`effective_class_property_declared_type`] but returns the raw
    /// annotation type, without the `static readonly: unique symbol` wrap.
    /// Used at the declaration-site initializer assignability check, where
    /// the relation must compare against the lowered `symbol` form so a
    /// fresh-symbol initializer (`Symbol()`) is accepted.
    pub(crate) fn class_property_relation_declared_type(
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

    /// The assignability target for a class property's declaration-site
    /// initializer. An optional property (`prop?: T`) has declared type
    /// `T | undefined` under `strictNullChecks` without
    /// `exactOptionalPropertyTypes`, so its initializer (e.g. `= undefined`) is
    /// checked against `T | undefined`; otherwise the bare declared type is used.
    /// Scoped to the initializer relation — it does not change the member's
    /// stored type, the contextual type, or the excess-property shape. (#14737)
    pub(crate) fn class_property_init_relation_target(
        &mut self,
        prop: &PropertyDeclData,
        declared_type: TypeId,
    ) -> TypeId {
        if prop.question_token
            && declared_type != TypeId::ANY
            && declared_type != TypeId::ERROR
            && self.ctx.strict_null_checks()
            && !self.ctx.exact_optional_property_types()
        {
            class_property_query::class_property_optional_type_with_undefined(
                self.ctx.types,
                declared_type,
            )
        } else {
            declared_type
        }
    }

    /// Lift a `symbol`-typed `static readonly p: unique symbol` annotation to
    /// `unique_symbol(SymbolRef(prop_sym))` so downstream `typeof Class.p`
    /// queries see a distinct unique-symbol identity, mirroring the wrapping
    /// that `get_type_of_variable_declaration` applies to const variables.
    fn wrap_static_readonly_unique_symbol_type(
        &mut self,
        member_idx: NodeIndex,
        prop: &PropertyDeclData,
        raw_type: TypeId,
    ) -> TypeId {
        if raw_type != TypeId::SYMBOL
            || !self.has_static_modifier(&prop.modifiers)
            || !self.has_readonly_modifier(&prop.modifiers)
            || !self.is_unique_symbol_type_annotation(prop.type_annotation)
        {
            return raw_type;
        }
        let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx) else {
            return raw_type;
        };
        class_property_query::static_readonly_unique_symbol_type(self.ctx.types, sym_id)
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
                    t = parameter_query::optional_parameter_type_with_undefined(self.ctx.types, t);
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
                            {
                                let jsdoc_param_names: Vec<String> =
                                    Self::extract_jsdoc_param_names(&func_jsdoc)
                                        .into_iter()
                                        .map(|(name, _)| name)
                                        .collect();
                                let pname = self.effective_jsdoc_param_name(
                                    param.name,
                                    &jsdoc_param_names,
                                    i,
                                );
                                if let Some(t) = self.resolve_jsdoc_param_type_with_pos(
                                    &func_jsdoc,
                                    &pname,
                                    Some(comment_start),
                                ) {
                                    found = Some(t);
                                    break;
                                }
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
                                        use crate::query_boundaries::common::ContextualTypeContext;
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
                    if let Some(existing) = self.ctx.symbol_types.get(&sym_id)
                        && existing != TypeId::ERROR
                        && type_id != existing
                        && type_id.is_any_unknown_or_error()
                    {
                        continue;
                    }
                    // When called without pre-computed param_types (None path),
                    // don't overwrite a parameter type that was already cached by
                    // get_type_of_function (which computes types from initializer
                    // expressions in JS files). Only overwrite if the existing
                    // cached type is absent or is a placeholder (ERROR). Explicit
                    // parameter annotations remain authoritative.
                    if param_types.is_none()
                        && param.type_annotation.is_none()
                        && let Some(existing) = self.ctx.symbol_types.get(&sym_id)
                        && existing != TypeId::ERROR
                    {
                        continue;
                    }
                    self.cache_symbol_type(sym_id, type_id);
                }
            }
        }
    }

    /// Resolve a parameter's contextual type from its enclosing function's
    /// contextual type. When `extended` is set, the enclosing function's
    /// contextual type is additionally recovered from broader syntactic
    /// positions (return, parameter default, class member, assignment,
    /// object-literal property, conditional) — used only by the TS2403
    /// redeclaration baseline, never by the parameter's checked type.
    pub(crate) fn contextual_parameter_type_from_enclosing_function(
        &mut self,
        param_idx: NodeIndex,
        extended: bool,
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
        } else {
            let method = self
                .ctx
                .arena
                .get_method_decl(self.ctx.arena.get(function_idx)?)?;
            &method.parameters.nodes
        };

        let param_position = parameters.iter().position(|&idx| idx == param_idx)?;
        let contextual_index = parameters[..param_position]
            .iter()
            .filter(|&&idx| {
                self.ctx
                    .arena
                    .get(idx)
                    .and_then(|node| self.ctx.arena.get_parameter(node))
                    .is_none_or(|p| !self.is_this_parameter_name(p.name))
            })
            .count();

        // Baseline-only: the syntactic contextual position (var/return/parameter/
        // class-member/assignment/object-property/call-argument) is the authoritative
        // declared type for the TS2403 redeclaration baseline, so it must win over the
        // parameter/arrow's own inferred/cached type. Gated on `extended`, so the
        // non-baseline caller keeps the original source order exactly.
        let contextual_type = extended
            .then(|| self.expression_contextual_type_annotation(function_idx))
            .flatten()
            .or(self.ctx.contextual_type)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_node_symbol(function_idx)
                    .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id))
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
        let helper = ContextualTypeContext::with_expected_and_options(
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
            && !crate::query_boundaries::common::type_contains_undefined(self.ctx.types, ty)
        {
            ty = parameter_query::optional_parameter_type_with_undefined(self.ctx.types, ty);
        }

        Some(ty)
    }

    /// The contextual type an expression node receives from its syntactic
    /// position, walking outward through parenthesized/conditional wrappers.
    /// Used only by the TS2403 redeclaration baseline to recover the declared
    /// type of a contextually-typed parameter (no annotation); it must not
    /// change the parameter's checked type. Mirrors the positions tsc
    /// propagates a contextual type through.
    fn expression_contextual_type_annotation(&mut self, expr_idx: NodeIndex) -> Option<TypeId> {
        self.expression_contextual_type_annotation_rec(expr_idx, 0)
    }

    fn expression_contextual_type_annotation_rec(
        &mut self,
        expr_idx: NodeIndex,
        depth: u8,
    ) -> Option<TypeId> {
        if depth > 8 {
            return None;
        }
        let parent_idx = self.ctx.arena.get_extended(expr_idx)?.parent;
        let parent_kind = self.ctx.arena.get(parent_idx)?.kind;
        match parent_kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let vd = self.ctx.arena.get_variable_declaration_at(parent_idx)?;
                (vd.initializer == expr_idx && vd.type_annotation.is_some())
                    .then(|| self.get_type_from_type_node(vd.type_annotation))
            }
            syntax_kind_ext::PARAMETER => {
                let p = self.ctx.arena.get_parameter_at(parent_idx)?;
                (p.initializer == expr_idx && p.type_annotation.is_some())
                    .then(|| self.get_type_from_type_node(p.type_annotation))
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let pd = self.ctx.arena.get_property_decl_at(parent_idx)?;
                (pd.initializer == expr_idx && pd.type_annotation.is_some())
                    .then(|| self.get_type_from_type_node(pd.type_annotation))
            }
            syntax_kind_ext::RETURN_STATEMENT => {
                let ann = self.enclosing_function_return_annotation_node(parent_idx)?;
                ann.is_some().then(|| self.get_type_from_type_node(ann))
            }
            syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.ctx.arena.get_binary_expr_at(parent_idx)?;
                if bin.right != expr_idx || !self.is_assignment_operator(bin.operator_token) {
                    return None;
                }
                let ty = self.get_type_of_node(bin.left);
                (ty != TypeId::ERROR).then_some(ty)
            }
            syntax_kind_ext::PARENTHESIZED_EXPRESSION | syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                self.expression_contextual_type_annotation_rec(parent_idx, depth + 1)
            }
            syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let pa = self.ctx.arena.get_property_assignment_at(parent_idx)?;
                if pa.initializer != expr_idx {
                    return None;
                }
                let name = self.get_property_name_resolved(pa.name)?;
                let obj_idx = self.ctx.arena.get_extended(parent_idx)?.parent;
                let obj_ctx = self.expression_contextual_type_annotation_rec(obj_idx, depth + 1)?;
                // Evaluate so a `Lazy`/unevaluated annotation object type exposes its
                // members to the structural property lookup.
                let obj_ctx = self.evaluate_contextual_type(obj_ctx);
                let atom = self.ctx.types.intern_string(&name);
                let db = self.ctx.types.as_type_database();
                crate::query_boundaries::common::raw_property_type(db, obj_ctx, atom)
            }
            syntax_kind_ext::CALL_EXPRESSION => {
                // Callback argument: the expression's contextual type is the
                // callee parameter type at this argument's position.
                let (callee_idx, arg_index) = {
                    let call = self.ctx.arena.get_call_expr_at(parent_idx)?;
                    let args = call.arguments.as_ref()?;
                    let arg_index = args.nodes.iter().position(|&a| a == expr_idx)?;
                    (call.expression, arg_index)
                };
                let callee_ty = self.get_type_of_node(callee_idx);
                if callee_ty == TypeId::ERROR {
                    return None;
                }
                // Resolve a `Lazy`/unevaluated callee type so its call signature
                // exposes its parameters. The parameter at this argument's index is
                // the callback's contextual type (`get_parameter_type` reads the
                // first/only call signature — the first-applicable signature for the
                // simple non-overloaded callees this baseline targets; overload
                // selection is not reattempted here, so an overloaded callee that
                // yields no parameter simply leaves the baseline unchanged).
                let callee_ty = self.evaluate_contextual_type(callee_ty);
                let helper = ContextualTypeContext::with_expected_and_options(
                    self.ctx.types,
                    callee_ty,
                    self.ctx.compiler_options.no_implicit_any,
                );
                helper.get_parameter_type(arg_index)
            }
            _ => None,
        }
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
        let helper = ContextualTypeContext::with_expected_and_options(
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
                            if !last.rest {
                                return None;
                            }
                            let rest_start = shape.params.len().saturating_sub(1);
                            // For fixed-length (non-variadic) tuple rest params, compute
                            // the remaining slice after the callback's regular params have
                            // consumed earlier elements.  When all elements are consumed the
                            // slice is empty, so we return `[]` — not the full tuple — which
                            // correctly models the callback's rest arity.
                            // Example: `(a, b, c, ...x)` vs `(...args: [A, B, C])` at index=3:
                            //   rest_start=0, consumed=3, remaining=[] → `...x: []` (no error).
                            if let Some(elements) = crate::query_boundaries::common::tuple_elements(
                                self.ctx.types,
                                last.type_id,
                            ) {
                                let has_variadic = elements.iter().any(|e| e.rest);
                                if !has_variadic {
                                    let consumed = index.saturating_sub(rest_start);
                                    let remaining =
                                        elements[consumed.min(elements.len())..].to_vec();
                                    return Some(parameter_query::tuple_type_from_elements(
                                        self.ctx.types,
                                        remaining,
                                    ));
                                }
                            }
                            Some(last.type_id)
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
                // tsc derives the contextual signature for a function expression
                // from a union of function types by discarding members whose
                // call signature is arity-smaller than the expression
                // (`isAritySmaller`). When exactly one member can accept the
                // callback's arity, the contextual signature -- and thus every
                // parameter type -- comes solely from that member. Without this,
                // a union of plain single-signature function types (e.g.
                // `MethodDecorator | PropertyDecorator | ClassDecorator`, where
                // only the 3-parameter `MethodDecorator` can accept a
                // 3-parameter callback) yields no contextual type for any
                // parameter and spuriously reports TS7006, because the
                // per-member mixed-overload path below only handles members
                // carrying two or more overload signatures. The 2-or-more
                // survivor case is left to the existing per-member logic, which
                // preserves tsc's "members must agree, else implicit any".
                // `evaluated_member` already equals `member` when evaluation
                // produced no change, so it is the effective member type.
                let mut arity_viable_members: Vec<TypeId> = Vec::new();
                for &(_, evaluated_member) in &evaluated_members {
                    if self.callable_member_accepts_callback_arity(evaluated_member, arg_count) {
                        arity_viable_members.push(evaluated_member);
                    }
                }
                if arity_viable_members.len() == 1
                    && let Some(param_type) = self.contextual_callable_member_param_type_for_call(
                        arity_viable_members[0],
                        index,
                        arg_count,
                    )
                {
                    return Some(param_type);
                }
                let contextual_members: Vec<_> = evaluated_members
                    .into_iter()
                    .filter_map(|(member, evaluated_member)| {
                        let target_member = if evaluated_member == member {
                            member
                        } else {
                            evaluated_member
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
                    _ => {
                        // When all collected types are callable (e.g., callback types from
                        // mixed-overload union members like `(A[] | B[]).filter(cb)`),
                        // merge them into a single combined callable with unioned parameters.
                        // This matches tsc's behavior: the intersection of function types
                        // produces a combined function with unioned parameter types
                        // (contravariance), enabling correct contextual typing for callbacks.
                        // Without this, the union of callback types causes get_parameter_type
                        // to return None (param types disagree across members), yielding `any`.
                        if let Some(merged) = parameter_query::merge_callable_contextual_types(
                            self.ctx.types,
                            &contextual_members,
                        ) {
                            Some(merged)
                        } else {
                            Some(parameter_query::union_preserve_members_type(
                                self.ctx.types,
                                contextual_members,
                            ))
                        }
                    }
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
        let helper = ContextualTypeContext::with_expected_and_options(
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
        if expected.is_intrinsic() {
            return expected;
        }
        let lazy_failures_at_entry = crate::query_boundaries::common::lazy_resolve_failure_count();
        if !self.contextual_signature_normalization_session_clean(lazy_failures_at_entry) {
            return self.normalize_contextual_signature_with_env_uncached(expected);
        }
        let Some(eval_stamp) = self.assignability_eval_memo_stamp() else {
            return self.normalize_contextual_signature_with_env_uncached(expected);
        };
        let stamp = (
            eval_stamp,
            self.ctx.compiler_options.no_unchecked_indexed_access,
            self.ctx.compiler_options.exact_optional_property_types,
            self.ctx.compiler_options.strict_null_checks,
            self.ctx.compiler_options.no_implicit_any,
        );
        if let Some(cached) = self
            .ctx
            .lookup_contextual_signature_normalization_cache(expected, stamp)
        {
            return cached;
        }
        let normalized = self.normalize_contextual_signature_with_env_uncached(expected);
        let stamp_after = self.assignability_eval_memo_stamp().map(|eval_stamp| {
            (
                eval_stamp,
                self.ctx.compiler_options.no_unchecked_indexed_access,
                self.ctx.compiler_options.exact_optional_property_types,
                self.ctx.compiler_options.strict_null_checks,
                self.ctx.compiler_options.no_implicit_any,
            )
        });
        if self.contextual_signature_normalization_session_clean(lazy_failures_at_entry)
            && stamp_after == Some(stamp)
            && self.contextual_signature_normalization_cacheable(expected, normalized)
        {
            self.ctx
                .cache_contextual_signature_normalization_result(expected, stamp, normalized);
        }
        normalized
    }

    fn contextual_signature_normalization_session_clean(
        &self,
        lazy_failures_at_entry: u64,
    ) -> bool {
        !self.ctx.eval_session.refs_resolution_fuel_exhausted()
            && !self.ctx.eval_session.lazy_resolution_fuel_exhausted()
            && !self.ctx.depth_exceeded.get()
            && crate::query_boundaries::common::lazy_resolve_failure_count()
                == lazy_failures_at_entry
    }

    fn contextual_signature_normalization_cacheable(
        &self,
        expected: TypeId,
        normalized: TypeId,
    ) -> bool {
        let contains_context_placeholder = |ty| {
            crate::query_boundaries::common::contains_infer_types(self.ctx.types, ty)
                || crate::query_boundaries::common::contains_this_type(self.ctx.types, ty)
                || crate::query_boundaries::state::type_environment::contains_type_query_db(
                    self.ctx.types,
                    ty,
                )
                || crate::query_boundaries::common::contains_file_relative_content(
                    self.ctx.types,
                    ty,
                )
        };
        !contains_context_placeholder(expected) && !contains_context_placeholder(normalized)
    }

    fn normalize_contextual_signature_with_env_uncached(&mut self, expected: TypeId) -> TypeId {
        fn should_preserve_contextual_param_type(
            db: &dyn tsz_solver::construction::TypeDatabase,
            ty: TypeId,
        ) -> bool {
            // Delegate to solver query: checks if any union member is constructor-like
            crate::query_boundaries::common::is_constructor_like_type(db, ty)
        }

        fn is_tuple_like_rest_param(
            db: &dyn tsz_solver::construction::TypeDatabase,
            ty: TypeId,
        ) -> bool {
            crate::query_boundaries::common::tuple_elements(db, ty).is_some()
                || crate::query_boundaries::common::union_members(db, ty).is_some_and(|members| {
                    !members.is_empty()
                        && members.iter().all(|member| {
                            crate::query_boundaries::common::tuple_elements(db, *member).is_some()
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
                return parameter_query::union_preserve_members_type(
                    self.ctx.types,
                    normalized_members,
                );
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
                return parameter_query::intersection_type(self.ctx.types, normalized_members);
            }
            return expected;
        }

        // A contextual type extracted across a module/arena boundary can arrive
        // as a bare, unresolved reference (`UnresolvedTypeName`/`Lazy`): an
        // imported alias body (`type Write<…> = (set: Setter, …) => …`)
        // references `Setter` whose name is in scope only in the *declaring*
        // file, so the lowering pass left it `UnresolvedTypeName("Setter")` with
        // no contextual signature of its own. Resolve such a reference through
        // the env once so the signature-shaped normalization below — and the
        // rest-tuple contextual parameter extraction it feeds — sees the real
        // callable shape. Without this the contextually-typed callback rest
        // parameter (`(...setArgs) =>`) falls back to `any`, which then
        // spuriously trips the TS2556 spread gate (#14746). Guarded on the
        // deferred-reference kinds and on the absence of a directly available
        // contextual signature, so non-deferred contextual types are untouched.
        if (crate::query_boundaries::spread::unresolved_type_name_atom(self.ctx.types, expected)
            .is_some()
            || crate::query_boundaries::common::is_lazy_type(self.ctx.types, expected))
            && crate::query_boundaries::checkers::call::get_contextual_signature(
                self.ctx.types,
                expected,
            )
            .is_none()
        {
            // The `TypeEnvironment` resolver only answers names it was seeded
            // with, so a bare cross-module `UnresolvedTypeName` stays opaque
            // there. Evaluate through the `CheckerContext` resolver, which walks
            // the merged binder graph and recovers the declaring-file symbol.
            let evaluated =
                crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                    self.ctx.types,
                    &self.ctx,
                    expected,
                );
            if evaluated != expected
                && crate::query_boundaries::checkers::call::get_contextual_signature(
                    self.ctx.types,
                    evaluated,
                )
                .is_some()
            {
                return self.normalize_contextual_signature_with_env(evaluated);
            }
        }

        // A MULTI-signature callable keeps its overload set. Collapsing it to
        // the combined mono shape (the get_contextual_signature +
        // function_type_from_shape path below) replaces per-overload
        // positional extraction — which unions parameter types across the
        // set — with a single signature whose merged rest tuple defeats
        // positional mapping (probe: the pre-collapse callable extracts every
        // position; the collapsed form loses parameter 1 and emitted false
        // TS7006 on the zustand devtools `as NamedSet<S>` arrow). Instead,
        // env-evaluate each signature's parameter types in place — the same
        // service the mono path below provides — so deferred forms like the
        // rebuilt overloads' `[...TakeTwo<Sa>, action?: Action]` rest tuples
        // reduce (and spread-of-tuple normalization flattens them) while the
        // overload set survives for the per-signature extractors.
        if let Some(shape_id) =
            crate::query_boundaries::common::callable_shape_id(self.ctx.types, expected)
        {
            let shape = self.ctx.types.callable_shape(shape_id);
            if shape.call_signatures.len() > 1 {
                let mut rebuilt = shape.as_ref().clone();
                let mut changed = false;
                for sig in &mut rebuilt.call_signatures {
                    if !sig.type_params.is_empty() {
                        continue;
                    }
                    for param in &mut sig.params {
                        let evaluated = self.evaluate_type_with_env(param.type_id);
                        if evaluated != param.type_id
                            && evaluated != TypeId::ERROR
                            && evaluated != TypeId::UNKNOWN
                        {
                            param.type_id = evaluated;
                            changed = true;
                        }
                    }
                }
                if changed {
                    return self.ctx.types.callable(rebuilt);
                }
                return expected;
            }
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
            } else if crate::query_boundaries::common::type_param_info(self.ctx.types, resolved)
                .is_some()
            {
                // Preserve type parameters that appear inside contextual callback
                // signatures. Collapsing them to constraints here loses outer
                // generic identity, e.g. ProxyHandler<T> callback targets become
                // Function when passed through a generic identity wrapper.
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
            parameter_query::function_type_from_shape(self.ctx.types, shape)
        } else {
            expected
        }
    }
}
