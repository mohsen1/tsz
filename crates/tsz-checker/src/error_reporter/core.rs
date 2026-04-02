//! Core error emission helpers and type formatting utilities.

use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::diagnostics as query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn sanitize_type_annotation_text_for_diagnostic(
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

    pub(super) fn widen_function_like_display_type(&mut self, type_id: TypeId) -> TypeId {
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

    pub(super) fn normalize_assignability_display_type(&mut self, ty: TypeId) -> TypeId {
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
    pub(super) fn strip_non_object_union_members_for_excess_display(&self, ty: TypeId) -> TypeId {
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

    pub(super) fn format_extract_keyof_string_type(&mut self, ty: TypeId) -> Option<String> {
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

    pub(super) fn format_annotation_like_type(&mut self, text: &str) -> String {
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
        formatted
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

    pub(super) fn should_use_evaluated_assignability_display(
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

    fn format_structural_indexed_object_type(&mut self, ty: TypeId) -> Option<String> {
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
    fn type_contains_string_literal(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::type_contains_string_literal(self.ctx.types, type_id)
    }

    pub(super) fn literal_expression_display(&self, expr_idx: NodeIndex) -> Option<String> {
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

    pub(super) fn assignment_source_expression(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
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

    pub(super) fn assignment_target_expression(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
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
            current = ext.parent;
        }

        false
    }

    fn is_property_assignment_initializer(&self, anchor_idx: NodeIndex) -> bool {
        let current = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let Some(ext) = self.ctx.arena.get_extended(current) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && self
                .ctx
                .arena
                .get_property_assignment(parent)
                .is_some_and(|prop| prop.initializer == current)
    }

    pub(crate) fn object_literal_initializer_anchor_for_type(
        &mut self,
        object_idx: NodeIndex,
        source_type: TypeId,
    ) -> Option<(u32, u32)> {
        let mut current = self.ctx.arena.skip_parenthesized_and_assertions(object_idx);
        let mut guard = 0;

        loop {
            guard += 1;
            if guard > 32 {
                return None;
            }

            let node = self.ctx.arena.get(current)?;

            let direct_initializer =
                if let Some(prop) = self.ctx.arena.get_property_assignment(node) {
                    Some(prop.initializer)
                } else {
                    self.ctx.arena.get_shorthand_property(node).map(|prop| prop.name)
                };

            if let Some(initializer_idx) = direct_initializer {
                if let Some(anchor) = self.resolve_diagnostic_anchor(
                    initializer_idx,
                    crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind::Exact,
                ) {
                    return Some((anchor.start, anchor.length));
                }

                let (pos, end) = self.get_node_span(initializer_idx)?;
                return Some(self.normalized_anchor_span(
                    initializer_idx,
                    pos,
                    end.saturating_sub(pos),
                ));
            }

            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                let literal = self.ctx.arena.get_literal_expr(node)?;
                let source_display = self.format_type_for_assignability_message(
                    self.widen_type_for_display(source_type),
                );

                for child_idx in literal.elements.nodes.iter().copied() {
                    let Some(child) = self.ctx.arena.get(child_idx) else {
                        continue;
                    };

                    let candidate_idx =
                        if let Some(prop) = self.ctx.arena.get_property_assignment(child) {
                            prop.initializer
                        } else if let Some(prop) = self.ctx.arena.get_shorthand_property(child) {
                            prop.name
                        } else {
                            continue;
                        };

                    let candidate_type = self.get_type_of_node(candidate_idx);
                    if matches!(candidate_type, TypeId::ERROR | TypeId::UNKNOWN) {
                        continue;
                    }

                    let candidate_display = self.format_type_for_assignability_message(
                        self.widen_type_for_display(candidate_type),
                    );
                    if candidate_type != source_type && candidate_display != source_display {
                        continue;
                    }

                    if let Some(anchor) = self.resolve_diagnostic_anchor(
                        candidate_idx,
                        crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind::Exact,
                    ) {
                        return Some((anchor.start, anchor.length));
                    }

                    let (pos, end) = self.get_node_span(candidate_idx)?;
                    return Some(self.normalized_anchor_span(
                        candidate_idx,
                        pos,
                        end.saturating_sub(pos),
                    ));
                }

                return None;
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = self.ctx.arena.skip_parenthesized_and_assertions(ext.parent);
        }
    }

    fn direct_diagnostic_source_expression(&self, anchor_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // Only skip parenthesized expressions, NOT type assertions.
        // For `<foo>({})`, we want the type assertion node (type `foo`),
        // not the inner `{}` expression.
        let expr_idx = self.ctx.arena.skip_parenthesized(anchor_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && self.is_assignment_operator(binary.operator_token)
        {
            return None;
        }
        let is_expression_like = matches!(
            node.kind,
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::BINARY_EXPRESSION
                || k == syntax_kind_ext::CONDITIONAL_EXPRESSION
                || k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
        );
        if !is_expression_like {
            return None;
        }

        let parent_idx = self.ctx.arena.get_extended(expr_idx)?.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return Some(expr_idx);
        };

        if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.ctx.arena.get_binary_expr(parent_node)
            && self.is_assignment_operator(bin.operator_token)
            && bin.left == expr_idx
        {
            return None;
        }

        if parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && let Some(prop) = self.ctx.arena.get_property_assignment(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        if parent_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            && let Some(prop) = self.ctx.arena.get_shorthand_property(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        // Class property declaration names are not source expressions.
        // When TS2322 is anchored at the property name (e.g., `y` in `y: string = 42`),
        // the source expression is the initializer, not the name identifier.
        // Without this guard, get_type_of_node on the name triggers identifier
        // resolution → TS2304 "Cannot find name" false positive.
        if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
            && let Some(prop) = self.ctx.arena.get_property_decl(parent_node)
            && prop.name == expr_idx
        {
            return None;
        }

        // JSX attribute names are not source expressions.
        // When TS2322 is anchored at an attribute name (e.g., `x` in `<Comp x={10} />`),
        // the error reporter must not call get_type_of_node on the attribute name
        // identifier, which would trigger TS2304 "Cannot find name".
        if parent_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
            && let Some(attr) = self.ctx.arena.get_jsx_attribute(parent_node)
            && attr.name == expr_idx
        {
            return None;
        }

        Some(expr_idx)
    }

    fn declared_type_annotation_text_for_expression_with_options(
        &self,
        expr_idx: NodeIndex,
        allow_object_shapes: bool,
    ) -> Option<String> {
        let node_text_in_arena = |arena: &tsz_parser::NodeArena, node_idx: NodeIndex| {
            let node = arena.get(node_idx)?;
            let source = arena.source_files.first()?.text.as_ref();
            let start = node.pos as usize;
            let end = node.end as usize;
            if start >= end || end > source.len() {
                return None;
            }
            Some(source[start..end].to_string())
        };
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.get_cross_file_symbol(sym_id)?;
        let owner_binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .or_else(|| {
                self.ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .and_then(|arena| self.ctx.get_binder_for_arena(arena))
            })
            .unwrap_or(self.ctx.binder);
        let fallback_arena = if symbol.decl_file_idx != u32::MAX {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        } else {
            owner_binder
                .symbol_arenas
                .get(&sym_id)
                .map(std::convert::AsRef::as_ref)
                .unwrap_or(self.ctx.arena)
        };

        let mut declarations: Vec<(NodeIndex, &tsz_parser::NodeArena)> = Vec::new();
        let mut push_declaration = |decl_idx: NodeIndex| {
            if decl_idx.is_none() {
                return;
            }

            let mut pushed = false;
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    let arena = arena.as_ref();
                    if arena.get(decl_idx).is_none() {
                        continue;
                    }
                    let key = (decl_idx, arena as *const tsz_parser::NodeArena);
                    if declarations.iter().all(|(existing_idx, existing_arena)| {
                        (
                            *existing_idx,
                            *existing_arena as *const tsz_parser::NodeArena,
                        ) != key
                    }) {
                        declarations.push((decl_idx, arena));
                    }
                    pushed = true;
                }
            }

            if !pushed && fallback_arena.get(decl_idx).is_some() {
                let key = (decl_idx, fallback_arena as *const tsz_parser::NodeArena);
                if declarations.iter().all(|(existing_idx, existing_arena)| {
                    (
                        *existing_idx,
                        *existing_arena as *const tsz_parser::NodeArena,
                    ) != key
                }) {
                    declarations.push((decl_idx, fallback_arena));
                }
            }
        };

        push_declaration(symbol.value_declaration);
        for &decl_idx in &symbol.declarations {
            push_declaration(decl_idx);
        }

        for (decl_idx, decl_arena) in declarations {
            let decl = decl_arena.get(decl_idx)?;
            if let Some(param) = decl_arena.get_parameter(decl)
                && param.type_annotation.is_some()
            {
                let mut text =
                    node_text_in_arena(decl_arena, param.type_annotation).and_then(|text| {
                        self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                    })?;
                if param.question_token
                    && self.ctx.strict_null_checks()
                    && !text.contains("undefined")
                {
                    if text.contains("=>") {
                        text = format!("({text}) | undefined");
                    } else {
                        text.push_str(" | undefined");
                    }
                }
                return Some(text);
            }

            if let Some(var_decl) = decl_arena.get_variable_declaration(decl)
                && var_decl.type_annotation.is_some()
            {
                return node_text_in_arena(decl_arena, var_decl.type_annotation).and_then(|text| {
                    self.sanitize_type_annotation_text_for_diagnostic(text, allow_object_shapes)
                });
            }
        }

        None
    }

    pub(crate) fn declared_type_annotation_text_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        self.declared_type_annotation_text_for_expression_with_options(expr_idx, false)
    }

    fn declared_diagnostic_source_annotation_text(&self, expr_idx: NodeIndex) -> Option<String> {
        self.declared_type_annotation_text_for_expression_with_options(expr_idx, true)
    }

    fn should_prefer_declared_source_annotation_display(
        &mut self,
        expr_idx: NodeIndex,
        expr_type: TypeId,
        annotation_text: &str,
    ) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return false;
        }

        let annotation = annotation_text.trim();
        if annotation.contains('&') {
            return !annotation.starts_with("null |") && !annotation.starts_with("undefined |");
        }

        let display_type =
            self.widen_function_like_display_type(self.widen_type_for_display(expr_type));
        let formatted = self.format_type_for_assignability_message(display_type);
        // Keep declaration-site function signatures when the fallback display has
        // collapsed them to an alias name. tsc uses the declared callable surface
        // for lanes like templateLiteralTypes7 rather than a later alias-equivalent
        // name discovered from the shared type body.
        if annotation.contains("=>") && !formatted.contains("=>") {
            return true;
        }
        let resolved = self.resolve_type_for_property_access(display_type);
        let evaluated = self.judge_evaluate(resolved);
        let resolver =
            tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
        let has_index_signature = resolver.has_index_signature(
            evaluated,
            tsz_solver::objects::index_signatures::IndexKind::String,
        ) || resolver.has_index_signature(
            evaluated,
            tsz_solver::objects::index_signatures::IndexKind::Number,
        );
        if !formatted.starts_with('{') && !has_index_signature {
            return false;
        }

        // Don't use annotation text when it starts with `null` or `undefined` in
        // a union — the computed type formatter correctly reorders null/undefined
        // to the end (matching tsc's display), but annotation text preserves
        // source order which would put them first.
        if (annotation.starts_with("null |") || annotation.starts_with("undefined |"))
            && !annotation.contains('&')
        {
            return false;
        }
        annotation.contains('&') || !annotation.starts_with('{')
    }

    fn format_declared_annotation_for_diagnostic(&self, annotation_text: &str) -> String {
        let mut formatted = annotation_text.trim().to_string();
        if formatted.contains(':') {
            formatted = formatted.replace(" }", "; }");
        }
        formatted
    }

    pub(crate) fn format_type_diagnostic_structural(&self, ty: TypeId) -> String {
        let mut formatter = self.ctx.create_diagnostic_type_formatter();
        formatter.format(ty).into_owned()
    }

    fn synthesized_object_parent_display_name(&self, ty: TypeId) -> Option<String> {
        use tsz_binder::symbol_flags;
        use tsz_solver::type_queries::get_object_shape_id;

        let shape_id = get_object_shape_id(self.ctx.types, ty)?;
        let shape = self.ctx.types.object_shape(shape_id);
        let has_js_ctor_brand = shape.properties.iter().any(|prop| {
            self.ctx
                .types
                .resolve_atom_ref(prop.name)
                .starts_with("__js_ctor_brand_")
        });
        let mut parent_ids = shape.properties.iter().filter_map(|prop| prop.parent_id);
        let parent_sym = parent_ids.next()?;
        if parent_ids.any(|other| other != parent_sym) {
            return None;
        }

        let symbol = self.get_cross_file_symbol(parent_sym)?;
        if !has_js_ctor_brand
            && (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) == 0
        {
            return None;
        }

        Some(symbol.escaped_name.clone())
    }

    pub(crate) fn format_property_receiver_type_for_diagnostic(&mut self, ty: TypeId) -> String {
        let assignability_display = self.format_type_for_assignability_message(ty);
        if let Some(name) = self.synthesized_object_parent_display_name(ty) {
            let generic_prefix = format!("{name}<");
            if assignability_display.starts_with(&generic_prefix) {
                return assignability_display;
            }
            return name;
        }
        if self.ctx.definition_store.find_def_for_type(ty).is_none()
            && self
                .ctx
                .definition_store
                .find_type_alias_by_body(ty)
                .is_some()
        {
            return self.format_type_diagnostic_structural(ty);
        }
        assignability_display
    }

    pub(crate) fn named_type_display_name(&self, type_id: TypeId) -> Option<String> {
        if self.ctx.types.get_display_alias(type_id).is_some() {
            return None;
        }

        if let Some(def_id) = tsz_solver::lazy_def_id(self.ctx.types, type_id)
            .or_else(|| self.ctx.definition_store.find_def_for_type(type_id))
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            let name = self.ctx.types.resolve_atom(def.name);
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }

        if let Some(shape_id) =
            tsz_solver::type_queries::get_object_shape_id(self.ctx.types, type_id)
        {
            let shape = self.ctx.types.object_shape(shape_id);
            if let Some(sym_id) = shape.symbol
                && let Some(symbol) = self.get_cross_file_symbol(sym_id)
                && !symbol.escaped_name.is_empty()
            {
                return Some(symbol.escaped_name.clone());
            }
        }

        if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id)
            && let Some(symbol) = self.get_cross_file_symbol(sym_id)
            && !symbol.escaped_name.is_empty()
        {
            return Some(symbol.escaped_name.clone());
        }

        None
    }

    pub(crate) fn preferred_constructor_display_name(&mut self, type_id: TypeId) -> Option<String> {
        let base_name = self.named_type_display_name(type_id)?;
        let is_callable_or_constructible =
            tsz_solver::type_queries::get_callable_shape(self.ctx.types, type_id).is_some()
                || tsz_solver::type_queries::get_function_shape(self.ctx.types, type_id).is_some();
        if !is_callable_or_constructible {
            return None;
        }

        let constructor_name = format!("{base_name}Constructor");
        let constructor_type = self.resolve_lib_type_by_name(&constructor_name)?;
        if matches!(constructor_type, TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        let source_display =
            self.format_type_for_assignability_message(self.widen_type_for_display(type_id));
        let constructor_display = self
            .format_type_for_assignability_message(self.widen_type_for_display(constructor_type));
        (source_display == constructor_display).then_some(constructor_name)
    }

    fn jsdoc_annotated_expression_display(
        &mut self,
        expr_idx: NodeIndex,
        target: TypeId,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = expr_idx;
        loop {
            if self
                .ctx
                .arena
                .node_info(current)
                .and_then(|info| self.ctx.arena.get(info.parent))
                .is_some_and(|parent| {
                    matches!(
                        parent.kind,
                        syntax_kind_ext::PROPERTY_ASSIGNMENT
                            | syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                            | syntax_kind_ext::METHOD_DECLARATION
                            | syntax_kind_ext::GET_ACCESSOR
                            | syntax_kind_ext::SET_ACCESSOR
                    )
                })
            {
                return None;
            }
            if let Some(type_id) = self.jsdoc_type_annotation_for_node_direct(current) {
                let display_type = self.widen_function_like_display_type(type_id);
                return Some(self.format_assignability_type_for_message(display_type, target));
            }

            let node = self.ctx.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return None;
            }

            let paren = self.ctx.arena.get_parenthesized(node)?;
            current = paren.expression;
        }
    }

    fn empty_array_literal_source_type_display(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let literal = self.ctx.arena.get_literal_expr(node)?;
        if !literal.elements.nodes.is_empty() {
            return None;
        }
        Some(if self.ctx.strict_null_checks() {
            "never[]".to_string()
        } else {
            "undefined[]".to_string()
        })
    }

    fn object_literal_source_type_display(
        &mut self,
        expr_idx: NodeIndex,
        target: Option<TypeId>,
    ) -> Option<String> {
        // Only skip parentheses, not type assertions.  When the source is
        // `<foo>({})`, the diagnostic should display the asserted type name
        // `foo`, not the inner object literal `{}`.  Returning `None` here
        // lets the caller fall through to `get_type_of_node` which yields
        // the asserted type.
        let expr_idx = self.ctx.arena.skip_parenthesized(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let literal = self.ctx.arena.get_literal_expr(node)?;
        let target = target.map(|target| self.evaluate_type_for_assignability(target));
        let target_shape = target.and_then(|target| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target)
        });
        let mut parts = Vec::new();
        for child_idx in literal.elements.nodes.iter().copied() {
            let child = self.ctx.arena.get(child_idx)?;
            let prop = self.ctx.arena.get_property_assignment(child)?;
            let name_node = self.ctx.arena.get(prop.name)?;
            let display_name = match name_node.kind {
                k if k == tsz_scanner::SyntaxKind::Identifier as u16 => self
                    .ctx
                    .arena
                    .get_identifier(name_node)?
                    .escaped_text
                    .clone(),
                k if k == tsz_scanner::SyntaxKind::StringLiteral as u16
                    || k == tsz_scanner::SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
                {
                    let lit = self.ctx.arena.get_literal(name_node)?;
                    format!("\"{}\"", lit.text)
                }
                k if k == tsz_scanner::SyntaxKind::NumericLiteral as u16 => {
                    self.ctx.arena.get_literal(name_node)?.text.clone()
                }
                _ => return None,
            };
            let property_name = self
                .get_property_name(prop.name)
                .map(|name| self.ctx.types.intern_string(&name));
            let value_type = self.get_type_of_node(prop.initializer);
            if value_type == TypeId::ERROR {
                return None;
            }

            // tsc preserves literal types in fresh object literal error messages
            // when the target property type accepts literals (e.g., discriminated
            // unions: `tag: "A" | "B" | "C"`). Otherwise it widens (e.g., `string`).
            // Check the target property type to decide.
            let target_accepts_literal = property_name
                .and_then(|name| {
                    let shape = target_shape.as_ref()?;
                    shape
                        .properties
                        .iter()
                        .find(|p| p.name == name)
                        .map(|p| p.type_id)
                })
                .is_some_and(|target_prop_type| {
                    self.type_contains_string_literal(target_prop_type)
                });
            if target_accepts_literal {
                if let Some(literal_display) = self.literal_expression_display(prop.initializer) {
                    parts.push(format!("{display_name}: {literal_display}"));
                    continue;
                }
            }

            // For nested object literals, recurse
            if let Some(nested_display) =
                self.object_literal_source_type_display(prop.initializer, None)
            {
                parts.push(format!("{display_name}: {nested_display}"));
                continue;
            }

            // Fall back to type system for non-literal expressions.
            // For function properties, merge parameter types from target shape.
            let value_display_type = property_name
                .and_then(|name| {
                    let shape = target_shape.as_ref()?;
                    shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == name)
                        .map(|prop| prop.type_id)
                })
                .filter(|target_prop_type| {
                    crate::query_boundaries::diagnostics::function_shape(self.ctx.types, value_type)
                        .is_some()
                        && crate::query_boundaries::diagnostics::function_shape(
                            self.ctx.types,
                            *target_prop_type,
                        )
                        .is_some()
                })
                .and_then(|target_prop_type| {
                    let value_shape = crate::query_boundaries::diagnostics::function_shape(
                        self.ctx.types,
                        value_type,
                    )?;
                    let target_shape = crate::query_boundaries::diagnostics::function_shape(
                        self.ctx.types,
                        target_prop_type,
                    )?;
                    let merged_params: Vec<_> = value_shape
                        .params
                        .iter()
                        .zip(target_shape.params.iter())
                        .map(|(value_param, target_param)| tsz_solver::ParamInfo {
                            type_id: target_param.type_id,
                            ..*value_param
                        })
                        .collect();
                    let merged = self
                        .ctx
                        .types
                        .factory()
                        .function(tsz_solver::FunctionShape {
                            type_params: value_shape.type_params.clone(),
                            params: merged_params,
                            this_type: value_shape.this_type,
                            return_type: value_shape.return_type,
                            type_predicate: value_shape.type_predicate,
                            is_constructor: value_shape.is_constructor,
                            is_method: value_shape.is_method,
                        });
                    Some(merged)
                })
                .unwrap_or(value_type);
            let widened_value_display_type =
                self.widen_function_like_display_type(value_display_type);
            let value_display =
                self.format_type_for_assignability_message(widened_value_display_type);
            parts.push(format!("{display_name}: {value_display}"));
        }

        if parts.is_empty() {
            return Some("{}".to_string());
        }

        Some(format!("{{ {}; }}", parts.join("; ")))
    }

    pub(super) fn format_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if source == TypeId::UNDEFINED
            && self.ctx.arena.get(anchor_idx).is_some_and(|node| {
                node.kind == tsz_parser::parser::syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
            })
        {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(display) = self.jsdoc_annotated_expression_display(anchor_idx, target) {
            return display;
        }

        if tsz_solver::literal_value(self.ctx.types, source).is_some()
            && tsz_solver::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(display) = self.preferred_evaluated_source_display(source) {
            return display;
        }

        if self.is_literal_sensitive_assignment_target(target)
            && let Some(display) = self.literal_expression_display(anchor_idx)
        {
            return display;
        }
        if self.is_literal_sensitive_assignment_target(target)
            && tsz_solver::literal_value(self.ctx.types, source).is_some()
        {
            return self.format_assignability_type_for_message(source, target);
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if self.is_literal_sensitive_assignment_target(target)
                && let Some(display) = self.literal_expression_display(expr_idx)
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            // Only use the node-derived type when it plausibly represents the
            // source of the assignment, not the target.  For-of loops pass the
            // element type as `source` but anchor the diagnostic at the loop
            // variable whose node type equals the *target* (declared variable
            // type), not the source.  When the node type matches the target but
            // not the source, the anchor is the assignment target — skip
            // node-based resolution to avoid confusing "Type 'X' is not
            // assignable to type 'X'" messages.
            let node_is_target_not_source = expr_type == target && expr_type != source;
            let node_type_matches_source = expr_type != TypeId::ERROR && !node_is_target_not_source;
            if node_type_matches_source {
                if let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                    && self.should_prefer_declared_source_annotation_display(
                        expr_idx,
                        expr_type,
                        &annotation_text,
                    )
                {
                    return self.format_declared_annotation_for_diagnostic(&annotation_text);
                }
                let display_type =
                    if self.should_widen_enum_member_assignment_source(expr_type, target) {
                        self.widen_enum_member_type(expr_type)
                    } else {
                        expr_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                let display_type = if self.is_literal_sensitive_assignment_target(target) {
                    display_type
                } else if tsz_solver::keyof_inner_type(self.ctx.types, display_type).is_some() {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    crate::query_boundaries::common::widen_type(self.ctx.types, evaluated)
                } else {
                    crate::query_boundaries::common::widen_type(self.ctx.types, display_type)
                };
                return self.format_assignability_type_for_message(display_type, target);
            }

            if node_type_matches_source
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                return display;
            }
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.literal_expression_display(expr_idx)
                && (self.is_literal_sensitive_assignment_target(target)
                    || (self.assignment_source_is_return_expression(anchor_idx)
                        && crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            target,
                        )
                        && !self.is_property_assignment_initializer(expr_idx)
                        // When the target is a bare type parameter (e.g. T),
                        // tsc widens literals in error messages: "Type 'string'
                        // is not assignable to type 'T'" rather than "Type '\"\"'
                        // is not assignable to type 'T'". Preserve literals only
                        // for complex generic targets like indexed access types.
                        && !self.target_is_bare_type_parameter(target)))
            {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if expr_type != TypeId::ERROR
                && let Some(annotation_text) =
                    self.declared_diagnostic_source_annotation_text(expr_idx)
                && self.should_prefer_declared_source_annotation_display(
                    expr_idx,
                    expr_type,
                    &annotation_text,
                )
            {
                return self.format_declared_annotation_for_diagnostic(&annotation_text);
            }
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = self.widen_type_for_display(expr_type);
                if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                    self.widen_enum_member_type(widened_expr_type)
                } else {
                    widened_expr_type
                }
            } else {
                self.widen_type_for_display(source)
            };
            let display_type = self.widen_function_like_display_type(display_type);

            if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
                && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
            {
                return self.format_assignability_type_for_message(display_type, target);
            }

            if expr_type == TypeId::ERROR
                && let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
            {
                return display;
            }

            let display_type =
                if tsz_solver::keyof_inner_type(self.ctx.types, display_type).is_some() {
                    let evaluated = self.evaluate_type_for_assignability(display_type);
                    crate::query_boundaries::common::widen_type(self.ctx.types, evaluated)
                } else {
                    display_type
                };
            let formatted = self.format_type_for_assignability_message(display_type);
            let resolved_for_access = self.resolve_type_for_property_access(display_type);
            let resolved = self.judge_evaluate(resolved_for_access);
            let resolver =
                tsz_solver::objects::index_signatures::IndexSignatureResolver::new(self.ctx.types);
            if !formatted.contains('{')
                && !formatted.contains('[')
                && !formatted.contains('|')
                && !formatted.contains('&')
                && !formatted.contains('<')
                && !crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    display_type,
                )
                && (resolver.has_index_signature(
                    resolved,
                    tsz_solver::objects::index_signatures::IndexKind::String,
                ) || resolver.has_index_signature(
                    resolved,
                    tsz_solver::objects::index_signatures::IndexKind::Number,
                ))
            {
                if let Some(structural) = self.format_structural_indexed_object_type(resolved) {
                    return structural;
                }
                return self.format_type(resolved);
            }
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx)
                && !display.starts_with("keyof ")
                && !display.starts_with("typeof ")
                && !display.contains("[P in ")
                && !display.contains("[K in ")
                // Don't use annotation text for union types — the TypeFormatter
                // reorders null/undefined to the end to match tsc's display.
                // Annotation text preserves the user's original order which
                // differs from tsc's canonical display.
                && !display.contains(" | ")
                // Don't use annotation text when the formatted type includes
                // `| undefined` (added by strictNullChecks for optional params)
                // that the raw annotation text doesn't have. The annotation text
                // reflects the source code literally and misses the semantic
                // `| undefined` injection.
                && (!formatted.contains("| undefined") || display.contains("| undefined"))
            {
                if tsz_solver::type_queries::get_enum_def_id(self.ctx.types, display_type).is_some()
                {
                    return self.format_assignability_type_for_message(display_type, target);
                }
                return self.format_annotation_like_type(&display);
            }
            return formatted;
        }

        self.format_assignability_type_for_message(source, target)
    }

    pub(super) fn format_assignment_target_type_for_diagnostic(
        &mut self,
        target: TypeId,
        source: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        let target_expr = self
            .assignment_target_expression(anchor_idx)
            .unwrap_or(anchor_idx);

        if let Some(display) = self.declared_type_annotation_text_for_expression(target_expr)
            && (display.starts_with("keyof ")
                || display.starts_with("typeof ")
                || display.contains("[P in ")
                || display.contains("[K in "))
        {
            return self.format_annotation_like_type(&display);
        }

        self.format_assignability_type_for_message(target, source)
    }

    pub(super) fn format_nested_assignment_source_type_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> String {
        if tsz_solver::literal_value(self.ctx.types, source).is_some()
            && tsz_solver::string_intrinsic_components(self.ctx.types, target)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(source);
            return self.format_assignability_type_for_message(widened, target);
        }

        if let Some(display) = self.preferred_evaluated_source_display(source) {
            return display;
        }

        if let Some(expr_idx) = self.direct_diagnostic_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            if expr_type != TypeId::ERROR {
                let widened_expr_type = self.widen_type_for_display(expr_type);
                let display_type =
                    if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                        self.widen_enum_member_type(widened_expr_type)
                    } else {
                        widened_expr_type
                    };
                let display_type = self.widen_function_like_display_type(display_type);
                return self.format_assignability_type_for_message(display_type, target);
            }
        }

        if let Some(expr_idx) = self.assignment_source_expression(anchor_idx) {
            if let Some(display) = self.declared_type_annotation_text_for_expression(expr_idx) {
                return display;
            }

            if let Some(display) = self.empty_array_literal_source_type_display(expr_idx) {
                return display;
            }

            if let Some(display) = self.object_literal_source_type_display(expr_idx, Some(target)) {
                return display;
            }

            let expr_type = self.get_type_of_node(expr_idx);
            let display_type = if expr_type != TypeId::ERROR {
                let widened_expr_type = if self.is_literal_sensitive_assignment_target(target) {
                    expr_type
                } else {
                    self.widen_type_for_display(expr_type)
                };
                if self.should_widen_enum_member_assignment_source(widened_expr_type, target) {
                    self.widen_enum_member_type(widened_expr_type)
                } else {
                    widened_expr_type
                }
            } else {
                self.widen_type_for_display(source)
            };
            let display_type = self.widen_function_like_display_type(display_type);
            return self.format_assignability_type_for_message(display_type, target);
        }

        self.format_assignability_type_for_message(source, target)
    }

    fn preferred_evaluated_source_display(&mut self, source: TypeId) -> Option<String> {
        if tsz_solver::is_template_literal_type(self.ctx.types, source) {
            return Some(self.format_type_diagnostic_structural(source));
        }

        let evaluated = self.evaluate_type_for_assignability(source);
        if evaluated == source || evaluated == TypeId::ERROR {
            return None;
        }

        if tsz_solver::literal_value(self.ctx.types, evaluated).is_some()
            || tsz_solver::is_template_literal_type(self.ctx.types, evaluated)
            || tsz_solver::string_intrinsic_components(self.ctx.types, evaluated).is_some()
        {
            return Some(self.format_type_diagnostic_structural(evaluated));
        }

        None
    }

    pub(super) fn is_literal_sensitive_assignment_target(&mut self, target: TypeId) -> bool {
        if tsz_solver::string_intrinsic_components(self.ctx.types, target)
            .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            return false;
        }

        let target = self.evaluate_type_for_assignability(target);
        self.is_literal_sensitive_assignment_target_inner(target)
    }

    /// Check if the target type is a bare type parameter (e.g. `T`).
    /// Used to decide whether to widen literals in error messages:
    /// tsc widens `""` → `string` when the target is a simple type param,
    /// but preserves literals for complex generic targets like `Type[K]`.
    pub(super) fn target_is_bare_type_parameter(&self, target: TypeId) -> bool {
        crate::query_boundaries::state::checking::is_type_parameter(self.ctx.types, target)
    }

    fn is_literal_sensitive_assignment_target_inner(&self, target: TypeId) -> bool {
        if tsz_solver::literal_value(self.ctx.types, target).is_some() {
            return true;
        }
        if tsz_solver::type_queries::get_enum_def_id(self.ctx.types, target).is_some() {
            return true;
        }
        if tsz_solver::type_queries::is_symbol_or_unique_symbol(self.ctx.types, target)
            && target != TypeId::SYMBOL
        {
            return true;
        }
        // Template literal types (e.g., `:${string}:`) expect specific string
        // patterns — preserving the source literal in the diagnostic is more
        // informative than showing widened `string`.
        if tsz_solver::is_template_literal_type(self.ctx.types, target) {
            return true;
        }
        if let Some(list) = tsz_solver::union_list_id(self.ctx.types, target)
            .or_else(|| tsz_solver::intersection_list_id(self.ctx.types, target))
        {
            return self
                .ctx
                .types
                .type_list(list)
                .iter()
                .copied()
                .any(|member| self.is_literal_sensitive_assignment_target_inner(member));
        }
        target == TypeId::NEVER
    }

    fn should_widen_enum_member_assignment_source(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let widened_source = self.widen_enum_member_type(source);
        if widened_source == source {
            return false;
        }

        let target = self.evaluate_type_for_assignability(target);
        tsz_solver::type_queries::get_enum_def_id(self.ctx.types, target).is_none()
            && crate::query_boundaries::common::union_members(self.ctx.types, target).is_none()
            && crate::query_boundaries::common::intersection_members(self.ctx.types, target)
                .is_none()
    }

    pub(super) fn unresolved_unused_renaming_property_in_type_query(
        &self,
        name: &str,
        idx: NodeIndex,
    ) -> Option<String> {
        let mut saw_type_query = false;
        let mut current = idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::TYPE_QUERY {
                saw_type_query = true;
            }

            if matches!(
                node.kind,
                syntax_kind_ext::FUNCTION_TYPE
                    | syntax_kind_ext::CONSTRUCTOR_TYPE
                    | syntax_kind_ext::CALL_SIGNATURE
                    | syntax_kind_ext::CONSTRUCT_SIGNATURE
                    | syntax_kind_ext::METHOD_SIGNATURE
                    | syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::METHOD_DECLARATION
                    | syntax_kind_ext::CONSTRUCTOR
                    | syntax_kind_ext::GET_ACCESSOR
                    | syntax_kind_ext::SET_ACCESSOR
            ) {
                if !saw_type_query {
                    return None;
                }
                return self.find_renamed_binding_property_for_name(current, name);
            }

            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        None
    }

    fn find_renamed_binding_property_for_name(
        &self,
        root: NodeIndex,
        name: &str,
    ) -> Option<String> {
        let mut stack = vec![root];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(node)
                && binding.property_name.is_some()
                && binding.name.is_some()
                && self.ctx.arena.get_identifier_text(binding.name) == Some(name)
            {
                let prop_name = self
                    .ctx
                    .arena
                    .get_identifier_text(binding.property_name)
                    .map(str::to_string)?;
                return Some(prop_name);
            }

            stack.extend(self.ctx.arena.get_children(node_idx));
        }
        None
    }

    pub(super) fn has_more_specific_diagnostic_at_span(&self, start: u32, length: u32) -> bool {
        self.ctx.diagnostics.iter().any(|diag| {
            diag.start == start
                && diag.length == length
                && diag.code != diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        })
    }

    pub(crate) fn has_diagnostic_code_within_span(&self, start: u32, end: u32, code: u32) -> bool {
        self.ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == code && diag.start >= start && diag.start < end)
    }
}
