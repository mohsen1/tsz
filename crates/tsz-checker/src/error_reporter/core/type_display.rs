//! Core type display and formatting utilities for error reporting.

use crate::query_boundaries::diagnostics as query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn sanitize_type_annotation_text_for_diagnostic(
        &self,
        text: String,
        allow_object_shapes: bool,
    ) -> Option<String> {
        fn parenthesize_intersection_in_union_text(text: &str) -> String {
            let mut parts = Vec::new();
            let mut current = String::new();
            let mut depth = 0u32;

            for (i, ch) in text.char_indices() {
                match ch {
                    '(' | '<' | '[' => {
                        depth += 1;
                        current.push(ch);
                    }
                    ')' | '>' | ']' => {
                        depth = depth.saturating_sub(1);
                        current.push(ch);
                    }
                    '|' if depth == 0
                        && text.get(i.saturating_sub(1)..i) == Some(" ")
                        && text.get(i + 1..i + 2) == Some(" ") =>
                    {
                        parts.push(current.trim().to_string());
                        current = String::new();
                    }
                    _ => current.push(ch),
                }
            }
            parts.push(current.trim().to_string());

            parts
                .into_iter()
                .map(|part| {
                    if part.contains(" & ") && !part.starts_with('(') {
                        format!("({part})")
                    } else {
                        part
                    }
                })
                .collect::<Vec<_>>()
                .join(" | ")
        }

        let mut text = text.trim().trim_start_matches(':').trim().to_string();
        if let Some(nl) = text.find('\n') {
            text = text[..nl].trim_end().to_string();
        }
        if text.ends_with('=') {
            text.pop();
            text = text.trim_end().to_string();
        }
        while matches!(text.chars().last(), Some(',') | Some(';')) {
            text.pop();
            text = text.trim_end().to_string();
        }
        while matches!(text.chars().last(), Some(')')) {
            let open_count = text.chars().filter(|&ch| ch == '(').count();
            let close_count = text.chars().filter(|&ch| ch == ')').count();
            if close_count <= open_count {
                break;
            }
            text.pop();
            text = text.trim_end().to_string();
        }
        if !allow_object_shapes && (text.starts_with('{') || text.starts_with('[')) {
            return None;
        }
        let open_count = text.chars().filter(|&ch| ch == '(').count();
        let close_count = text.chars().filter(|&ch| ch == ')').count();
        if open_count != close_count || text.is_empty() {
            return None;
        }
        if text.contains(" & ") && text.contains(" | ") {
            text = parenthesize_intersection_in_union_text(&text);
        }
        Some(text)
    }

    fn param_matches_property_key_literal(&self, prop_name: Atom, ty: TypeId) -> bool {
        let prop_name = self.ctx.types.resolve_atom_ref(prop_name);
        if self.ctx.types.literal_string(prop_name.as_ref()) == ty {
            return true;
        }
        prop_name
            .parse::<f64>()
            .ok()
            .is_some_and(|num| self.ctx.types.literal_number(num) == ty)
    }

    fn normalize_excess_display_type_for_property(
        &self,
        prop_name: Option<Atom>,
        ty: TypeId,
    ) -> TypeId {
        let ty = self.normalize_excess_display_type(ty);
        let Some(prop_name) = prop_name else {
            return ty;
        };

        if let Some(shape) = query::function_shape(self.ctx.types, ty) {
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| {
                    let normalized = self.normalize_excess_display_type(param.type_id);
                    let type_id = if self.param_matches_property_key_literal(prop_name, normalized)
                    {
                        normalized
                    } else {
                        crate::query_boundaries::common::widen_literal_type(
                            self.ctx.types,
                            normalized,
                        )
                    };
                    tsz_solver::ParamInfo { type_id, ..*param }
                })
                .collect();

            if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b) {
                ty
            } else {
                self.ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type: shape.this_type,
                        return_type: shape.return_type,
                        type_predicate: shape.type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
            }
        } else {
            ty
        }
    }

    pub(in crate::error_reporter) fn widen_function_like_display_type(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        let constructor_display_def = self
            .ctx
            .definition_store
            .find_def_for_type(type_id)
            .and_then(|def_id| {
                self.ctx
                    .definition_store
                    .get(def_id)
                    .filter(|def| def.is_class_constructor())
                    .map(|_| def_id)
            });

        let type_id = self.evaluate_type_with_env(type_id);
        if tsz_solver::is_generic_application(self.ctx.types, type_id) {
            let widened = crate::query_boundaries::common::widen_type(self.ctx.types, type_id);
            if let Some(def_id) = constructor_display_def {
                self.ctx
                    .definition_store
                    .register_type_to_def(widened, def_id);
            }
            return widened;
        }
        let type_id = self.resolve_type_for_property_access(type_id);
        let type_id = self.resolve_lazy_type(type_id);
        let type_id = self.evaluate_application_type(type_id);
        let widened = crate::query_boundaries::common::widen_type(self.ctx.types, type_id);
        if let Some(def_id) = constructor_display_def {
            self.ctx
                .definition_store
                .register_type_to_def(widened, def_id);
        }
        widened
    }

    fn terminal_assignment_source_expression(&self, expr_idx: NodeIndex) -> NodeIndex {
        let mut current = expr_idx;
        let mut guard = 0;

        loop {
            guard += 1;
            if guard > 256 {
                return current;
            }

            let expr = self.ctx.arena.skip_parenthesized(current);
            let Some(node) = self.ctx.arena.get(expr) else {
                return current;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return expr;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(node) else {
                return expr;
            };
            if !self.is_assignment_operator(bin.operator_token) {
                return expr;
            }
            current = bin.right;
        }
    }

    fn normalize_excess_display_type(&self, ty: TypeId) -> TypeId {
        let ty = tsz_solver::evaluate_type(self.ctx.types, ty);
        if let Some(app) = query::type_application(self.ctx.types, ty) {
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| self.normalize_excess_display_type(arg))
                .collect();
            if args == app.args {
                ty
            } else {
                self.ctx.types.factory().application(app.base, args)
            }
        } else if let Some(shape) = query::function_shape(self.ctx.types, ty) {
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| tsz_solver::ParamInfo {
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
                self.ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape {
                        type_params: shape.type_params.clone(),
                        params,
                        this_type: shape.this_type,
                        return_type,
                        type_predicate: shape.type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
            }
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
            self.ctx.types.factory().union_preserve_members(
                members
                    .iter()
                    .map(|&member| self.normalize_excess_display_type(member))
                    .collect(),
            )
        } else if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            self.ctx.types.factory().intersection(
                members
                    .iter()
                    .map(|&member| self.normalize_excess_display_type(member))
                    .collect(),
            )
        } else {
            ty
        }
    }

    pub(in crate::error_reporter) fn normalize_assignability_display_type(
        &mut self,
        ty: TypeId,
    ) -> TypeId {
        // Depth guard: recursive types (e.g., `interface Foo { j: Foo }`) cause
        // unbounded recursion when normalizing property types for display. Deep
        // recursion can trip the stack overflow breaker in get_type_of_symbol,
        // permanently poisoning symbol resolution and causing subsequent type
        // evaluations to return ERROR — which silently suppresses real
        // assignability diagnostics (e.g., TS2322).
        thread_local! {
            static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        }
        let depth = DEPTH.get();
        if depth >= 10 {
            return ty;
        }

        DEPTH.set(depth + 1);
        let mut visiting = FxHashSet::default();
        let result = self.normalize_assignability_display_type_inner(ty, &mut visiting, 0);
        DEPTH.set(depth);
        result
    }

    fn should_truncate_assignability_display_type(&self, ty: TypeId, depth: usize) -> bool {
        if depth < 3 {
            return false;
        }

        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
            || tsz_solver::function_shape_id(self.ctx.types, ty).is_some()
            || tsz_solver::callable_shape_id(self.ctx.types, ty).is_some()
        {
            return true;
        }

        if depth < 5 {
            return false;
        }

        if query::type_application(self.ctx.types, ty).is_some() {
            return true;
        }

        if query::union_members(self.ctx.types, ty).is_some_and(|members| members.len() > 4)
            || query::intersection_members(self.ctx.types, ty)
                .is_some_and(|members| members.len() > 3)
        {
            return true;
        }

        tsz_solver::type_queries::get_object_shape(self.ctx.types, ty).is_some_and(|shape| {
            shape.properties.len() > 6
                || shape.string_index.is_some()
                || shape.number_index.is_some()
        })
    }

    fn normalize_assignability_display_type_inner(
        &mut self,
        ty: TypeId,
        visiting: &mut FxHashSet<TypeId>,
        depth: usize,
    ) -> TypeId {
        const MAX_ASSIGNABILITY_DISPLAY_DEPTH: usize = 12;
        let ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);

        if depth >= MAX_ASSIGNABILITY_DISPLAY_DEPTH || !visiting.insert(ty) {
            return ty;
        }

        if self.should_truncate_assignability_display_type(ty, depth) {
            visiting.remove(&ty);
            return ty;
        }

        let result = if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let has_undefined = members.contains(&TypeId::UNDEFINED);
            let has_null = members.contains(&TypeId::NULL);
            let generic_scaffolding_only = members.iter().all(|&member| {
                member == TypeId::UNDEFINED
                    || member == TypeId::NULL
                    || crate::query_boundaries::state::checking::is_type_parameter_like(
                        self.ctx.types,
                        member,
                    )
                    || crate::query_boundaries::common::contains_type_parameters(
                        self.ctx.types,
                        member,
                    )
            });
            if generic_scaffolding_only {
                if has_undefined {
                    TypeId::UNDEFINED
                } else if has_null {
                    TypeId::NULL
                } else {
                    ty
                }
            } else if query::union_members(self.ctx.types, ty).is_none() {
                // Non-generic intersection that isn't also a union: preserve as-is.
                // Evaluation would lose display_properties (literal values) on fresh
                // object members. tsc shows `{ fooProp: "frizzlebizzle"; } & Bar`
                // not `{ fooProp: string; } & Bar`.
                ty
            } else if let Some(members) = query::union_members(self.ctx.types, ty) {
                let normalized: Vec<_> = members
                    .iter()
                    .map(|&member| {
                        self.normalize_assignability_display_type_inner(member, visiting, depth + 1)
                    })
                    .collect();
                if normalized == members {
                    ty
                } else {
                    self.ctx.types.factory().union_preserve_members(normalized)
                }
            } else {
                let evaluated =
                    if tsz_solver::type_queries::is_index_access_type(self.ctx.types, ty)
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            ty,
                        )
                    {
                        ty
                    } else {
                        self.evaluate_type_for_assignability(ty)
                    };

                if self.should_truncate_assignability_display_type(evaluated, depth) {
                    visiting.remove(&ty);
                    return evaluated;
                }

                if let Some(app) = query::type_application(self.ctx.types, evaluated) {
                    let args: Vec<_> = app
                        .args
                        .iter()
                        .map(|&arg| {
                            self.normalize_assignability_display_type_inner(
                                arg,
                                visiting,
                                depth + 1,
                            )
                        })
                        .collect();
                    if args == app.args {
                        evaluated
                    } else {
                        self.ctx.types.factory().application(app.base, args)
                    }
                } else if let Some(shape) = query::function_shape(self.ctx.types, evaluated) {
                    let params: Vec<_> = shape
                        .params
                        .iter()
                        .map(|param| tsz_solver::ParamInfo {
                            type_id: self.normalize_assignability_display_type_inner(
                                param.type_id,
                                visiting,
                                depth + 1,
                            ),
                            ..*param
                        })
                        .collect();
                    let return_type = self.normalize_assignability_display_type_inner(
                        shape.return_type,
                        visiting,
                        depth + 1,
                    );
                    let return_type =
                        crate::query_boundaries::common::widen_type(self.ctx.types, return_type);
                    if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                        && return_type == shape.return_type
                    {
                        evaluated
                    } else {
                        self.ctx
                            .types
                            .factory()
                            .function(tsz_solver::FunctionShape {
                                type_params: shape.type_params.clone(),
                                params,
                                this_type: shape.this_type,
                                return_type,
                                type_predicate: shape.type_predicate,
                                is_constructor: shape.is_constructor,
                                is_method: shape.is_method,
                            })
                    }
                } else if let Some(shape) =
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, evaluated)
                {
                    let mut shape = shape.as_ref().clone();
                    let mut changed = false;
                    for prop in &mut shape.properties {
                        let normalized_read = self.normalize_assignability_display_type_inner(
                            prop.type_id,
                            visiting,
                            depth + 1,
                        );
                        let normalized_write = self.normalize_assignability_display_type_inner(
                            prop.write_type,
                            visiting,
                            depth + 1,
                        );
                        changed |=
                            normalized_read != prop.type_id || normalized_write != prop.write_type;
                        prop.type_id = normalized_read;
                        prop.write_type = normalized_write;
                    }
                    if let Some(index) = shape.string_index.as_mut() {
                        let normalized = self.normalize_assignability_display_type_inner(
                            index.value_type,
                            visiting,
                            depth + 1,
                        );
                        changed |= normalized != index.value_type;
                        index.value_type = normalized;
                    }
                    if let Some(index) = shape.number_index.as_mut() {
                        let normalized = self.normalize_assignability_display_type_inner(
                            index.value_type,
                            visiting,
                            depth + 1,
                        );
                        changed |= normalized != index.value_type;
                        index.value_type = normalized;
                    }
                    if changed {
                        let new_ty = self.ctx.types.factory().object_with_index(shape);
                        if let Some(display_props) =
                            self.ctx.types.get_display_properties(evaluated)
                        {
                            self.ctx
                                .types
                                .store_display_properties(new_ty, display_props.as_ref().clone());
                        }
                        new_ty
                    } else {
                        evaluated
                    }
                } else if let Some(members) = query::union_members(self.ctx.types, evaluated) {
                    self.ctx.types.factory().union_preserve_members(
                        members
                            .iter()
                            .map(|&member| {
                                self.normalize_assignability_display_type_inner(
                                    member,
                                    visiting,
                                    depth + 1,
                                )
                            })
                            .collect(),
                    )
                } else if let Some(members) = query::intersection_members(self.ctx.types, evaluated)
                {
                    self.ctx.types.factory().intersection(
                        members
                            .iter()
                            .map(|&member| {
                                self.normalize_assignability_display_type_inner(
                                    member,
                                    visiting,
                                    depth + 1,
                                )
                            })
                            .collect(),
                    )
                } else {
                    evaluated
                }
            }
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
            let normalized: Vec<_> = members
                .iter()
                .map(|&member| {
                    self.normalize_assignability_display_type_inner(member, visiting, depth + 1)
                })
                .collect();
            if normalized == members {
                ty
            } else {
                self.ctx.types.factory().union_preserve_members(normalized)
            }
        } else if let Some(app) = query::type_application(self.ctx.types, ty) {
            if query::preserves_named_application_base(self.ctx.types, app.base) {
                let args: Vec<_> = app
                    .args
                    .iter()
                    .map(|&arg| {
                        self.normalize_assignability_display_type_inner(arg, visiting, depth + 1)
                    })
                    .collect();
                if args == app.args {
                    ty
                } else {
                    self.ctx.types.factory().application(app.base, args)
                }
            } else {
                let evaluated =
                    if tsz_solver::type_queries::is_index_access_type(self.ctx.types, ty)
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            ty,
                        )
                    {
                        ty
                    } else {
                        self.evaluate_type_for_assignability(ty)
                    };

                if self.should_truncate_assignability_display_type(evaluated, depth) {
                    visiting.remove(&ty);
                    return evaluated;
                }
                self.normalize_assignability_display_type_inner(evaluated, visiting, depth + 1)
            }
        } else {
            let evaluated = if tsz_solver::type_queries::is_index_access_type(self.ctx.types, ty)
                && crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
            {
                ty
            } else {
                self.evaluate_type_for_assignability(ty)
            };

            if self.should_truncate_assignability_display_type(evaluated, depth) {
                visiting.remove(&ty);
                return evaluated;
            }

            if let Some(app) = query::type_application(self.ctx.types, evaluated) {
                let args: Vec<_> = app
                    .args
                    .iter()
                    .map(|&arg| {
                        self.normalize_assignability_display_type_inner(arg, visiting, depth + 1)
                    })
                    .collect();
                if args == app.args {
                    evaluated
                } else {
                    self.ctx.types.factory().application(app.base, args)
                }
            } else if let Some(shape) = query::function_shape(self.ctx.types, evaluated) {
                let params: Vec<_> = shape
                    .params
                    .iter()
                    .map(|param| tsz_solver::ParamInfo {
                        type_id: self.normalize_assignability_display_type_inner(
                            param.type_id,
                            visiting,
                            depth + 1,
                        ),
                        ..*param
                    })
                    .collect();
                let return_type = self.normalize_assignability_display_type_inner(
                    shape.return_type,
                    visiting,
                    depth + 1,
                );
                let return_type =
                    crate::query_boundaries::common::widen_type(self.ctx.types, return_type);
                if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                    && return_type == shape.return_type
                {
                    evaluated
                } else {
                    self.ctx
                        .types
                        .factory()
                        .function(tsz_solver::FunctionShape {
                            type_params: shape.type_params.clone(),
                            params,
                            this_type: shape.this_type,
                            return_type,
                            type_predicate: shape.type_predicate,
                            is_constructor: shape.is_constructor,
                            is_method: shape.is_method,
                        })
                }
            } else if let Some(shape) =
                tsz_solver::type_queries::get_object_shape(self.ctx.types, evaluated)
            {
                let mut shape = shape.as_ref().clone();
                let mut changed = false;
                for prop in &mut shape.properties {
                    let normalized_read = self.normalize_assignability_display_type_inner(
                        prop.type_id,
                        visiting,
                        depth + 1,
                    );
                    let normalized_write = self.normalize_assignability_display_type_inner(
                        prop.write_type,
                        visiting,
                        depth + 1,
                    );
                    changed |=
                        normalized_read != prop.type_id || normalized_write != prop.write_type;
                    prop.type_id = normalized_read;
                    prop.write_type = normalized_write;
                }
                if let Some(index) = shape.string_index.as_mut() {
                    let normalized = self.normalize_assignability_display_type_inner(
                        index.value_type,
                        visiting,
                        depth + 1,
                    );
                    changed |= normalized != index.value_type;
                    index.value_type = normalized;
                }
                if let Some(index) = shape.number_index.as_mut() {
                    let normalized = self.normalize_assignability_display_type_inner(
                        index.value_type,
                        visiting,
                        depth + 1,
                    );
                    changed |= normalized != index.value_type;
                    index.value_type = normalized;
                }
                if changed {
                    let new_ty = self.ctx.types.factory().object_with_index(shape);
                    if let Some(display_props) = self.ctx.types.get_display_properties(evaluated) {
                        self.ctx
                            .types
                            .store_display_properties(new_ty, display_props.as_ref().clone());
                    }
                    new_ty
                } else {
                    evaluated
                }
            } else if let Some(members) = query::intersection_members(self.ctx.types, evaluated) {
                self.ctx.types.factory().intersection(
                    members
                        .iter()
                        .map(|&member| {
                            self.normalize_assignability_display_type_inner(
                                member,
                                visiting,
                                depth + 1,
                            )
                        })
                        .collect(),
                )
            } else {
                evaluated
            }
        };

        visiting.remove(&ty);
        result
    }

    fn split_optional_object_for_excess_display(&self, ty: TypeId) -> TypeId {
        let ty = tsz_solver::evaluate_type(self.ctx.types, ty);
        if let Some(members) = query::union_members(self.ctx.types, ty) {
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

    /// For TS2353 diagnostics on union targets, strip non-object members (primitives,
    /// undefined, null, void, never, etc.) so the displayed type matches tsc.
    /// For example, `IProps | number` becomes `IProps`, and
    /// `{ testBool?: boolean | undefined; } | undefined` becomes `{ testBool?: boolean | undefined; }`.
    pub(in crate::error_reporter) fn strip_non_object_union_members_for_excess_display(
        &self,
        ty: TypeId,
    ) -> TypeId {
        let ty = tsz_solver::evaluate_type(self.ctx.types, ty);
        if let Some(members) = query::union_members(self.ctx.types, ty) {
            let object_like: Vec<_> = members
                .iter()
                .copied()
                .filter(|member| {
                    let evaluated = tsz_solver::evaluate_type(self.ctx.types, *member);
                    !tsz_solver::is_primitive_type(self.ctx.types, evaluated)
                        && !crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            evaluated,
                        )
                })
                .collect();
            // Only strip if we actually removed something and have at least one member left
            if !object_like.is_empty() && object_like.len() < members.len() {
                if object_like.len() == 1 {
                    return object_like[0];
                }
                return tsz_solver::utils::union_or_single(self.ctx.types, object_like);
            }
        }
        ty
    }

    fn split_wildcard_object_for_excess_display(&mut self, ty: TypeId) -> Option<String> {
        let ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        let ty = self.split_optional_object_for_excess_display(ty);
        let shape = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty)?;
        if shape.string_index.is_some() || shape.number_index.is_some() {
            return None;
        }

        let wildcard_name = self.ctx.types.intern_string("*");
        let mut wildcard_props = Vec::new();
        let mut named_props = Vec::new();

        for prop in &shape.properties {
            let mut cloned = prop.clone();
            cloned.type_id =
                self.normalize_excess_display_type_for_property(Some(cloned.name), cloned.type_id);
            cloned.write_type = self
                .normalize_excess_display_type_for_property(Some(cloned.name), cloned.write_type);
            if cloned.name == wildcard_name {
                wildcard_props.push(cloned);
            } else {
                named_props.push(cloned);
            }
        }

        if wildcard_props.is_empty() || named_props.is_empty() {
            return None;
        }

        let named_obj = self.ctx.types.factory().object(named_props);
        let wildcard_obj = self.ctx.types.factory().object(wildcard_props);
        Some(format!(
            "{} & {}",
            self.format_type_diagnostic(named_obj),
            self.format_type_diagnostic(wildcard_obj)
        ))
    }

    fn materialize_finite_mapped_type_for_display(&mut self, ty: TypeId) -> Option<TypeId> {
        if let Some((mapped_id, mapped)) = query::mapped_type(self.ctx.types, ty) {
            let names =
                crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                    self.ctx.types,
                    mapped_id,
                )?;
            let mut names: Vec<_> = names.into_iter().collect();
            names.sort_by(|a, b| {
                self.ctx
                    .types
                    .resolve_atom_ref(*a)
                    .cmp(&self.ctx.types.resolve_atom_ref(*b))
            });

            let mut properties = Vec::with_capacity(names.len());
            for name in names {
                let property_name = self.ctx.types.resolve_atom_ref(name).to_string();
                let type_id =
                    crate::query_boundaries::state::checking::get_finite_mapped_property_type(
                        self.ctx.types,
                        mapped_id,
                        &property_name,
                    )?;
                let type_id = self.normalize_excess_display_type_for_property(Some(name), type_id);
                let mut property = tsz_solver::PropertyInfo::new(name, type_id);
                property.optional =
                    mapped.optional_modifier == Some(tsz_solver::MappedModifier::Add);
                property.readonly =
                    mapped.readonly_modifier == Some(tsz_solver::MappedModifier::Add);
                properties.push(property);
            }

            Some(self.ctx.types.factory().object(properties))
        } else if let Some(members) = query::intersection_members(self.ctx.types, ty) {
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
            changed.then(|| self.ctx.types.factory().intersection(remapped))
        } else if let Some(members) = query::union_members(self.ctx.types, ty) {
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
            changed.then(|| self.ctx.types.factory().union(remapped))
        } else {
            None
        }
    }

    pub(crate) fn format_excess_property_target_type(&mut self, ty: TypeId) -> String {
        // If the type is a named alias (e.g., `type ExoticAnimal = CatDog | ManBearPig`),
        // tsc shows the alias name in excess property messages. Check for Lazy(DefId)
        // references before evaluation strips the name. The formatter handles Lazy types
        // by resolving to the definition name.
        if tsz_solver::is_lazy_type(self.ctx.types, ty) {
            return self.format_type_diagnostic(ty);
        }

        // For already-evaluated types, check if a type alias name can be recovered
        // via body_to_alias or type_to_def. This handles cases where the Lazy
        // reference was resolved before reaching this function.
        if let Some(alias_name) = self.lookup_type_alias_name_for_display(ty) {
            return alias_name;
        }

        if let Some(display) = self.split_wildcard_object_for_excess_display(ty) {
            return display;
        }

        // For union targets, tsc strips non-object members (primitives like number,
        // undefined, null, etc.) from the displayed type. Excess property checking
        // only applies to object-like members, so the diagnostic should reference
        // only those members rather than the full union.
        let ty = self.strip_non_object_union_members_for_excess_display(ty);

        if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let preserve_intersection_parts = members
                .iter()
                .any(|member| tsz_solver::evaluate_type(self.ctx.types, *member) == TypeId::OBJECT);
            let mut changed = false;
            let parts: Vec<String> = members
                .iter()
                .map(|&member| {
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        self.format_type_diagnostic(materialized)
                    } else {
                        self.format_type_diagnostic(member)
                    }
                })
                .collect();
            if changed || preserve_intersection_parts {
                return parts.join(" & ");
            }
        }

        let display_ty = self
            .materialize_finite_mapped_type_for_display(ty)
            .unwrap_or(ty);
        self.format_type_diagnostic(display_ty)
    }

    pub(in crate::error_reporter) fn format_extract_keyof_string_type(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let members = crate::query_boundaries::common::intersection_members(self.ctx.types, ty)?;
        if members.len() != 2 || !members.contains(&TypeId::STRING) {
            return None;
        }

        let other = members
            .iter()
            .copied()
            .find(|&member| member != TypeId::STRING)?;
        if !tsz_solver::type_queries::is_keyof_type(self.ctx.types, other) {
            return None;
        }

        Some(format!(
            "Extract<{}, string>",
            self.format_type_for_assignability_message(other)
        ))
    }

    pub(in crate::error_reporter) fn format_annotation_like_type(&mut self, text: &str) -> String {
        let mut formatted = text.trim().to_string();
        if formatted.contains(";}") {
            formatted = formatted.replace(";}", "; }");
        }
        if formatted.contains(':') && formatted.ends_with(" }") && !formatted.ends_with("; }") {
            formatted = format!("{}; }}", &formatted[..formatted.len() - 2]);
        }
        // Normalize `{prop: type}` to `{ prop: type; }` — tsc always adds
        // spaces inside braces and trailing semicolons for inline object types.
        // Handle both standalone `{...}` and intersection parts `& {...}`.
        formatted = Self::normalize_inline_object_braces(&formatted);
        // tsc always displays Array<T> as T[] in error messages.
        // Convert generic Array form to shorthand when reading from source annotations.
        formatted = Self::normalize_array_generic_to_shorthand(&formatted);
        formatted
    }

    /// Convert `Array<T>` to `T[]` and `ReadonlyArray<T>` to `readonly T[]`
    /// in annotation text to match tsc's diagnostic display.
    fn normalize_array_generic_to_shorthand(text: &str) -> String {
        if !text.contains("Array<") {
            return text.to_string();
        }
        let mut result = text.to_string();
        // Process ReadonlyArray<T> first (before Array<T> to avoid partial matches)
        while let Some(start) = result.find("ReadonlyArray<") {
            if let Some(inner) = Self::extract_balanced_angle_bracket_content(&result, start + 14) {
                let needs_parens = inner.contains("=>") || inner.contains(" | ");
                let replacement = if needs_parens {
                    format!("readonly ({inner})[]")
                } else {
                    format!("readonly {inner}[]")
                };
                let end = start + 14 + inner.len() + 1; // "ReadonlyArray<" + inner + ">"
                result = format!("{}{}{}", &result[..start], replacement, &result[end..]);
            } else {
                break;
            }
        }
        // Then Array<T>
        while let Some(start) = result.find("Array<") {
            // Make sure it's not part of a longer name (e.g., "ReadonlyArray" already handled)
            if start > 0 && result.as_bytes()[start - 1].is_ascii_alphanumeric() {
                // Part of a longer identifier, skip
                break;
            }
            if let Some(inner) = Self::extract_balanced_angle_bracket_content(&result, start + 6) {
                let needs_parens = inner.contains("=>") || inner.contains(" | ");
                let replacement = if needs_parens {
                    format!("({inner})[]")
                } else {
                    format!("{inner}[]")
                };
                let end = start + 6 + inner.len() + 1; // "Array<" + inner + ">"
                result = format!("{}{}{}", &result[..start], replacement, &result[end..]);
            } else {
                break;
            }
        }
        result
    }

    /// Extract content between balanced angle brackets starting at `pos`.
    /// `pos` should point to the character right after the opening `<`.
    /// Returns the inner content (without brackets) if balanced.
    fn extract_balanced_angle_bracket_content(text: &str, pos: usize) -> Option<String> {
        let bytes = text.as_bytes();
        let mut depth = 1;
        let mut i = pos;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'<' => depth += 1,
                b'>' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(text[pos..i].to_string());
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Normalize inline object type braces in annotation text to match TSC's
    /// formatting: `{prop: type}` → `{ prop: type; }`.
    fn normalize_inline_object_braces(text: &str) -> String {
        let mut result = String::with_capacity(text.len() + 8);
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        let mut i = 0;
        while i < len {
            if chars[i] == '{' {
                // Find the matching closing brace
                let mut depth = 1;
                let mut j = i + 1;
                while j < len && depth > 0 {
                    if chars[j] == '{' {
                        depth += 1;
                    } else if chars[j] == '}' {
                        depth -= 1;
                    }
                    j += 1;
                }
                // j now points past the closing '}'
                let inner_start = i + 1;
                let inner_end = j - 1;
                let inner: String = chars[inner_start..inner_end].iter().collect();
                let trimmed = inner.trim();

                if trimmed.is_empty() {
                    result.push_str("{}");
                } else {
                    // Ensure `{ ... }` spacing
                    let needs_space_start =
                        !trimmed.is_empty() && (i + 1 >= len || chars[i + 1] != ' ');
                    let needs_semicolon = !trimmed.ends_with(';')
                        && !trimmed.ends_with("};")
                        && trimmed.contains(':');
                    result.push_str("{ ");
                    result.push_str(trimmed);
                    if needs_semicolon {
                        result.push(';');
                    }
                    let _ = needs_space_start;
                    result.push_str(" }");
                }
                i = j;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }

    pub(crate) fn excess_property_target_annotation_text_for_site(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        let mut current = idx;
        loop {
            let info = self.ctx.arena.node_info(current)?;
            let parent_idx = info.parent;
            let parent = self.ctx.arena.get(parent_idx)?;
            if parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                let grandparent_idx = self.ctx.arena.node_info(parent_idx)?.parent;
                let grandparent = self.ctx.arena.get(grandparent_idx)?;
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(grandparent)
                    && var_decl.initializer == parent_idx
                    && var_decl.type_annotation.is_some()
                {
                    return self.node_text(var_decl.type_annotation).and_then(|text| {
                        self.sanitize_type_annotation_text_for_diagnostic(text, true)
                    });
                }
                return None;
            }
            current = parent_idx;
        }
    }

    pub(in crate::error_reporter) fn should_use_evaluated_assignability_display(
        &self,
        ty: TypeId,
        evaluated: TypeId,
    ) -> bool {
        if ty == evaluated || evaluated == TypeId::ERROR {
            return false;
        }

        if ty == TypeId::BOOLEAN_TRUE || ty == TypeId::BOOLEAN_FALSE {
            return false;
        }

        if tsz_solver::literal_value(self.ctx.types, ty).is_some() {
            return false;
        }

        // For TypeQuery (typeof X), don't use evaluated display - preserve the
        // typeof syntax instead of expanding to the full function type.
        // This prevents double function arrows like `() => () => typeof fn`.
        if tsz_solver::type_queries::is_type_query_type(self.ctx.types, ty) {
            return false;
        }

        // For function types with a return type that is a TypeQuery, don't use
        // the evaluated display. The evaluation would resolve the TypeQuery to
        // the full function type, causing double arrows like `() => () => typeof fn`.
        if let Some(fn_shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, ty) {
            if tsz_solver::type_queries::is_type_query_type(self.ctx.types, fn_shape.return_type) {
                return false;
            }
        }

        // Also check callable types (single call signature)
        if let Some(callable) = tsz_solver::type_queries::get_callable_shape(self.ctx.types, ty) {
            if callable.call_signatures.len() == 1 {
                let sig = &callable.call_signatures[0];
                if tsz_solver::type_queries::is_type_query_type(self.ctx.types, sig.return_type) {
                    return false;
                }
            }
        }

        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, evaluated)
        {
            return false;
        }

        if evaluated == TypeId::NEVER
            || tsz_solver::literal_value(self.ctx.types, evaluated).is_some()
        {
            return true;
        }

        if (tsz_solver::lazy_def_id(self.ctx.types, ty).is_some()
            || tsz_solver::string_intrinsic_components(self.ctx.types, ty).is_some())
            && (tsz_solver::is_template_literal_type(self.ctx.types, evaluated)
                || tsz_solver::string_intrinsic_components(self.ctx.types, evaluated).is_some())
        {
            return true;
        }

        if !tsz_solver::type_queries::is_index_access_type(self.ctx.types, ty)
            && !tsz_solver::type_queries::is_keyof_type(self.ctx.types, ty)
            && !tsz_solver::type_queries::is_conditional_type(self.ctx.types, ty)
            && !tsz_solver::is_generic_application(self.ctx.types, ty)
        {
            return false;
        }

        // For IndexAccess types, display the evaluated form when it resolves to a
        // concrete type (union, object, primitive). This makes error messages show
        // the resolved type instead of the raw indexed access syntax.
        // e.g., `Pairs<FooBar>[keyof FooBar]` → `{ key: "foo"; value: string; } | { key: "bar"; value: number; }`
        if tsz_solver::type_queries::is_index_access_type(self.ctx.types, ty) {
            return true;
        }

        matches!(
            evaluated,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::VOID
        )
    }

    pub(in crate::error_reporter) fn format_structural_indexed_object_type(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let shape = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty)?;
        if shape.string_index.is_none() && shape.number_index.is_none() {
            return None;
        }

        let mut parts = Vec::new();
        if let Some(idx) = &shape.string_index {
            let key_name = idx
                .param_name
                .map(|a| self.ctx.types.resolve_atom_ref(a).to_string())
                .unwrap_or_else(|| "x".to_string());
            parts.push(format!(
                "[{key_name}: string]: {}",
                self.format_type(idx.value_type)
            ));
        }
        if let Some(idx) = &shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.ctx.types.resolve_atom_ref(a).to_string())
                .unwrap_or_else(|| "x".to_string());
            parts.push(format!(
                "[{key_name}: number]: {}",
                self.format_type(idx.value_type)
            ));
        }
        for prop in &shape.properties {
            let name = self.ctx.types.resolve_atom_ref(prop.name);
            let optional = if prop.optional { "?" } else { "" };
            let readonly = if prop.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{readonly}{name}{optional}: {}",
                self.format_type(prop.type_id)
            ));
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    /// Check if a type contains string literal types (directly or as union members).
    /// Used to determine whether an object literal property should display its
    /// literal value (for discriminated union contexts) or the widened type.
    pub(in crate::error_reporter) fn type_contains_string_literal(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::type_contains_string_literal(self.ctx.types, type_id)
    }

    pub(in crate::error_reporter) fn literal_expression_display(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        // Skip only parentheses, NOT type assertions. A type assertion like
        // `'bar' as any` changes the type to `any`, so the literal display
        // should not be used — the asserted type should be displayed instead.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        // If this is a type assertion expression (as/angle-bracket), don't
        // display the inner literal — let the caller use the asserted type.
        if node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::TYPE_ASSERTION
        {
            return None;
        }

        match node.kind {
            k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                let lit = self.ctx.arena.get_literal(node)?;
                let escaped = lit
                    .text
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                Some(format!("\"{escaped}\""))
            }
            k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                Some(lit.text.clone())
            }
            k if k == tsz_scanner::SyntaxKind::TrueKeyword as u16 => Some("true".to_string()),
            k if k == tsz_scanner::SyntaxKind::FalseKeyword as u16 => Some("false".to_string()),
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let operand = self.literal_expression_display(unary.operand)?;
                match unary.operator {
                    k if k == tsz_scanner::SyntaxKind::MinusToken as u16 => {
                        Some(format!("-{operand}"))
                    }
                    k if k == tsz_scanner::SyntaxKind::PlusToken as u16 => Some(operand),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                let cond = self.ctx.arena.get_conditional_expr(node)?;
                let left = self.literal_expression_display(cond.when_true)?;
                let right = self.literal_expression_display(cond.when_false)?;
                if left == right {
                    Some(left)
                } else {
                    Some(format!("{left} | {right}"))
                }
            }
            _ => None,
        }
    }

    pub(in crate::error_reporter) fn assignment_source_expression(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let bin = self.ctx.arena.get_binary_expr(node)?;
                    if self.is_assignment_operator(bin.operator_token) {
                        return Some(self.terminal_assignment_source_expression(bin.right));
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let stmt = self.ctx.arena.get_expression_statement(node)?;
                    let expr = self.ctx.arena.get(stmt.expression)?;
                    let bin = self.ctx.arena.get_binary_expr(expr)?;
                    return self
                        .is_assignment_operator(bin.operator_token)
                        .then_some(self.terminal_assignment_source_expression(bin.right));
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    return decl
                        .initializer
                        .is_some()
                        .then_some(self.terminal_assignment_source_expression(decl.initializer));
                }
                k if k == syntax_kind_ext::PARAMETER => {
                    let param = self.ctx.arena.get_parameter(node)?;
                    return param
                        .initializer
                        .is_some()
                        .then_some(self.terminal_assignment_source_expression(param.initializer));
                }
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_property_assignment(node)?;
                    return prop.initializer.is_some().then_some(prop.initializer);
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    let prop = self.ctx.arena.get_shorthand_property(node)?;
                    return prop.name.is_some().then_some(prop.name);
                }
                k if k == syntax_kind_ext::RETURN_STATEMENT => {
                    let ret = self.ctx.arena.get_return_statement(node)?;
                    return ret.expression.is_some().then_some(ret.expression);
                }
                _ => {}
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    pub(in crate::error_reporter) fn assignment_target_expression(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let bin = self.ctx.arena.get_binary_expr(node)?;
                    if self.is_assignment_operator(bin.operator_token) {
                        return Some(bin.left);
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                    let stmt = self.ctx.arena.get_expression_statement(node)?;
                    let expr = self.ctx.arena.get(stmt.expression)?;
                    let bin = self.ctx.arena.get_binary_expr(expr)?;
                    return self
                        .is_assignment_operator(bin.operator_token)
                        .then_some(bin.left);
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    return decl.name.is_some().then_some(decl.name);
                }
                k if k == syntax_kind_ext::PARAMETER => {
                    let param = self.ctx.arena.get_parameter(node)?;
                    return param.name.is_some().then_some(param.name);
                }
                _ => {}
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    pub(crate) fn assignment_source_is_return_expression(&self, anchor_idx: NodeIndex) -> bool {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::RETURN_STATEMENT {
                return true;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }

            // Expression body of an arrow function is an implicit return.
            // e.g. `(x: string): string => expr` — `expr` is the return value.
            if let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
                && let Some(func) = self.ctx.arena.get_function(parent_node)
                && func.body == current
                && node.kind != syntax_kind_ext::BLOCK
            {
                return true;
            }

            current = ext.parent;
        }

        false
    }
}
