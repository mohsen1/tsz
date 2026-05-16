//! Type formatting and diagnostic anchor helpers for error reporter.

use crate::state::{CheckerState, MemberAccessLevel};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn format_type_diagnostic_for_assignability_display(&mut self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true);
        formatter.format(type_id).into_owned()
    }
    fn format_type_diagnostic_widened_for_assignability_display(
        &mut self,
        type_id: TypeId,
    ) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true);
        formatter.format(type_id).into_owned()
    }

    pub(crate) fn format_type_for_property_receiver_message(&mut self, type_id: TypeId) -> String {
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_skip_application_alias_names()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true);
        formatter.format(type_id).into_owned()
    }

    pub(crate) fn truncate_property_receiver_display(display: String) -> String {
        const MAX_PROPERTY_RECEIVER_DISPLAY_CHARS: usize = 320;
        let should_truncate = display.starts_with("Omit<") || display.starts_with("merge<");
        if display.len() <= MAX_PROPERTY_RECEIVER_DISPLAY_CHARS || !should_truncate {
            return display;
        }
        let display =
            super::property_receiver_formatting::elide_long_property_receiver_object_literals(
                display,
            );
        if display.starts_with("merge<") {
            let mut truncated: String = display
                .chars()
                .take(MAX_PROPERTY_RECEIVER_DISPLAY_CHARS - 2)
                .collect();
            truncated.push_str("..");
            return truncated;
        }
        display
            .chars()
            .take(MAX_PROPERTY_RECEIVER_DISPLAY_CHARS)
            .collect()
    }

    pub(crate) fn format_long_property_receiver_type_for_diagnostic(&self, ty: TypeId) -> String {
        tsz_solver::TypeFormatter::with_symbols(self.ctx.types, &self.ctx.binder.symbols)
            .with_def_store(&self.ctx.definition_store)
            .with_diagnostic_mode()
            .with_long_property_receiver_display()
            .with_skip_application_alias_names()
            .with_strict_null_checks(self.ctx.compiler_options.strict_null_checks)
            .format(ty)
            .into_owned()
    }

    pub(crate) fn named_type_display_name(&self, type_id: TypeId) -> Option<String> {
        if self.ctx.types.get_display_alias(type_id).is_some() {
            return None;
        }

        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)
            .or_else(|| self.ctx.definition_store.find_def_for_type(type_id))
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            let name = self.ctx.types.resolve_atom(def.name);
            if !name.is_empty() {
                return Some(name);
            }
        }

        if let Some(shape_id) =
            crate::query_boundaries::common::object_shape_id(self.ctx.types, type_id)
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

    /// Returns true when `ty` is a `TypeId` registered against a `TypeAlias`
    /// definition in the def store. Used to gate widening transformations
    /// that would rebuild the type into a structurally-equivalent but distinct
    /// `TypeId` lacking the alias registration — such transformations cause
    /// the diagnostic printer to emit the structural body
    /// (e.g. `string | Promise<SimpleType>`) instead of the alias name
    /// (`SimpleType`).
    pub(crate) fn is_registered_type_alias_for_display(&self, ty: TypeId) -> bool {
        let Some(def_id) = self.ctx.definition_store.find_def_for_type(ty) else {
            return false;
        };
        self.ctx
            .definition_store
            .get(def_id)
            .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
    }

    fn assignability_display_has_own_signature_type_params(&self, ty: TypeId) -> bool {
        if let Some(fn_shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty)
        {
            return !fn_shape.type_params.is_empty();
        }

        crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty).is_some_and(
            |shape| {
                shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                    .any(|sig| !sig.type_params.is_empty())
            },
        )
    }

    pub(crate) fn normalize_template_placeholder_spacing_for_display(&self, text: &str) -> String {
        if !text.contains("${") {
            return text.to_string();
        }

        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0usize;

        while i < chars.len() {
            if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                out.push('$');
                out.push('{');
                i += 2;

                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }

                let mut depth = 1usize;
                let mut inner = String::new();
                while i < chars.len() {
                    let ch = chars[i];
                    i += 1;
                    if ch == '{' {
                        depth += 1;
                        inner.push(ch);
                        continue;
                    }
                    if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        inner.push(ch);
                        continue;
                    }
                    inner.push(ch);
                }

                out.push_str(inner.trim_end());
                out.push('}');
                continue;
            }

            out.push(chars[i]);
            i += 1;
        }

        out
    }

    pub(crate) fn format_type_for_assignability_message(&mut self, ty: TypeId) -> String {
        let format_with_def_store = |state: &Self, type_id: TypeId| {
            let mut formatter =
                tsz_solver::TypeFormatter::with_symbols(state.ctx.types, &state.ctx.binder.symbols)
                    .with_def_store(&state.ctx.definition_store)
                    .with_diagnostic_mode()
                    // Match tsc: optional parameters display as `(a?: T)`.
                    .with_preserve_optional_parameter_surface_syntax(true)
                    .with_strict_null_checks(state.ctx.compiler_options.strict_null_checks)
                    .with_builtin_iterator_return_type(
                        if state.ctx.compiler_options.strict_builtin_iterator_return {
                            TypeId::UNDEFINED
                        } else {
                            TypeId::ANY
                        },
                    )
                    .with_exact_optional_property_types(
                        state.ctx.compiler_options.exact_optional_property_types,
                    );
            formatter.format(type_id).into_owned()
        };
        let is_generic_callable = |state: &Self, type_id: TypeId| {
            crate::query_boundaries::common::callable_shape_for_type(state.ctx.types, type_id)
                .is_some_and(|shape| {
                    shape
                        .call_signatures
                        .iter()
                        .chain(shape.construct_signatures.iter())
                        .any(|sig| !sig.type_params.is_empty())
                })
                || crate::query_boundaries::common::function_shape_for_type(
                    state.ctx.types,
                    type_id,
                )
                .is_some_and(|shape| !shape.type_params.is_empty())
        };

        // Diagnostics for alias-wrapped string mappings and similar evaluated
        // surfaces need nested lazy refs ready before we decide whether to show
        // the original alias text or the evaluated result.
        self.ensure_relation_input_ready(ty);

        // If the type is a TypeParameter or Infer, format it directly as
        // its name.  This must happen before any evaluation/resolution that
        // could replace the type parameter with its constraint type.
        // tsc always displays type parameters by name in assignability messages.
        if let Some(info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types.as_type_database(), ty)
        {
            return self.ctx.types.resolve_atom_ref(info.name).to_string();
        }

        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && def.type_params.is_empty()
        {
            if let Some(body) = def.body {
                if crate::query_boundaries::common::is_type_query_type(self.ctx.types, body)
                    || self.type_alias_definition_body_is_type_query(&def)
                {
                    let evaluated = self.evaluate_type_with_env(ty);
                    if evaluated != ty && evaluated != TypeId::ERROR {
                        return self
                            .format_type_diagnostic_for_assignability_display_skipping_type_alias(
                                evaluated, def_id,
                            );
                    }
                }
                if self.ctx.definition_store.is_computed_body(body) {
                    let evaluated = self.evaluate_type_with_env(ty);
                    return self.format_type_diagnostic_for_assignability_display(evaluated);
                }
            }
            let evaluated = self.evaluate_type_with_env(ty);
            if evaluated != ty
                && self.ctx.types.get_display_alias(evaluated).is_some()
                && !crate::query_boundaries::recursive_alias::is_def_non_generic_recursive_alias(
                    self.ctx.types.as_type_database(),
                    &self.ctx.definition_store,
                    def_id,
                )
            {
                return self.format_type_for_assignability_message(evaluated);
            }
            let name = self.ctx.types.resolve_atom_ref(def.name);
            return name.to_string();
        }

        if let Some(keyof_alias) = self.ctx.types.get_display_alias(ty)
            && let Some(keyof_inner) =
                crate::query_boundaries::common::keyof_inner_type(self.ctx.types, keyof_alias)
            && let Some(alias_name) = self.lookup_type_alias_name_for_display(keyof_inner)
        {
            return format!("keyof {alias_name}");
        }

        if let Some(keyof_inner) =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, ty)
        {
            if let Some(alias_name) = self.lookup_type_alias_name_for_display(keyof_inner) {
                return format!("keyof {alias_name}");
            }

            if let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, keyof_inner)
                && let Some(sym_id) = shape.symbol
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                return format!("keyof {}", symbol.escaped_name);
            }
        }

        if let Some(alias_name) = self.lookup_type_alias_name_for_display(ty) {
            return alias_name;
        }

        if let Some(collapsed) = self.format_union_with_collapsed_enum_display(ty) {
            return collapsed;
        }

        if let Some(enum_name) = self.format_qualified_enum_name_for_message(ty) {
            return enum_name;
        }

        if ty == TypeId::BOOLEAN_TRUE {
            return "true".to_string();
        }
        if ty == TypeId::BOOLEAN_FALSE {
            return "false".to_string();
        }

        // Alias bodies like `Uppercase<A>` often arrive here before the nested
        // lazy arg has been reduced, even though the fully evaluated surface is
        // a concrete literal or template pattern that tsc prints in TS2322.
        if let Some((kind, type_arg)) =
            crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, ty)
        {
            let resolved_arg =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_arg)
                    .and_then(|def_id| self.ctx.definition_store.get(def_id))
                    .filter(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
                    .and_then(|def| def.body)
                    .map(|body| self.evaluate_type_for_assignability(body))
                    .unwrap_or_else(|| self.evaluate_type_for_assignability(type_arg));
            if resolved_arg != type_arg {
                let remapped = self.ctx.types.string_intrinsic(kind, resolved_arg);
                let evaluated_remapped = self.evaluate_type_for_assignability(remapped);
                if crate::query_boundaries::common::literal_value(
                    self.ctx.types,
                    evaluated_remapped,
                )
                .is_some()
                    || crate::query_boundaries::common::is_template_literal_type(
                        self.ctx.types,
                        evaluated_remapped,
                    )
                    || crate::query_boundaries::common::string_intrinsic_components(
                        self.ctx.types,
                        evaluated_remapped,
                    )
                    .is_some()
                {
                    return self.format_type_for_assignability_message(evaluated_remapped);
                }
            }
        }

        // For deferred conditional types, check if the conditional is ambiguous
        // (tsc shows the branch union rather than the alias form).
        let is_cond = crate::query_boundaries::common::is_conditional_type(self.ctx.types, ty);
        if is_cond && let Some(branch_union) = self.compute_ambiguous_conditional_display(ty) {
            return self.format_type_for_assignability_message(branch_union);
        }

        let evaluated = self.evaluate_type_for_assignability(ty);
        if let Some(display) = self.application_backed_primitive_intersection_display(ty, evaluated)
        {
            return display;
        }
        let use_eval = self.should_use_evaluated_assignability_display(ty, evaluated);
        if use_eval {
            return self.format_type_for_assignability_message(evaluated);
        }

        if let Some((object_type, index_type)) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, ty)
            && let Some(extract_display) = self.format_extract_keyof_string_type(index_type)
        {
            let object_display = self.format_type_for_assignability_message(object_type);
            return format!("{object_display}[{extract_display}]");
        }

        if let Some(extract_display) = self.format_extract_keyof_string_type(ty) {
            return extract_display;
        }

        let display_ty = self.normalize_assignability_display_type(ty);
        if let Some(alias_name) = self.lookup_type_alias_name_for_display(display_ty) {
            return alias_name;
        }

        let application_display =
            crate::query_boundaries::common::type_application(self.ctx.types, display_ty)
                .map(|_| display_ty)
                .or_else(|| {
                    self.ctx
                        .types
                        .get_display_alias(display_ty)
                        .or_else(|| self.ctx.types.get_display_alias(ty))
                        .filter(|&alias| {
                            crate::query_boundaries::common::type_application(self.ctx.types, alias)
                                .is_some()
                        })
                });
        if let Some(application_display) = application_display {
            let normalized =
                self.normalize_property_receiver_application_display_type(application_display);
            if self
                .property_receiver_application_base_name(normalized)
                .is_some_and(|name| name == "merge")
            {
                let mut formatter = self
                    .ctx
                    .create_diagnostic_type_formatter()
                    .with_long_property_receiver_display()
                    .with_display_properties()
                    .with_skip_application_alias_names()
                    .with_long_property_receiver_object_elision_end_depth(0);
                return Self::truncate_property_receiver_display(
                    formatter.format(normalized).into_owned(),
                );
            }
            if normalized != application_display {
                return self.format_type_diagnostic_widened_for_assignability_display(normalized);
            }
        }

        if let Some(display) =
            self.application_backed_primitive_intersection_display(display_ty, display_ty)
        {
            return display;
        }

        if is_generic_callable(self, display_ty)
            && self
                .ctx
                .definition_store
                .find_def_for_type(display_ty)
                .or_else(|| self.ctx.definition_store.find_def_for_type(ty))
                .is_some()
        {
            return format_with_def_store(self, display_ty);
        }
        // For fresh object literal types, format without display properties so
        // widened types are shown: `{ two: number }` not `{ two: 1 }`.
        // Other types (class expressions, interfaces) keep their display properties
        // to preserve named type display (e.g., `typeof A`).
        // Restrict this to actual anonymous object/object-with-index types.
        // Intersections are excluded: tsc's widening behavior in intersection
        // contexts depends on the target type (literal targets preserve literals,
        // non-literal targets widen). This context is not available here.
        let is_anonymous_object_type =
            crate::query_boundaries::dispatch::is_object_like_type(self.ctx.types, display_ty)
                && !crate::query_boundaries::common::is_intersection_type(
                    self.ctx.types,
                    display_ty,
                )
                && crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    display_ty,
                )
                .is_some_and(|shape| shape.symbol.is_none());
        let is_fresh_object_literal =
            self.ctx.types.get_display_properties(display_ty).is_some() && is_anonymous_object_type;
        let mut formatted = if is_fresh_object_literal {
            self.format_type_diagnostic_widened_for_assignability_display(display_ty)
        } else {
            self.format_type_diagnostic_for_assignability_display(display_ty)
        };
        if !formatted.contains('<')
            && let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, display_ty)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let symbol_name = symbol.escaped_name.as_str();
            if formatted == symbol_name {
                // Prefer display_alias from the solver — it preserves the original
                // Application type (e.g. `A<number>`) with correct type arguments.
                let alias_type = self
                    .ctx
                    .types
                    .get_display_alias(display_ty)
                    .or_else(|| self.ctx.types.get_display_alias(ty));
                if let Some(alias) = alias_type {
                    let alias_fmt = self.format_type_diagnostic_for_assignability_display(alias);
                    if alias_fmt.starts_with(symbol_name) && alias_fmt.contains('<') {
                        formatted = alias_fmt;
                    }
                }

                // If display_alias didn't provide type args, try heuristic recovery.
                if !formatted.contains('<') {
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    let type_param_count = if let Some(type_params) =
                        self.ctx.get_def_type_params(def_id)
                    {
                        type_params.len()
                    } else {
                        symbol
                            .declarations
                            .iter()
                            .find_map(|decl| {
                                let node = self.ctx.arena.get(*decl)?;
                                let class = self.ctx.arena.get_class(node)?;
                                Some(class.type_parameters.as_ref().map_or(0, |p| p.nodes.len()))
                            })
                            .unwrap_or(0)
                    };
                    if type_param_count > 0 && shape.properties.len() >= type_param_count {
                        // Recover instantiation args from actual value-carrying members.
                        // For methods, use return type (not the full function signature)
                        // since method return types often directly reflect type params.
                        // E.g. `compareTo(other: T): T` with T=number → return type is `number`.
                        let resolve_candidate_type = |prop: &tsz_solver::PropertyInfo| -> TypeId {
                            if prop.is_method {
                                // For methods, extract a representative type instead of
                                // the full function signature.
                                // Strategy: prefer return type, but if it's trivial
                                // (void/never/any/unknown/undefined), use the first
                                // non-trivial parameter type. This handles both
                                // `compareTo(other: T): T` → return type `number`, and
                                // `foo(a: T): void` → param type `{ a: string }`.
                                let extract_from_shape =
                                    |params: &[tsz_solver::ParamInfo],
                                     return_type: TypeId|
                                     -> TypeId {
                                        let is_trivial = matches!(
                                            return_type,
                                            TypeId::VOID
                                                | TypeId::NEVER
                                                | TypeId::ANY
                                                | TypeId::UNKNOWN
                                                | TypeId::UNDEFINED
                                                | TypeId::NULL
                                        );
                                        if !is_trivial {
                                            return return_type;
                                        }
                                        // Return type is trivial — use first substantive param
                                        for param in params {
                                            if !matches!(
                                                param.type_id,
                                                TypeId::VOID
                                                    | TypeId::NEVER
                                                    | TypeId::ANY
                                                    | TypeId::UNKNOWN
                                                    | TypeId::UNDEFINED
                                                    | TypeId::NULL
                                            ) {
                                                return param.type_id;
                                            }
                                        }
                                        return_type
                                    };
                                if let Some(fn_shape) =
                                    crate::query_boundaries::common::function_shape_for_type(
                                        self.ctx.types,
                                        prop.type_id,
                                    )
                                {
                                    return extract_from_shape(
                                        &fn_shape.params,
                                        fn_shape.return_type,
                                    );
                                }
                                if let Some(callable) =
                                    crate::query_boundaries::common::callable_shape_for_type(
                                        self.ctx.types,
                                        prop.type_id,
                                    )
                                    && callable.call_signatures.len() == 1
                                {
                                    let sig = &callable.call_signatures[0];
                                    return extract_from_shape(&sig.params, sig.return_type);
                                }
                            }
                            prop.type_id
                        };
                        let build_candidates =
                            |predicate: fn(&tsz_solver::PropertyInfo) -> bool,
                             types: &dyn tsz_solver::TypeDatabase| {
                                let mut candidates: Vec<(String, TypeId)> = shape
                                    .properties
                                    .iter()
                                    .filter(|prop| predicate(prop))
                                    .filter_map(|prop| {
                                        let name = types.resolve_atom_ref(prop.name).to_string();
                                        if tsz_solver::utils::is_synthetic_private_brand_name(&name)
                                        {
                                            None
                                        } else {
                                            Some((name, resolve_candidate_type(prop)))
                                        }
                                    })
                                    .collect();
                                candidates.sort_by(|a, b| a.0.cmp(&b.0));
                                candidates
                            };
                        let mut candidates = build_candidates(
                            |prop| !prop.is_method && !prop.is_class_prototype,
                            self.ctx.types.as_type_database(),
                        );
                        if candidates.len() < type_param_count {
                            candidates = build_candidates(
                                |prop| !prop.is_method,
                                self.ctx.types.as_type_database(),
                            );
                        }
                        if candidates.len() < type_param_count {
                            candidates = build_candidates(
                                |prop| !prop.is_class_prototype,
                                self.ctx.types.as_type_database(),
                            );
                        }
                        if candidates.len() < type_param_count {
                            candidates =
                                build_candidates(|_| true, self.ctx.types.as_type_database());
                        }
                        let args: Vec<String> = candidates
                            .iter()
                            .take(type_param_count)
                            .map(|(_, type_id)| {
                                self.format_type_diagnostic_for_assignability_display(*type_id)
                            })
                            .collect();
                        if args.len() == type_param_count {
                            formatted = format!("{}<{}>", symbol_name, args.join(", "));
                        }
                    }
                }
            }
        }

        // tsc commonly formats object type literals with a trailing semicolon before `}`.
        if formatted.starts_with("{ ")
            && formatted.ends_with(" }")
            && formatted.contains(':')
            && !formatted.ends_with("; }")
        {
            formatted = format!("{}; }}", &formatted[..formatted.len() - 2]);
        }
        formatted = self.normalize_assignability_union_display_order(formatted);
        // tsc renders `Array<T>` / `ReadonlyArray<T>` as `T[]` / `readonly T[]`
        // in assignability messages; mirror that at the boundary so callers
        // that bypass the annotation-text path still pick it up.
        formatted = Self::normalize_array_generic_to_shorthand(&formatted);
        self.normalize_template_placeholder_spacing_for_display(&formatted)
    }

    pub(crate) fn authoritative_assignability_def_name(&mut self, ty: TypeId) -> Option<String> {
        let has_generic_callable_surface = |state: &Self, candidate: TypeId| {
            crate::query_boundaries::common::callable_shape_for_type(state.ctx.types, candidate)
                .is_some_and(|shape| {
                    shape
                        .call_signatures
                        .iter()
                        .chain(shape.construct_signatures.iter())
                        .any(|sig| !sig.type_params.is_empty())
                })
                || crate::query_boundaries::common::function_shape_for_type(
                    state.ctx.types,
                    candidate,
                )
                .is_some_and(|shape| !shape.type_params.is_empty())
        };
        let direct_def_name = |state: &Self, candidate: TypeId| {
            let def_id = crate::query_boundaries::common::lazy_def_id(
                state.ctx.types.as_type_database(),
                candidate,
            )
            .or_else(|| state.ctx.definition_store.find_def_for_type(candidate))?;
            let def = state.ctx.definition_store.get(def_id)?;
            if def.kind == tsz_solver::def::DefKind::TypeAlias
                && (def.body.is_some_and(|body| {
                    state.assignability_display_has_own_signature_type_params(body)
                }) || state.assignability_display_has_own_signature_type_params(candidate))
            {
                return None;
            }
            let name = state.ctx.types.resolve_atom_ref(def.name).to_string();
            // Class constructor, enum, and namespace defs represent the static/value
            // side and should display as "typeof Name" to match tsc.
            if matches!(
                def.kind,
                tsz_solver::def::DefKind::ClassConstructor
                    | tsz_solver::def::DefKind::Enum
                    | tsz_solver::def::DefKind::Namespace
            ) {
                Some(format!("typeof {name}"))
            } else {
                Some(name)
            }
        };

        let symbol_backed_name = |state: &Self, candidate: TypeId| {
            if state.assignability_display_has_own_signature_type_params(candidate) {
                return None;
            }
            let symbol_name =
                crate::query_boundaries::common::object_shape_for_type(state.ctx.types, candidate)
                    .and_then(|shape| shape.symbol)
                    .or_else(|| {
                        crate::query_boundaries::common::callable_shape_for_type(
                            state.ctx.types,
                            candidate,
                        )
                        .and_then(|shape| shape.symbol)
                    })
                    .and_then(|sym_id| state.ctx.binder.get_symbol(sym_id))
                    .map(|symbol| symbol.escaped_name.clone())?;
            Some(symbol_name)
        };

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, ty)
        {
            let mut named_members = Vec::new();
            let mut saw_namespace_member = false;

            for member in members {
                if crate::query_boundaries::common::is_module_namespace_type(self.ctx.types, member)
                    || crate::query_boundaries::common::is_type_query_type(self.ctx.types, member)
                    || self.ctx.namespace_module_names.contains_key(&member)
                {
                    saw_namespace_member = true;
                    continue;
                }

                if let Some(name) =
                    direct_def_name(self, member).or_else(|| symbol_backed_name(self, member))
                {
                    named_members.push(name);
                }
            }

            named_members.sort();
            named_members.dedup();
            if saw_namespace_member && named_members.len() == 1 {
                return named_members.into_iter().next();
            }
        }

        let export_equals_default_name = |state: &mut Self, candidate: TypeId| {
            let default_name = state.ctx.types.intern_string("default");
            let shape =
                crate::query_boundaries::common::object_shape_for_type(state.ctx.types, candidate)?;
            let default_prop = shape
                .properties
                .iter()
                .find(|prop| prop.name == default_name)?;
            let default_ty = default_prop.type_id;

            let wrapper_method_mentions_default = shape.properties.iter().any(|prop| {
                let Some(return_ty) = crate::query_boundaries::common::return_type_for_type(
                    state.ctx.types,
                    prop.type_id,
                ) else {
                    return false;
                };
                let Some(return_members) = crate::query_boundaries::common::intersection_members(
                    state.ctx.types,
                    return_ty,
                ) else {
                    return false;
                };
                let has_default_member = return_members.iter().copied().any(|member| {
                    member == default_ty
                        || direct_def_name(state, member) == direct_def_name(state, default_ty)
                        || symbol_backed_name(state, member)
                            == symbol_backed_name(state, default_ty)
                });
                let has_namespace_member = return_members.iter().copied().any(|member| {
                    crate::query_boundaries::common::is_module_namespace_type(
                        state.ctx.types,
                        member,
                    ) || crate::query_boundaries::common::is_type_query_type(
                        state.ctx.types,
                        member,
                    ) || state.ctx.namespace_module_names.contains_key(&member)
                });
                has_default_member && has_namespace_member
            });

            if !wrapper_method_mentions_default {
                return None;
            }

            direct_def_name(state, default_ty).or_else(|| symbol_backed_name(state, default_ty))
        };

        if let Some(name) = export_equals_default_name(self, ty) {
            return Some(name);
        }

        let display_ty = self.normalize_assignability_display_type(ty);
        if has_generic_callable_surface(self, ty) || has_generic_callable_surface(self, display_ty)
        {
            return None;
        }
        if let Some(name) = export_equals_default_name(self, display_ty) {
            return Some(name);
        }
        // Raw Tuple types must not be resolved to a type alias name via find_def_for_type.
        // A literal tuple declaration like `let x: [number, string]` interns to the same
        // TypeId as `type T = [number, string]`, which would cause x's error messages to
        // show "T" instead of the structural form. Only Lazy(DefId) references (which
        // arise from explicit alias usage) correctly produce alias names.
        let ty_is_raw_tuple = crate::query_boundaries::common::is_tuple_type(self.ctx.types, ty);
        let def_id =
            crate::query_boundaries::common::lazy_def_id(self.ctx.types.as_type_database(), ty)
                .or_else(|| {
                    if ty_is_raw_tuple {
                        None
                    } else {
                        self.ctx.definition_store.find_def_for_type(ty)
                    }
                })
                .or_else(|| {
                    if ty_is_raw_tuple {
                        None
                    } else {
                        self.ctx.definition_store.find_def_for_type(display_ty)
                    }
                })
                .or_else(|| {
                    if ty_is_raw_tuple {
                        return None;
                    }
                    let evaluated = self.evaluate_type_for_assignability(ty);
                    self.ctx.definition_store.find_def_for_type(evaluated)
                })?;
        let def = self.ctx.definition_store.get(def_id)?;
        // Type aliases register their body TypeId in `find_def_for_type`. For
        // an alias whose body is a generic `Application`, the body TypeId is
        // interned and is shared with any direct write of the same application
        // form (e.g., `let a: T<A>` and `type C = T<A>` both produce
        // `Application(T, [A])`). When `ty` itself arrives here as an
        // `Application` — i.e., the user wrote the application form — using
        // the alias's name would surface an unrelated sibling alias in the
        // diagnostic. Preserve the application form by returning None so the
        // upstream formatter renders `T<A>` rather than `C`.
        if def.kind == tsz_solver::def::DefKind::TypeAlias
            && crate::query_boundaries::common::is_generic_application(
                self.ctx.types.as_type_database(),
                ty,
            )
        {
            return None;
        }
        if def.kind == tsz_solver::def::DefKind::TypeAlias
            && (def
                .body
                .is_some_and(|body| self.assignability_display_has_own_signature_type_params(body))
                || self.assignability_display_has_own_signature_type_params(ty)
                || self.assignability_display_has_own_signature_type_params(display_ty))
        {
            return None;
        }
        let name = self.ctx.types.resolve_atom_ref(def.name).to_string();
        if matches!(
            def.kind,
            tsz_solver::def::DefKind::ClassConstructor
                | tsz_solver::def::DefKind::Enum
                | tsz_solver::def::DefKind::Namespace
        ) {
            Some(format!("typeof {name}"))
        } else {
            Some(name)
        }
    }

    pub(crate) fn format_assignability_type_for_message(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> String {
        self.format_assignability_type_for_message_internal(ty, other, true)
    }

    pub(crate) fn format_assignability_type_for_message_preserving_nullish(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> String {
        self.format_assignability_type_for_message_internal(ty, other, false)
    }

    pub(crate) fn finalize_pair_display_for_diagnostic(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_display: String,
        target_display: String,
    ) -> (String, String) {
        if source == target {
            return (source_display, target_display);
        }

        let Some(source_name) = Self::bare_nominal_display_name(&source_display) else {
            return (source_display, target_display);
        };
        let Some(target_name) = Self::bare_nominal_display_name(&target_display) else {
            return (source_display, target_display);
        };
        if source_name != target_name {
            return (source_display, target_display);
        }

        let (pair_source, pair_target) = self.format_type_pair_diagnostic(source, target);
        if pair_source == pair_target
            || (pair_source == source_display && pair_target == target_display)
        {
            let source_candidate = self.format_assignability_type_for_message(source, target);
            let target_candidate = self.format_assignability_type_for_message(target, source);
            if source_candidate == target_candidate
                || (source_candidate == source_display && target_candidate == target_display)
            {
                return (source_display, target_display);
            }
            return (source_candidate, target_candidate);
        }

        // Enum-member → enum-type widening: upstream produces `W` while the
        // disambiguator regenerates `W.a`.  When the upstream `source_display`
        // is exactly the dotted *parent* of `pair_source` (i.e. `pair_source`
        // is `<source_display>.<member>`), the disambiguator is undoing
        // upstream's deliberate widening.  Cross-package symlink
        // disambiguation is unaffected because there `pair_source` matches
        // `source_display` (no parent-of relationship triggered).
        let pair_source_parent = pair_source
            .rsplit_once('.')
            .map(|(parent, _)| parent.trim_end());
        if pair_source_parent == Some(source_display.as_str()) && source_display != target_display {
            return (source_display, target_display);
        }

        (pair_source, pair_target)
    }

    fn bare_nominal_display_name(display: &str) -> Option<&str> {
        let mut text = display.trim();
        if let Some(rest) = text.strip_prefix("typeof ") {
            text = rest.trim();
        }

        if text.is_empty()
            || text.starts_with('{')
            || text.starts_with('[')
            || text.starts_with('"')
            || text.starts_with('\'')
            || text.contains("=>")
            || text.contains(" | ")
            || text.contains(" & ")
        {
            return None;
        }

        let head = text.split_once('<').map(|(head, _)| head).unwrap_or(text);
        let name = head.rsplit_once('.').map(|(_, name)| name).unwrap_or(head);
        let mut chars = name.chars();
        let first = chars.next()?;
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return None;
        }
        if !chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()) {
            return None;
        }

        match name {
            "any" | "unknown" | "never" | "string" | "number" | "boolean" | "symbol" | "bigint"
            | "void" | "undefined" | "null" | "object" => None,
            _ => Some(name),
        }
    }

    fn format_assignability_type_for_message_internal(
        &mut self,
        ty: TypeId,
        other: TypeId,
        strip_top_level_nullish: bool,
    ) -> String {
        if let Some(collapsed) =
            self.collapsed_anonymous_object_intersection_for_assignability_display(ty)
        {
            return self.format_collapsed_object_for_assignability_display(collapsed);
        }

        if self.target_preserves_literal_surface(other) {
            return self.format_type_diagnostic_for_assignability_display(ty);
        }
        if let Some(enum_name) = self.format_disambiguated_enum_name_for_assignment(ty, other) {
            return enum_name;
        }
        if crate::query_boundaries::common::literal_value(self.ctx.types, ty).is_some()
            && crate::query_boundaries::common::string_intrinsic_components(self.ctx.types, other)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(ty);
            return self.format_type_for_assignability_message(widened);
        }
        if let Some(display) = self.constrained_variadic_tuple_parameter_display(ty, other) {
            return display;
        }
        if let Some(type_name) = self.format_class_constructor_name_for_assignment(ty, other) {
            return type_name;
        }
        if let Some(type_name) = self.format_disambiguated_nominal_name_for_assignment(ty, other) {
            return type_name;
        }

        // When displaying the TARGET type and the SOURCE is non-nullable,
        // strip null/undefined from the top-level union to match tsc's behavior.
        // tsc only shows the non-nullable part of the target since null/undefined
        // are not relevant to the structural mismatch.
        if strip_top_level_nullish
            && let Some(stripped) = self.strip_nullish_for_assignability_display(ty, other)
        {
            return self.format_type_for_assignability_message(stripped);
        }

        // For intersection types containing a fresh anonymous object member,
        // use widened display when the target is NOT literal-sensitive.
        // tsc widens `{ fooProp: "frizzlebizzle" } & Bar` to
        // `{ fooProp: string } & Bar` when the target has non-literal property
        // types, but preserves the literal when the target has literal types.
        if crate::query_boundaries::common::is_intersection_type(
            self.ctx.types.as_type_database(),
            ty,
        ) && !self.is_literal_sensitive_assignment_target(other)
            && self.intersection_has_fresh_anonymous_object(ty)
        {
            return self.format_type_diagnostic_widened_for_assignability_display(ty);
        }

        self.format_type_for_assignability_message(ty)
    }

    /// Check if an intersection type contains a fresh anonymous object member
    /// (one with `display_properties` and no symbol name).
    fn intersection_has_fresh_anonymous_object(&self, ty: TypeId) -> bool {
        crate::query_boundaries::common::intersection_members(self.ctx.types.as_type_database(), ty)
            .is_some_and(|members| {
                members.iter().any(|&m| {
                    self.ctx.types.get_display_properties(m).is_some()
                        && crate::query_boundaries::common::object_shape_for_type(self.ctx.types, m)
                            .is_some_and(|shape| shape.symbol.is_none())
                })
            })
    }

    fn class_constructor_symbol_for_assignment_display(
        &mut self,
        ty: TypeId,
    ) -> Option<tsz_binder::SymbolId> {
        let display_ty = self.normalize_assignability_display_type(ty);
        let evaluated = self.evaluate_type_for_assignability(ty);
        [ty, display_ty, evaluated]
            .into_iter()
            .find_map(|candidate| {
                let sym_id =
                    crate::query_boundaries::common::type_shape_symbol(self.ctx.types, candidate)
                        .or_else(|| {
                            crate::query_boundaries::common::object_shape_for_type(
                                self.ctx.types,
                                candidate,
                            )
                            .and_then(|shape| shape.symbol)
                        })
                        .or_else(|| {
                            crate::query_boundaries::common::callable_shape_for_type(
                                self.ctx.types,
                                candidate,
                            )
                            .and_then(|shape| shape.symbol)
                        })?;
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                let is_class_symbol = symbol.has_any_flags(tsz_binder::symbol_flags::CLASS);
                let is_value_type = crate::query_boundaries::common::function_shape_for_type(
                    self.ctx.types,
                    candidate,
                )
                .is_some()
                    || crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        candidate,
                    )
                    .is_some();
                (is_class_symbol && is_value_type).then_some(sym_id)
            })
    }

    fn format_class_constructor_name_for_assignment(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<String> {
        let ty_sym = self.class_constructor_symbol_for_assignment_display(ty)?;
        let other_sym = self.class_constructor_symbol_for_assignment_display(other);
        let ty_name = self.qualified_symbol_name_for_message(ty_sym)?;

        if let Some(other_sym) = other_sym
            && other_sym != ty_sym
            && self.ctx.binder.get_symbol(other_sym)?.escaped_name
                == self.ctx.binder.get_symbol(ty_sym)?.escaped_name
            && self.is_exported_external_module_symbol(ty_sym)
            && let Some(module_name) = self.module_specifier_for_symbol(ty_sym)
        {
            return Some(format!("typeof import(\"{module_name}\").{ty_name}"));
        }

        Some(format!("typeof {ty_name}"))
    }

    /// When `ty` is a union containing null/undefined and `other` (the
    /// counterpart in the assignability check) is non-nullable, strip the
    /// top-level null/undefined members from `ty`.  This matches tsc which
    /// shows only the non-nullable part of the target to reduce noise.
    pub(crate) fn strip_nullish_for_assignability_display(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<TypeId> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, ty)?;
        // Only strip when the union has null or undefined members
        let has_null = members.contains(&TypeId::NULL);
        let has_undefined = members.contains(&TypeId::UNDEFINED);
        if !has_null && !has_undefined {
            return None;
        }
        // Only strip when the OTHER type is non-nullable (not a union with null/undefined)
        if other == TypeId::NULL || other == TypeId::UNDEFINED {
            return None;
        }
        if let Some(other_members) =
            crate::query_boundaries::common::union_members(self.ctx.types, other)
            && other_members
                .iter()
                .any(|&m| m == TypeId::NULL || m == TypeId::UNDEFINED)
        {
            return None;
        }
        // When `other` is a generic type (type parameter or intersection of type
        // parameters), reduce it to its base constraint and check if that
        // contains null/undefined.  tsc preserves the full target union when
        // the source's base constraint is nullable.  Example:
        //   source `T & U` where constraints are `string | ... | undefined`
        //   target `string | null` must stay `string | null` (not `string`).
        let other_base = crate::query_boundaries::common::get_base_constraint_for_display(
            self.ctx.types.as_type_database(),
            other,
        );
        if other_base != other
            && let Some(other_base_members) =
                crate::query_boundaries::common::union_members(self.ctx.types, other_base)
            && other_base_members
                .iter()
                .any(|&m| m == TypeId::NULL || m == TypeId::UNDEFINED)
        {
            return None;
        }
        // Also handle direct TypeId::NULL/UNDEFINED in the reduced base (e.g.,
        // T extends undefined reduces to `undefined`).
        if other_base == TypeId::NULL || other_base == TypeId::UNDEFINED {
            return None;
        }
        let filtered: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&m| m != TypeId::NULL && m != TypeId::UNDEFINED)
            .collect();
        if filtered.is_empty() || filtered.len() == members.len() {
            return None;
        }
        if filtered.len() == 1 {
            return Some(filtered[0]);
        }
        Some(self.ctx.types.factory().union(filtered))
    }

    pub(crate) fn should_strip_nullish_for_property_display(&self, target: TypeId) -> bool {
        crate::query_boundaries::common::union_members(self.ctx.types, target).is_some()
            || crate::query_boundaries::common::intersection_members(self.ctx.types, target)
                .is_some()
    }

    pub(super) fn format_enum_member_name_for_message(&mut self, ty: TypeId) -> Option<String> {
        let def_id = crate::query_boundaries::common::enum_def_id(self.ctx.types, ty)?;
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER) {
            return None;
        }
        self.format_qualified_enum_name_for_message(ty)
    }

    pub(super) fn format_qualified_enum_name_for_message(&mut self, ty: TypeId) -> Option<String> {
        let def_id = crate::query_boundaries::common::enum_def_id(self.ctx.types, ty)?;
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER) {
            let parent = self.ctx.binder.get_symbol(symbol.parent)?;
            return Some(format!("{}.{}", parent.escaped_name, symbol.escaped_name));
        }
        let mut parts = vec![symbol.escaped_name.clone()];
        let decl_idx = symbol.primary_declaration()?;
        let mut current = self.ctx.arena.get_extended(decl_idx)?.parent;

        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module_decl) = self.ctx.arena.get_module(node)
                && let Some(name) = self.ctx.arena.get_identifier_text(module_decl.name)
            {
                parts.push(name.to_string());
            }

            current = self.ctx.arena.get_extended(current)?.parent;
        }

        if parts.len() == 1 {
            let mut current = symbol.parent;
            while current != tsz_binder::SymbolId::NONE {
                let parent = self.ctx.binder.get_symbol(current)?;
                if !parent.has_any_flags(
                    tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE
                        | tsz_binder::symbol_flags::ENUM,
                ) {
                    break;
                }
                parts.push(parent.escaped_name.clone());
                current = parent.parent;
            }
        }

        parts.reverse();
        Some(parts.join("."))
    }

    fn format_disambiguated_enum_name_for_assignment(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<String> {
        let ty_sym = self.enum_symbol_from_enumish_type(ty)?;
        let other_sym = self.enum_symbol_from_enumish_type(other)?;
        if ty_sym == other_sym {
            return None;
        }

        let ty_symbol = self.ctx.binder.get_symbol(ty_sym)?;
        let other_symbol = self.ctx.binder.get_symbol(other_sym)?;

        if crate::query_boundaries::common::enum_def_id(self.ctx.types, ty)
            .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER))
        {
            return self.format_qualified_enum_name_for_message(ty);
        }

        if ty_symbol.escaped_name != other_symbol.escaped_name {
            return Some(ty_symbol.escaped_name.clone());
        }

        if self.is_exported_external_module_enum_symbol(ty_sym)
            && let Some(module_name) = self.module_specifier_for_symbol(ty_sym)
        {
            return Some(format!(
                "import(\"{module_name}\").{}",
                ty_symbol.escaped_name
            ));
        }

        self.format_qualified_enum_name_for_message(ty)
    }

    fn format_disambiguated_nominal_name_for_assignment(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<String> {
        let ty_sym = self.nominal_shape_symbol_for_display(ty)?;
        let other_sym = self.nominal_shape_symbol_for_display(other)?;
        if ty_sym == other_sym {
            return None;
        }
        let ty_symbol = self.ctx.binder.get_symbol(ty_sym)?;
        let other_symbol = self.ctx.binder.get_symbol(other_sym)?;
        if ty_symbol.escaped_name != other_symbol.escaped_name {
            return None;
        }
        if self.is_exported_external_module_symbol(ty_sym)
            && let Some(module_name) = self.module_specifier_for_symbol(ty_sym)
        {
            return Some(format!(
                "import(\"{module_name}\").{}",
                ty_symbol.escaped_name
            ));
        }
        let qualified = self.qualified_symbol_name_for_message(ty_sym)?;
        if qualified == ty_symbol.escaped_name {
            return None;
        }
        Some(qualified)
    }

    fn nominal_shape_symbol_for_display(&mut self, ty: TypeId) -> Option<tsz_binder::SymbolId> {
        let resolved = self.evaluate_type_for_assignability(ty);
        [ty, resolved].into_iter().find_map(|candidate| {
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, candidate).or_else(
                || {
                    let def_id =
                        crate::query_boundaries::common::lazy_def_id(self.ctx.types, candidate)?;
                    self.ctx.def_to_symbol_id_with_fallback(def_id)
                },
            )
        })
    }

    fn qualified_symbol_name_for_message(&self, sym_id: tsz_binder::SymbolId) -> Option<String> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut parts = vec![symbol.escaped_name.clone()];
        let mut current = symbol.parent;
        while current != tsz_binder::SymbolId::NONE {
            let parent = self.ctx.binder.get_symbol(current)?;
            if !parent.has_any_flags(
                tsz_binder::symbol_flags::NAMESPACE_MODULE
                    | tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::ENUM,
            ) {
                break;
            }
            parts.push(parent.escaped_name.clone());
            current = parent.parent;
        }
        parts.reverse();
        Some(parts.join("."))
    }

    fn is_exported_external_module_enum_symbol(&self, sym_id: tsz_binder::SymbolId) -> bool {
        self.is_exported_external_module_symbol(sym_id)
    }

    fn is_exported_external_module_symbol(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.is_exported
            && symbol.decl_file_idx != u32::MAX
            && self
                .ctx
                .get_binder_for_file(symbol.decl_file_idx as usize)
                .is_some_and(tsz_binder::BinderState::is_external_module)
    }

    fn module_specifier_for_symbol(&self, sym_id: tsz_binder::SymbolId) -> Option<String> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if let Some(specifier) = self.ctx.module_specifiers.get(&symbol.decl_file_idx) {
            return Some(specifier.clone());
        }

        let arena = self.ctx.get_arena_for_file(symbol.decl_file_idx);
        let source_file = arena.source_files.first()?;
        let file_name = &source_file.file_name;
        let stem = file_name
            .rsplit_once('.')
            .map(|(base, _)| base)
            .unwrap_or(file_name);
        let basename = stem.rsplit_once('/').map(|(_, name)| name).unwrap_or(stem);
        Some(basename.to_string())
    }

    fn is_function_like_type(&mut self, ty: TypeId) -> bool {
        let resolved = self.resolve_type_for_property_access(ty);
        let evaluated = self.judge_evaluate(resolved);
        [ty, resolved, evaluated].into_iter().any(|candidate| {
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, candidate)
                .is_some()
                || crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    candidate,
                )
                .is_some_and(|s| !s.call_signatures.is_empty())
                || candidate == TypeId::FUNCTION
        })
    }

    /// Find a string literal spelling suggestion for TS2820.
    /// Returns the suggested literal string if the source is a string literal
    /// close to one of the target's string literal members.
    pub(super) fn find_string_literal_spelling_suggestion(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        // Source must be a string literal
        let source_str =
            match crate::query_boundaries::common::literal_value(self.ctx.types, source) {
                Some(tsz_solver::LiteralValue::String(atom)) => self.ctx.types.resolve_atom(atom),
                _ => return None,
            };

        // Collect target string literal members
        let target_literals: Vec<String> = if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target)
        {
            members
                .iter()
                .filter_map(|&m| {
                    match crate::query_boundaries::common::literal_value(self.ctx.types, m) {
                        Some(tsz_solver::LiteralValue::String(atom)) => {
                            Some(self.ctx.types.resolve_atom(atom))
                        }
                        _ => None,
                    }
                })
                .collect()
        } else if let Some(tsz_solver::LiteralValue::String(atom)) =
            crate::query_boundaries::common::literal_value(self.ctx.types, target)
        {
            vec![self.ctx.types.resolve_atom(atom)]
        } else {
            vec![]
        };

        // Use tsc's getSpellingSuggestion algorithm with weighted Levenshtein.
        // tsc uses substitution cost 2.0 (0.1 for case-only diffs), which means
        // short strings like "baz" vs "bar" won't trigger a suggestion.
        let name_len = source_str.chars().count();
        let maximum_length_difference = 2usize.max((name_len as f64 * 0.34).floor() as usize);
        let mut best_distance = (name_len as f64 * 0.4).floor() + 1.0;
        let mut best_candidate: Option<String> = None;

        for candidate in &target_literals {
            if candidate == &source_str {
                continue;
            }
            let candidate_len = candidate.chars().count();
            let len_diff = candidate_len.abs_diff(name_len);
            if len_diff > maximum_length_difference {
                continue;
            }
            // Skip short candidates unless they match by case
            if candidate_len < 3 && candidate.to_lowercase() != source_str.to_lowercase() {
                continue;
            }
            if let Some(distance) =
                Self::levenshtein_with_max(&source_str, candidate, best_distance - 0.1)
            {
                best_distance = distance;
                best_candidate = Some(candidate.clone());
            }
        }

        // TSC wraps the suggestion in double quotes (it's a string literal type name)
        best_candidate.map(|s| format!("\"{s}\""))
    }

    pub(in crate::error_reporter) fn format_ts2820_target_display(
        &mut self,
        target: TypeId,
        evaluated_target: TypeId,
        target_str: &str,
    ) -> String {
        let expanded_target_str = self.format_type_diagnostic(evaluated_target);
        if expanded_target_str == target_str {
            return target_str.to_string();
        }

        if self.ts2820_target_contains_application_surface(target)
            || self.ts2820_target_contains_alias_surface(target)
        {
            Self::widen_numeric_member_literals_in_display_text(target_str)
        } else {
            expanded_target_str
        }
    }

    fn widen_numeric_member_literals_in_display_text(display: &str) -> String {
        let bytes = display.as_bytes();
        let mut out = String::with_capacity(display.len());
        let mut i = 0usize;
        let is_boundary = |b: u8| {
            matches!(
                b,
                b';' | b',' | b'}' | b'>' | b')' | b'|' | b'&' | b']' | b' '
            )
        };
        while i < bytes.len() {
            if i + 2 < bytes.len() && bytes[i] == b':' && bytes[i + 1] == b' ' {
                out.push(':');
                out.push(' ');
                i += 2;

                let mut j = i;
                if j < bytes.len() && bytes[j] == b'-' {
                    j += 1;
                }
                let mut saw_digit = false;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                    saw_digit = true;
                }
                if j < bytes.len() && bytes[j] == b'.' {
                    j += 1;
                    while j < bytes.len() && bytes[j].is_ascii_digit() {
                        j += 1;
                        saw_digit = true;
                    }
                }
                if saw_digit && (j >= bytes.len() || is_boundary(bytes[j])) {
                    out.push_str("number");
                    i = j;
                    continue;
                }
            }

            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    fn ts2820_target_contains_alias_surface(&self, target: TypeId) -> bool {
        if self.ctx.types.get_display_alias(target).is_some()
            || self.lookup_type_alias_name_for_display(target).is_some()
        {
            return true;
        }

        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target).or_else(|| {
                crate::query_boundaries::common::intersection_members(self.ctx.types, target)
            })
        {
            return members
                .iter()
                .any(|&member| self.ts2820_target_contains_alias_surface(member));
        }

        false
    }

    fn ts2820_target_contains_application_surface(&self, target: TypeId) -> bool {
        if self.ts2820_is_named_application_surface(target) {
            return true;
        }

        self.ctx
            .types
            .get_display_alias(target)
            .is_some_and(|alias| self.ts2820_is_named_application_surface(alias))
    }

    fn ts2820_is_named_application_surface(&self, target: TypeId) -> bool {
        let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, target)
        else {
            return false;
        };
        !args.is_empty() && self.ts2820_application_base_has_named_surface(base)
    }

    fn ts2820_application_base_has_named_surface(&self, base: TypeId) -> bool {
        crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))
            .is_some()
            || self.ctx.types.get_display_alias(base).is_some()
            || self.lookup_type_alias_name_for_display(base).is_some()
    }

    pub(super) fn first_nonpublic_constructor_param_property(
        &mut self,
        ty: TypeId,
    ) -> Option<(String, MemberAccessLevel)> {
        let resolved = self.resolve_type_for_property_access(ty);
        let evaluated = self.judge_evaluate(resolved);
        let candidates = [ty, resolved, evaluated];

        let mut symbol_candidates: Vec<tsz_binder::SymbolId> = Vec::new();
        if let Some(sym) = candidates.into_iter().find_map(|candidate| {
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, candidate)
        }) {
            symbol_candidates.push(sym);
        }
        let ty_name = self.format_type_for_assignability_message(ty);
        let bare = ty_name.split('<').next().unwrap_or(&ty_name);
        let simple = bare.rsplit('.').next().unwrap_or(bare).trim();
        if !simple.is_empty() && !simple.starts_with('{') && !simple.contains(' ') {
            for &sym in self.ctx.binder.get_symbols().find_all_by_name(simple) {
                if !symbol_candidates.contains(&sym) {
                    symbol_candidates.push(sym);
                }
            }
        }
        if symbol_candidates.is_empty() {
            return None;
        }

        for symbol_id in symbol_candidates {
            let Some(symbol) = self.ctx.binder.get_symbol(symbol_id) else {
                continue;
            };
            for &decl_idx in &symbol.declarations {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION
                    && decl_node.kind != syntax_kind_ext::CLASS_EXPRESSION
                {
                    continue;
                }
                let Some(class) = self.ctx.arena.get_class(decl_node) else {
                    continue;
                };
                for &member_idx in &class.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                        continue;
                    }
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        let Some(level) = self.member_access_level_from_modifiers(&param.modifiers)
                        else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        return Some((name, level));
                    }
                }
            }
        }

        None
    }

    pub(super) fn missing_single_required_property(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<tsz_common::interner::Atom> {
        if crate::query_boundaries::common::is_primitive_type(self.ctx.types, source) {
            return None;
        }

        let source_candidates = {
            let resolved = self.resolve_type_for_property_access(source);
            let evaluated = self.judge_evaluate(resolved);
            [source, resolved, evaluated]
        };
        let target_candidates = {
            let resolved = self.resolve_type_for_property_access(target);
            let evaluated = self.judge_evaluate(resolved);
            [target, resolved, evaluated]
        };

        let source_is_function_like = self.is_function_like_type(source);

        let target_name = self.format_type_for_assignability_message(target);
        if target_name == "Callable" || target_name == "Applicable" {
            let required_name = if target_name == "Callable" {
                "call"
            } else {
                "apply"
            };
            let required_atom = self.ctx.types.intern_string(required_name);
            let source_has_prop = if source_is_function_like {
                true
            } else {
                source_candidates.iter().any(|candidate| {
                    if let Some(source_callable) =
                        crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            *candidate,
                        )
                    {
                        source_callable
                            .properties
                            .iter()
                            .any(|p| p.name == required_atom)
                    } else if let Some(source_shape) =
                        crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            *candidate,
                        )
                    {
                        source_shape
                            .properties
                            .iter()
                            .any(|p| p.name == required_atom)
                    } else {
                        false
                    }
                })
            };
            if !source_has_prop {
                return Some(required_atom);
            }
        }

        if !source_is_function_like {
            for target_candidate in target_candidates {
                let Some(target_callable) =
                    crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        target_candidate,
                    )
                else {
                    continue;
                };
                let Some(sym_id) = target_callable.symbol else {
                    continue;
                };
                let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                    continue;
                };
                if symbol.escaped_name == "Callable" {
                    return Some(self.ctx.types.intern_string("call"));
                }
                if symbol.escaped_name == "Applicable" {
                    return Some(self.ctx.types.intern_string("apply"));
                }
            }
        }

        for target_candidate in target_candidates {
            if let Some(target_callable) = crate::query_boundaries::common::callable_shape_for_type(
                self.ctx.types,
                target_candidate,
            ) {
                let required_props: Vec<_> = target_callable
                    .properties
                    .iter()
                    .filter(|p| !p.optional)
                    .collect();
                if required_props.len() == 1 {
                    let prop = required_props[0];
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "call" || prop_name.as_ref() == "apply" {
                        let source_has_prop = if source_is_function_like {
                            true
                        } else {
                            source_candidates.iter().any(|candidate| {
                                if let Some(source_callable) =
                                    crate::query_boundaries::common::callable_shape_for_type(
                                        self.ctx.types,
                                        *candidate,
                                    )
                                {
                                    source_callable
                                        .properties
                                        .iter()
                                        .any(|p| p.name == prop.name)
                                } else if let Some(source_shape) =
                                    crate::query_boundaries::common::object_shape_for_type(
                                        self.ctx.types,
                                        *candidate,
                                    )
                                {
                                    source_shape.properties.iter().any(|p| p.name == prop.name)
                                } else {
                                    false
                                }
                            })
                        };
                        if !source_has_prop {
                            return Some(prop.name);
                        }
                    }
                }
            }
        }

        let source_with_shape = {
            let direct = source;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        *candidate,
                    )
                    .is_some()
                })?
        };
        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        *candidate,
                    )
                    .is_some()
                })?
        };

        let source_shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            source_with_shape,
        )?;
        let target_shape = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            target_with_shape,
        )?;

        if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
            return None;
        }

        let missing_required_props: Vec<_> = target_shape
            .properties
            .iter()
            .filter(|p| !p.optional)
            .filter(|prop| !source_shape.properties.iter().any(|p| p.name == prop.name))
            .collect();
        if missing_required_props.len() != 1 {
            return None;
        }

        Some(missing_required_props[0].name)
    }

    /// Look up a displayable non-generic type alias name for a TypeId.
    pub(crate) fn lookup_type_alias_name_for_display(&self, ty: TypeId) -> Option<String> {
        // Only check composite types — tsc does NOT preserve alias names for
        // primitive types (number, string, etc.) or literal types.
        // Restricting to object/function/callable/union/intersection types avoids
        // regressions like `number` → `TypeOfInfinity`.
        let is_object =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, ty).is_some();
        let is_union = if !is_object {
            crate::query_boundaries::common::union_members(self.ctx.types, ty).is_some()
        } else {
            false
        };
        let is_function = if !is_object && !is_union {
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty).is_some()
                || crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, ty)
                    .is_some()
        } else {
            false
        };
        if !is_object && !is_function && !is_union {
            return None;
        }

        // If the type has a display alias (produced by evaluating a generic
        // Application like B<string>), let the formatter handle it — using the
        // raw alias name would lose the type arguments.
        if self.ctx.types.get_display_alias(ty).is_some_and(|alias| {
            crate::query_boundaries::common::type_application(self.ctx.types, alias).is_some()
        }) {
            return None;
        }
        if let Some(alias) = self.ctx.types.get_display_alias(ty)
            && let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, alias)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && def.type_params.is_empty()
        {
            let name = self.ctx.types.resolve_atom_ref(def.name);
            if name.contains('<') {
                return Some(name.to_string());
            }
        }

        // For intersection types (e.g., typeof X & Function), expand to the full
        // type representation rather than using the alias name. This matches tsc's
        // behavior in assignability messages for complex intersection types.
        if crate::query_boundaries::common::intersection_members(self.ctx.types, ty).is_some() {
            return None;
        }

        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(ty)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind != tsz_solver::def::DefKind::TypeAlias
            && !is_union
        {
            return None;
        }

        // Try body_to_alias first (raw alias body), then fall back to
        // type_to_def (evaluated alias form registered by the checker).
        let def_id = self
            .ctx
            .definition_store
            .find_type_alias_by_body(ty)
            .or_else(|| {
                let def_id = self.ctx.definition_store.find_def_for_type(ty)?;
                let def = self.ctx.definition_store.get(def_id)?;
                if def.kind == tsz_solver::def::DefKind::TypeAlias {
                    Some(def_id)
                } else {
                    None
                }
            })?;
        let def = self.ctx.definition_store.get(def_id)?;
        // Only use the alias for non-generic type aliases.  Generic aliases
        // need type argument display (e.g., B<string> not B).
        if !def.type_params.is_empty() {
            return None;
        }
        // `type T = typeof value` aliases display as the resolved value type
        // in assignment diagnostics. Do not repaint that resolved body as `T`.
        if def.body.is_some_and(|body| {
            crate::query_boundaries::common::is_type_query_type(self.ctx.types, body)
        }) || self.type_alias_definition_body_is_type_query(&def)
        {
            return None;
        }
        // Skip aliases whose body was computed by intersection reduction or
        // conditional evaluation. tsc shows the expanded form for these.
        if let Some(body) = def.body
            && self.ctx.definition_store.is_computed_body(body)
        {
            return None;
        }
        let name = self.ctx.types.resolve_atom_ref(def.name);
        Some(name.to_string())
    }
    pub(crate) fn recursive_non_generic_alias_body_name(&self, ty: TypeId) -> String {
        crate::query_boundaries::recursive_alias::recursive_non_generic_type_alias_body_name(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            ty,
        )
        .map(|name| self.ctx.types.resolve_atom_ref(name).to_string())
        .unwrap_or_else(|| self.format_type_diagnostic(ty))
    }

    pub(in crate::error_reporter) fn compute_ambiguous_conditional_display(
        &mut self,
        ty: TypeId,
    ) -> Option<TypeId> {
        let db = self.ctx.types.as_type_database();
        let cond = crate::query_boundaries::state::type_environment::get_conditional_type(db, ty)?;
        if !cond.is_distributive {
            return None;
        }
        let param_info = crate::query_boundaries::common::type_param_info(db, cond.check_type)?;
        let branches_are_concrete =
            !crate::query_boundaries::common::contains_type_parameters(db, cond.true_type)
                && !crate::query_boundaries::common::contains_type_parameters(db, cond.false_type);
        if !branches_are_concrete {
            return None;
        }
        let constraint = match param_info.constraint {
            Some(c) => c,
            None => return Some(self.ctx.types.union2(cond.true_type, cond.false_type)),
        };
        if crate::query_boundaries::assignability::is_fresh_subtype_of(
            db,
            constraint,
            cond.extends_type,
        ) {
            return None;
        }
        let extends_members: Vec<TypeId> =
            crate::query_boundaries::common::union_members(db, cond.extends_type)
                .unwrap_or_else(|| vec![cond.extends_type]);
        let has_overlap = extends_members.iter().any(|&m| {
            crate::query_boundaries::assignability::is_fresh_subtype_of(db, m, constraint)
        });
        if has_overlap {
            Some(self.ctx.types.union2(cond.true_type, cond.false_type))
        } else {
            None
        }
    }
}
