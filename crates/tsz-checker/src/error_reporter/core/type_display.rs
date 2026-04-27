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

            // Only wrap intersection parts in parens when there are multiple union
            // alternatives. A standalone intersection like `T & (0 | 1 | 2)` should
            // not get extra outer parens, but in a union like `A & B | C & D`, both
            // intersection parts need parens: `(A & B) | (C & D)`.
            if parts.len() == 1 {
                return parts.into_iter().next().unwrap_or_default();
            }

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
            // After newline truncation, reject if braces are unbalanced
            // (e.g., multi-line `{\n  foo: bar;\n}` gets truncated to `{`)
            let open_brace = text.chars().filter(|&c| c == '{').count();
            let close_brace = text.chars().filter(|&c| c == '}').count();
            if open_brace != close_brace {
                return None;
            }
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
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, type_id) {
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
        let mut widened = crate::query_boundaries::common::widen_type(self.ctx.types, type_id);
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, widened)
        {
            let widened_return =
                self.widen_fresh_object_literal_properties_for_display(shape.return_type);
            if widened_return != shape.return_type {
                widened = self
                    .ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: shape.this_type,
                        return_type: widened_return,
                        type_predicate: shape.type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    });
            }
        } else if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, widened)
        {
            let mut widened_shape = shape.as_ref().clone();
            let mut changed = false;

            for sig in &mut widened_shape.call_signatures {
                let widened_return =
                    self.widen_fresh_object_literal_properties_for_display(sig.return_type);
                if widened_return != sig.return_type {
                    changed = true;
                    sig.return_type = widened_return;
                }
            }
            for sig in &mut widened_shape.construct_signatures {
                let widened_return =
                    self.widen_fresh_object_literal_properties_for_display(sig.return_type);
                if widened_return != sig.return_type {
                    changed = true;
                    sig.return_type = widened_return;
                }
            }

            if changed {
                widened = self.ctx.types.factory().callable(widened_shape);
            }
        }
        if let Some(def_id) = constructor_display_def {
            self.ctx
                .definition_store
                .register_type_to_def(widened, def_id);
        }
        widened
    }

    pub(crate) fn widen_fresh_object_literal_properties_for_display(&self, ty: TypeId) -> TypeId {
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return ty;
        };
        // Only widen properties when the outer object type is itself a fresh
        // object literal (e.g. inferred return type from `() => ({ a: 1 })`).
        // Annotated types like `{ a: "x" }` carry the user's intent and must
        // not have their literal property types widened away in diagnostics —
        // tsc preserves them as-is, so when we receive a non-fresh shape here
        // we have to leave it untouched.
        if !crate::query_boundaries::common::is_fresh_object_type(self.ctx.types, ty) {
            return ty;
        }
        let mut widened_shape = shape.as_ref().clone();
        let mut changed = false;
        for prop in &mut widened_shape.properties {
            let widened_read =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, prop.type_id);
            let widened_write = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                prop.write_type,
            );
            if widened_read != prop.type_id || widened_write != prop.write_type {
                changed = true;
            }
            prop.type_id = widened_read;
            prop.write_type = widened_write;
        }
        if !changed {
            return ty;
        }
        self.ctx.types.factory().object_with_index(widened_shape)
    }

    pub(in crate::error_reporter) fn normalize_property_receiver_application_display_type(
        &mut self,
        ty: TypeId,
    ) -> TypeId {
        let Some(app) = query::type_application(self.ctx.types, ty) else {
            return ty;
        };

        let args: Vec<_> = app
            .args
            .iter()
            .map(|&arg| self.normalize_property_receiver_application_display_arg(arg))
            .collect();

        if args == app.args {
            ty
        } else {
            self.ctx.types.factory().application(app.base, args)
        }
    }

    fn normalize_property_receiver_application_display_alias(&mut self, ty: TypeId) -> TypeId {
        let Some(app) = query::type_application(self.ctx.types, ty) else {
            return ty;
        };

        let args: Vec<_> = app
            .args
            .iter()
            .map(|&arg| self.normalize_property_receiver_application_display_arg(arg))
            .collect();

        if args == app.args {
            ty
        } else {
            self.ctx.types.factory().application(app.base, args)
        }
    }

    fn normalize_property_receiver_application_display_arg(&mut self, ty: TypeId) -> TypeId {
        // Only resolve `Lazy(DefId)` references via the type environment.
        // Calling `evaluate_type_with_env` on richer shapes (e.g. `keyof T`,
        // `T[K]`, conditional types) eagerly expands them to their evaluated
        // structural form and loses the original syntactic identity that tsc
        // preserves in property-receiver diagnostics. Structural recursion
        // below already handles applications/unions/intersections/objects.
        if crate::query_boundaries::common::is_lazy_type(self.ctx.types.as_type_database(), ty) {
            let evaluated = self.evaluate_type_with_env(ty);
            if evaluated != ty {
                return self.normalize_property_receiver_application_display_arg(evaluated);
            }
        }

        if let Some(app) = query::type_application(self.ctx.types, ty) {
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| self.normalize_property_receiver_application_display_arg(arg))
                .collect();
            return if args == app.args {
                ty
            } else {
                self.ctx.types.factory().application(app.base, args)
            };
        }

        if let Some(members) = query::union_members(self.ctx.types, ty) {
            let normalized: Vec<_> = members
                .iter()
                .map(|&member| self.normalize_property_receiver_application_display_arg(member))
                .collect();
            return if normalized == members {
                ty
            } else {
                self.ctx.types.factory().union_preserve_members(normalized)
            };
        }

        if let Some(members) = query::intersection_members(self.ctx.types, ty) {
            let normalized: Vec<_> = members
                .iter()
                .map(|&member| self.normalize_property_receiver_application_display_arg(member))
                .collect();
            return if normalized == members {
                ty
            } else {
                self.ctx.types.factory().intersection(normalized)
            };
        }

        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)
        else {
            return ty;
        };
        let should_widen_properties =
            crate::query_boundaries::common::is_fresh_object_type(self.ctx.types, ty)
                || (self.ctx.types.get_display_properties(ty).is_some() && shape.symbol.is_none());
        if !should_widen_properties {
            return ty;
        }

        let mut normalized_shape = shape.as_ref().clone();
        let mut changed = self.ctx.types.get_display_properties(ty).is_some();

        for prop in &mut normalized_shape.properties {
            let normalized_read =
                self.normalize_property_receiver_application_display_arg(prop.type_id);
            let normalized_write =
                self.normalize_property_receiver_application_display_arg(prop.write_type);
            let widened_read = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                normalized_read,
            );
            let widened_write = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                normalized_write,
            );

            if widened_read != prop.type_id || widened_write != prop.write_type {
                changed = true;
            }

            prop.type_id = widened_read;
            prop.write_type = widened_write;
        }

        if let Some(index) = normalized_shape.string_index.as_mut() {
            let normalized =
                self.normalize_property_receiver_application_display_arg(index.value_type);
            let widened =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, normalized);
            if widened != index.value_type {
                changed = true;
                index.value_type = widened;
            }
        }

        if let Some(index) = normalized_shape.number_index.as_mut() {
            let normalized =
                self.normalize_property_receiver_application_display_arg(index.value_type);
            let widened =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, normalized);
            if widened != index.value_type {
                changed = true;
                index.value_type = widened;
            }
        }

        if changed {
            let new_ty = self.ctx.types.factory().object_with_index(normalized_shape);
            if let Some(alias_origin) = self.ctx.types.get_display_alias(ty) {
                let alias_origin =
                    self.normalize_property_receiver_application_display_alias(alias_origin);
                if query::type_application(self.ctx.types, alias_origin).is_some() {
                    self.ctx
                        .types
                        .store_display_alias_preferring_application(new_ty, alias_origin);
                } else {
                    self.ctx.types.store_display_alias(new_ty, alias_origin);
                }
            }
            new_ty
        } else {
            ty
        }
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
        let ty = crate::query_boundaries::common::evaluate_type(self.ctx.types, ty);
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
            || crate::query_boundaries::common::function_shape_id(self.ctx.types, ty).is_some()
            || crate::query_boundaries::common::callable_shape_id(self.ctx.types, ty).is_some()
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

        crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty).is_some_and(
            |shape| {
                shape.properties.len() > 6
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
            },
        )
    }

    fn normalize_assignability_display_type_inner(
        &mut self,
        ty: TypeId,
        visiting: &mut FxHashSet<TypeId>,
        depth: usize,
    ) -> TypeId {
        const MAX_ASSIGNABILITY_DISPLAY_DEPTH: usize = 12;
        // Type parameters should not be normalized — they should display as their
        // name (e.g., `T`) not their constraint (e.g., `String`). The solver's
        // `get_object_shape` looks through type parameter constraints, which causes
        // the object-shape branch below to incorrectly resolve `T extends String`
        // to the `String` interface's object type.
        if crate::query_boundaries::state::checking::is_type_parameter_like(self.ctx.types, ty) {
            return ty;
        }
        // Literal types should be preserved as-is — don't evaluate/widen them
        // to their base type.  tsc shows `"TypeTwo"` in error messages, not
        // `string`.  Without this guard the else-branch evaluates the literal
        // and the widened primitive replaces the original.
        if crate::query_boundaries::common::literal_value(self.ctx.types, ty).is_some() {
            return ty;
        }
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
            } else if query::function_shape(self.ctx.types, ty).is_some_and(|shape| {
                crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    shape.return_type,
                )
            }) {
                ty
            } else {
                let evaluated =
                    if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
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
                        .map(|param| {
                            // Skip normalizing TypeQuery param types to preserve typeof
                            // syntax, matching tsc's behavior of not expanding typeof
                            // references in parameter positions.
                            let type_id = if crate::query_boundaries::common::is_type_query_type(
                                self.ctx.types,
                                param.type_id,
                            ) {
                                param.type_id
                            } else {
                                self.normalize_assignability_display_type_inner(
                                    param.type_id,
                                    visiting,
                                    depth + 1,
                                )
                            };
                            tsz_solver::ParamInfo { type_id, ..*param }
                        })
                        .collect();
                    // Skip normalizing TypeQuery return types to preserve the typeof
                    // syntax. Resolving TypeQuery to the full function type causes double
                    // arrows like `() => () => typeof fn` instead of `() => typeof fn`.
                    let return_type = if crate::query_boundaries::common::is_type_query_type(
                        self.ctx.types,
                        shape.return_type,
                    ) || crate::query_boundaries::common::is_conditional_type(
                        self.ctx.types,
                        shape.return_type,
                    ) {
                        shape.return_type
                    } else {
                        self.normalize_assignability_display_type_inner(
                            shape.return_type,
                            visiting,
                            depth + 1,
                        )
                    };
                    let return_type = if crate::query_boundaries::common::is_conditional_type(
                        self.ctx.types,
                        shape.return_type,
                    ) {
                        return_type
                    } else {
                        let widened = crate::query_boundaries::common::widen_type(
                            self.ctx.types,
                            return_type,
                        );
                        self.widen_fresh_object_literal_properties_for_display(widened)
                    };
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
                } else if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    evaluated,
                ) {
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
                        // Propagate display_alias so the formatter can still
                        // recover the named form (e.g., `Array<string>`) for
                        // types whose property types changed during normalization.
                        if let Some(alias_origin) = self.ctx.types.get_display_alias(evaluated) {
                            self.ctx.types.store_display_alias(new_ty, alias_origin);
                        }
                        // Propagate definition store registration so the
                        // formatter can still show named types (interfaces,
                        // classes) whose properties changed during
                        // normalization — e.g., `Date` instead of the full
                        // structural expansion.
                        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(evaluated)
                        {
                            self.ctx
                                .definition_store
                                .register_type_to_def(new_ty, def_id);
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
                    if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
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
            let evaluated =
                if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
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
                // Skip normalizing TypeQuery return types to preserve the typeof
                // syntax. Resolving TypeQuery to the full function type causes double
                // arrows like `() => () => typeof fn` instead of `() => typeof fn`.
                let return_type = if crate::query_boundaries::common::is_type_query_type(
                    self.ctx.types,
                    shape.return_type,
                ) || crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    shape.return_type,
                ) {
                    shape.return_type
                } else {
                    self.normalize_assignability_display_type_inner(
                        shape.return_type,
                        visiting,
                        depth + 1,
                    )
                };
                let return_type = if crate::query_boundaries::common::is_conditional_type(
                    self.ctx.types,
                    shape.return_type,
                ) {
                    return_type
                } else {
                    let widened =
                        crate::query_boundaries::common::widen_type(self.ctx.types, return_type);
                    self.widen_fresh_object_literal_properties_for_display(widened)
                };
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
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, evaluated)
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
                    // Propagate display_alias and def-store registration so the
                    // formatter can still show named types (Date, Error, etc.)
                    // after normalization modifies property types.
                    if let Some(alias_origin) = self.ctx.types.get_display_alias(evaluated) {
                        self.ctx.types.store_display_alias(new_ty, alias_origin);
                    }
                    if let Some(def_id) = self.ctx.definition_store.find_def_for_type(evaluated) {
                        self.ctx
                            .definition_store
                            .register_type_to_def(new_ty, def_id);
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
        let ty = crate::query_boundaries::common::evaluate_type(self.ctx.types, ty);
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
        // Evaluate for structural analysis but preserve original members for display.
        // NoInfer<T> wrappers and type aliases are stripped by evaluation, but the
        // display should preserve them (tsc shows `NoInfer<{x: string}>` not `{x: string}`).
        let evaluated = crate::query_boundaries::common::evaluate_type(self.ctx.types, ty);
        let original_members = query::union_members(self.ctx.types, ty);
        if let Some(members) = query::union_members(self.ctx.types, evaluated) {
            let object_like: Vec<_> = members
                .iter()
                .enumerate()
                .filter(|(_, member)| {
                    let evaluated =
                        crate::query_boundaries::common::evaluate_type(self.ctx.types, **member);
                    !crate::query_boundaries::common::is_primitive_type(self.ctx.types, evaluated)
                        && !crate::query_boundaries::common::contains_type_parameters(
                            self.ctx.types,
                            evaluated,
                        )
                })
                .map(|(i, member)| {
                    // Use original (pre-evaluation) member if available for display
                    original_members
                        .as_ref()
                        .and_then(|orig| orig.get(i).copied())
                        .unwrap_or(*member)
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
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)?;
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
        if crate::query_boundaries::common::is_lazy_type(self.ctx.types, ty) {
            return self.format_type_diagnostic_widened(ty);
        }

        // Generic Application target (e.g., `Record<Keys, unknown>`): tsc shows
        // the Application form in excess-property messages. Either the type is
        // an Application directly, or it's the evaluated result carrying a
        // display_alias back to the Application. In both cases, route through
        // the standard diagnostic formatter so the Application syntax is used.
        let is_application =
            crate::query_boundaries::common::type_application(self.ctx.types, ty).is_some();
        let evaluated_application = if is_application {
            None
        } else if let Some(alias) = self.ctx.types.get_display_alias(ty) {
            crate::query_boundaries::common::type_application(self.ctx.types, alias).map(|_| alias)
        } else {
            None
        };
        if is_application || evaluated_application.is_some() {
            let mut formatter = self
                .ctx
                .create_diagnostic_type_formatter()
                .with_display_properties()
                .with_skip_application_alias_names();
            return formatter.format(ty).into_owned();
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
            let preserve_intersection_parts = members.iter().any(|member| {
                crate::query_boundaries::common::evaluate_type(self.ctx.types, *member)
                    == TypeId::OBJECT
            });
            let mut changed = false;
            let parts: Vec<String> = members
                .iter()
                .map(|&member| {
                    if let Some(materialized) =
                        self.materialize_finite_mapped_type_for_display(member)
                    {
                        changed = true;
                        self.format_type_diagnostic_widened(materialized)
                    } else {
                        self.format_type_diagnostic_widened(member)
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
        self.format_type_diagnostic_widened(display_ty)
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
        if !crate::query_boundaries::common::is_keyof_type(self.ctx.types, other) {
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
        // Prefer Array<T> shorthand conversion in annotation text, but preserve
        // generic constraint surface syntax (`<T extends Array<U>>`) where tsc
        // keeps the declared Array form.
        formatted = Self::normalize_array_generic_to_shorthand(&formatted);
        formatted
    }

    /// Convert `Array<T>` to `T[]` and `ReadonlyArray<T>` to `readonly T[]`
    /// in annotation text to match tsc's diagnostic display.
    ///
    /// Do not normalize when the generic array appears directly in a type
    /// parameter `extends` clause; tsc preserves `Array<T>` there.
    fn normalize_array_generic_to_shorthand(text: &str) -> String {
        if !text.contains("Array<") {
            return text.to_string();
        }
        let is_extends_constraint_position = |s: &str, start: usize| -> bool {
            let prefix_start = start.saturating_sub(32);
            let prefix = &s[prefix_start..start];
            prefix.trim_end().ends_with("extends")
        };
        let mut out = String::with_capacity(text.len());
        let mut i = 0usize;

        while i < text.len() {
            let slice = &text[i..];

            // Process ReadonlyArray<T> first to avoid matching inner Array<T>.
            if slice.starts_with("ReadonlyArray<")
                && (i == 0 || !text.as_bytes()[i - 1].is_ascii_alphanumeric())
                && let Some(inner) = Self::extract_balanced_angle_bracket_content(text, i + 14)
            {
                let end = i + 14 + inner.len() + 1; // "ReadonlyArray<" + inner + ">"
                if is_extends_constraint_position(text, i) {
                    out.push_str(&text[i..end]);
                } else {
                    let needs_parens = inner.contains("=>") || inner.contains(" | ");
                    if needs_parens {
                        out.push_str(&format!("readonly ({inner})[]"));
                    } else {
                        out.push_str(&format!("readonly {inner}[]"));
                    }
                }
                i = end;
                continue;
            }

            if slice.starts_with("Array<")
                && (i == 0 || !text.as_bytes()[i - 1].is_ascii_alphanumeric())
                && let Some(inner) = Self::extract_balanced_angle_bracket_content(text, i + 6)
            {
                let end = i + 6 + inner.len() + 1; // "Array<" + inner + ">"
                if is_extends_constraint_position(text, i) {
                    out.push_str(&text[i..end]);
                } else {
                    let needs_parens = inner.contains("=>") || inner.contains(" | ");
                    if needs_parens {
                        out.push_str(&format!("({inner})[]"));
                    } else {
                        out.push_str(&format!("{inner}[]"));
                    }
                }
                i = end;
                continue;
            }

            if let Some(ch) = slice.chars().next() {
                out.push(ch);
                i += ch.len_utf8();
            } else {
                break;
            }
        }

        out
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
                // Check for inline JSDoc @satisfies annotation on the object literal
                // e.g. `/** @satisfies {Record<Keys, unknown>} */ ({ x: 1 })`
                if let Some(jsdoc_satisfies_text) =
                    self.jsdoc_satisfies_type_text_for_node(parent_idx)
                {
                    return Some(jsdoc_satisfies_text);
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

        if crate::query_boundaries::common::literal_value(self.ctx.types, ty).is_some() {
            return false;
        }

        // For TypeQuery (typeof X), don't use evaluated display - preserve the
        // typeof syntax instead of expanding to the full function type.
        // This prevents double function arrows like `() => () => typeof fn`.
        if crate::query_boundaries::common::is_type_query_type(self.ctx.types, ty) {
            return false;
        }

        // For function types with a return type that is a TypeQuery, don't use
        // the evaluated display. The evaluation would resolve the TypeQuery to
        // the full function type, causing double arrows like `() => () => typeof fn`.
        if let Some(fn_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty)
            && crate::query_boundaries::common::is_type_query_type(
                self.ctx.types,
                fn_shape.return_type,
            )
        {
            return false;
        }

        // Also check callable types (single call signature)
        if let Some(callable) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)
            && callable.call_signatures.len() == 1
        {
            let sig = &callable.call_signatures[0];
            if crate::query_boundaries::common::is_type_query_type(self.ctx.types, sig.return_type)
            {
                return false;
            }
        }

        // For generic Application types whose type alias body is an IndexedAccess
        // or Conditional type, use the evaluated form. tsc doesn't preserve the
        // alias name through these computed type forms, even when free type
        // parameters are present:
        // - `type Cb<T> = {noAlias: () => T}["noAlias"]` → show `() => number`, not `Cb<number>`
        // - `type IsArray<T> = T extends unknown[] ? true : false` → show `boolean`, not `IsArray<T>`
        //
        // This check MUST run before the generic-type-parameter guard below —
        // these alias shapes should substitute their evaluated form for display
        // even when the reference contains free type parameters.
        if crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
            && let Some(def_id) =
                crate::query_boundaries::common::get_application_lazy_def_id(self.ctx.types, ty)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && let Some(body) = def.body
            && (crate::query_boundaries::common::is_index_access_type(self.ctx.types, body)
                || crate::query_boundaries::common::is_conditional_type(self.ctx.types, body))
        {
            return true;
        }

        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
            || crate::query_boundaries::common::contains_type_parameters(self.ctx.types, evaluated)
        {
            return false;
        }

        if evaluated == TypeId::NEVER
            || crate::query_boundaries::common::literal_value(self.ctx.types, evaluated).is_some()
        {
            return true;
        }

        if (crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty).is_some()
            || crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, ty)
                .is_some())
            && (crate::query_boundaries::common::is_template_literal_type(
                self.ctx.types,
                evaluated,
            ) || crate::query_boundaries::common::string_intrinsic_components(
                self.ctx.types,
                evaluated,
            )
            .is_some())
        {
            return true;
        }

        if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_keyof_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_conditional_type(self.ctx.types, ty)
            && !crate::query_boundaries::common::is_generic_application(self.ctx.types, ty)
        {
            return false;
        }

        // For IndexAccess types, display the evaluated form when it resolves to a
        // concrete type (union, object, primitive). This makes error messages show
        // the resolved type instead of the raw indexed access syntax.
        // e.g., `Pairs<FooBar>[keyof FooBar]` → `{ key: "foo"; value: string; } | { key: "bar"; value: number; }`
        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty) {
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
        let shape = crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty)?;
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
                        if operand.parse::<f64>().is_ok_and(|value| value == 0.0) {
                            return Some("0".to_string());
                        }
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
