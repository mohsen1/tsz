//! Type formatting and diagnostic anchor helpers for error reporter.
use crate::query_boundaries::common as query_common;
use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::{CheckerState, MemberAccessLevel};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn format_type_diagnostic_for_assignability_display(&mut self, type_id: TypeId) -> String {
        let exact_optional = self.ctx.compiler_options.exact_optional_property_types;
        let mut formatter = self
            .ctx
            .create_diagnostic_type_formatter()
            .with_display_properties()
            .with_expand_scalar_mapped_alias_applications()
            .with_preserve_optional_parameter_surface_syntax(true)
            .with_exact_optional_property_types(exact_optional);
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
        // Unique builder chain: the long-property-receiver / skip-application-alias
        // flags are exclusive to this surface, so it has no shared factory.
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
        // Fail-safe work-budget scope for callers that bypass
        // `format_type_for_diagnostic_role` (issue #13040).
        let _budget_scope = crate::error_reporter::display_budget::DisplayBudgetScope::enter();
        let format_with_def_store = |state: &Self, type_id: TypeId| {
            let mut formatter = state.ctx.create_assignability_type_formatter();
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

        // A conditional-bodied type alias loses its name once the conditional
        // reduces to a concrete type (`Tail<Src>` -> `[number, string]`); render
        // the structural form, leaving the provenance intact for the evaluator.
        // See `reduced_conditional_alias_display_should_skip_application`.
        if self.reduced_conditional_alias_display_should_skip_application(ty) {
            return self.format_type_for_assignability_message_skip_application_alias(ty);
        }

        // A *still-deferred* generic application whose alias body reduces through
        // a conditional or indexed access (`Classify<"x">`, `Head<[a, b]>`,
        // `Val<{…}>`, and the same through an alias chain) drops tsc's
        // `aliasSymbol` once it resolves to a concrete shape, so tsc prints the
        // evaluated structural form. The scalar/literal reductions already reach
        // the eager-evaluation path below (the checker collapses them to a shared
        // singleton with no alias), but object/tuple/array reductions retain the
        // application surface and would otherwise leak `Classify<"x">` through the
        // `lookup_type_alias_name_for_display` short-circuit further down. Reuse
        // the same target-side reduction policy so both diagnostic pair sides
        // agree; the helper keeps the alias for free-type-parameter, stalled, and
        // mapped-bodied applications (`Wrap<T>`, `Cond<T>`, `MapIt<…>`) and defers
        // a non-generic alias wrapping such an application to the path below.
        if let Some(display) = self.reduced_generic_application_source_display(ty) {
            return display;
        }

        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty)
            && let Some(def) = self.ctx.definition_store.get(def_id)
            && def.kind == tsz_solver::def::DefKind::TypeAlias
            && def.type_params.is_empty()
        {
            // A type-position `E.X` reference is stabilized as a def whose
            // binder symbol carries `ENUM_MEMBER`; the alias-name fallthrough
            // below would leak the bare member name (`X`). Route it through
            // the enum naming so it renders qualified (`E.X`) — or as the
            // bare enum name for a single-member enum (tsc identity). The
            // helper answers `None` for every non-enum lazy ref, so no extra
            // gate is needed here.
            if let Some(name) = self.format_qualified_enum_name_for_message(ty) {
                return name;
            }
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
            if evaluated != ty {
                if self.ctx.types.get_display_alias(evaluated).is_some()
                    && !crate::query_boundaries::recursive_alias::is_def_non_generic_recursive_alias(
                        self.ctx.types.as_type_database(),
                        &self.ctx.definition_store,
                        def_id,
                    )
                {
                    return self.format_type_for_assignability_message(evaluated);
                }
                // tsc attaches an alias symbol (and renders the alias name) only
                // to freshly-constructed structural types. A non-generic alias
                // whose body resolves to a bare intrinsic keyword or a literal
                // points at a shared singleton type with no alias symbol, so tsc
                // shows the underlying type (`string`, `42`, `never`, …) —
                // including through alias chains (`type A = B; type B = string`
                // renders as `string`). `evaluate_type_with_env` collapses the
                // chain, so a single check on the evaluated form covers the family.
                if crate::query_boundaries::type_predicates::is_intrinsic_or_literal_type(
                    self.ctx.types.as_type_database(),
                    evaluated,
                ) {
                    return self.format_type_for_assignability_message(evaluated);
                }
            }
            // tsc drops the alias symbol for a non-generic alias whose body is a
            // *computed* operator that resolves away — a conditional, indexed
            // access, `keyof`, template literal, string-mapping intrinsic, or a
            // utility application bottoming out at a shared singleton. The
            // `is_computed_body` / `evaluate_type_with_env` checks above only
            // catch bodies the checker explicitly marked computed or that the
            // environment evaluator fully reduces to an intrinsic; a body like
            // `true extends true ? string : number` is neither, so the alias
            // name would otherwise leak (`X1` instead of `string`). Consult the
            // shared solver display policy, which evaluates the computed body the
            // same way the `TypeFormatter` does, so the two diagnostic pipelines
            // agree.
            if let Some(underlying) =
                crate::query_boundaries::assignability_alias_display::type_alias_displayed_as_underlying(
                    self.ctx.types.as_type_database(),
                    &self.ctx.definition_store,
                    def_id,
                )
            {
                return self.format_type_diagnostic_for_assignability_display(underlying);
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
            // tsc always prints `keyof T` when the operand is a free type
            // parameter, even when T has an inline anonymous constraint whose
            // keys could be enumerated. The anonymous-object branch below
            // reaches the constraint via `get_object_shape`'s TypeParameter
            // look-through, so we must guard here before that path fires.
            if let Some(param_info) = crate::query_boundaries::common::type_param_info(
                self.ctx.types.as_type_database(),
                keyof_inner,
            ) {
                let param_name = self.ctx.types.resolve_atom_ref(param_info.name);
                return format!("keyof {param_name}");
            }

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

            // Anonymous object operand (an inline `keyof { ... }` type literal):
            // the operand has no user-visible name, so tsc renders the evaluated
            // key set (`"a" | "b"`) rather than the `keyof { ... }` spelling — an
            // index type only prints `keyof X` when `X` is a named reference. The
            // alias-name branch above already returned for a named alias and the
            // symbol-name branch for a symbol-bearing operand, so here it is
            // enough to confirm the operand is an object with no binder symbol.
            if crate::query_boundaries::common::object_shape_for_type(self.ctx.types, keyof_inner)
                .is_some_and(|shape| shape.symbol.is_none())
            {
                let evaluated = self.evaluate_type_with_env(ty);
                if evaluated != ty
                    && evaluated != TypeId::ERROR
                    && crate::query_boundaries::common::keyof_inner_type(self.ctx.types, evaluated)
                        .is_none()
                {
                    return self.format_type_for_assignability_message(evaluated);
                }
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

                // If display_alias didn't provide type args, recover the
                // application's ACTUAL arguments from properties whose declared
                // type is a bare type parameter (`value: T` → the instantiated
                // property type IS the argument for `T`, at `T`'s declared
                // index). Each recovered argument is placed at its parameter's
                // index; the display is only rewritten when every parameter is
                // recovered consistently. Harvesting other member types (the
                // old name-sorted property-type zip) fabricated argument lists
                // unrelated to the actual instantiation, so unrecoverable
                // arguments are elided (bare name) instead.
                if !formatted.contains('<') {
                    let type_param_names = self.symbol_type_param_names_for_display(symbol);
                    let type_param_count = type_param_names.len();
                    if type_param_count > 0 {
                        // For methods, the declared-type matcher inspects the
                        // declared RETURN annotation, so project the
                        // instantiated return type as the candidate value.
                        let candidate_value_type = |prop: &tsz_solver::PropertyInfo| -> TypeId {
                            if !prop.is_method {
                                return prop.type_id;
                            }
                            if let Some(fn_shape) =
                                crate::query_boundaries::common::function_shape_for_type(
                                    self.ctx.types,
                                    prop.type_id,
                                )
                            {
                                return fn_shape.return_type;
                            }
                            if let Some(callable) =
                                crate::query_boundaries::common::callable_shape_for_type(
                                    self.ctx.types,
                                    prop.type_id,
                                )
                                && callable.call_signatures.len() == 1
                            {
                                return callable.call_signatures[0].return_type;
                            }
                            prop.type_id
                        };
                        let mut slots: Vec<Option<TypeId>> = vec![None; type_param_count];
                        let mut conflict = false;
                        for prop in shape.properties.iter() {
                            let Some((index, candidate)) = self
                                .declared_property_type_arg_candidate_for_display(
                                    symbol,
                                    prop.name,
                                    candidate_value_type(prop),
                                    &type_param_names,
                                )
                            else {
                                continue;
                            };
                            match slots[index] {
                                None => slots[index] = Some(candidate),
                                Some(existing) if existing == candidate => {}
                                Some(_) => {
                                    conflict = true;
                                    break;
                                }
                            }
                        }
                        if !conflict && slots.iter().all(Option::is_some) {
                            let args: Vec<String> = slots
                                .iter()
                                .flatten()
                                .map(|type_id| {
                                    self.format_type_diagnostic_for_assignability_display(*type_id)
                                })
                                .collect();
                            formatted = format!("{}<{}>", symbol_name, args.join(", "));
                        }
                    }
                }
            }
        }

        // Callable-only shapes (call/construct-signature interfaces) carry no
        // properties, so recover their actual arguments from declared
        // signature annotations that are bare type parameters — again
        // index-placed and all-or-elide, never positional harvesting.
        if !formatted.contains('<')
            && let Some(sym_id) =
                crate::query_boundaries::common::type_shape_symbol(self.ctx.types, display_ty)
            && let Some(symbol) = self.get_cross_file_symbol(sym_id)
        {
            let symbol_name = symbol.escaped_name.clone();
            let type_param_names = self.symbol_type_param_names_for_display(symbol);
            let type_param_count = type_param_names.len();
            if type_param_count > 0 {
                let mut slots: Vec<Option<TypeId>> = vec![None; type_param_count];
                let mut conflict = false;
                self.fill_signature_type_arg_slots_for_display(
                    symbol,
                    display_ty,
                    &type_param_names,
                    &mut slots,
                    &mut conflict,
                );
                if !conflict && slots.iter().all(Option::is_some) {
                    let args: Vec<String> = slots
                        .iter()
                        .flatten()
                        .map(|type_id| {
                            self.format_type_diagnostic_for_assignability_display(*type_id)
                        })
                        .collect();
                    formatted = format!("{}<{}>", symbol_name, args.join(", "));
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

    pub(crate) fn format_type_for_assignability_message_skip_application_alias(
        &mut self,
        ty: TypeId,
    ) -> String {
        self.ensure_relation_input_ready(ty);
        let mut formatter = self
            .ctx
            .create_assignability_type_formatter()
            .with_skip_application_display_alias_chase();
        formatter.format(ty).into_owned()
    }

    /// Format an assignability-message type while suppressing the `display_alias`
    /// repaint on `Object` / `ObjectWithIndex` nodes.
    ///
    /// An evaluated structural object can carry an evaluation-origin
    /// `display_alias` back to the `Application` it reduced from. For a
    /// self-referential `Static<typeof X>` schema alias, the reduced object *is*
    /// the alias body, so the alias `X` repaints it (`X[]` instead of the
    /// structural shape). Callers that have already reduced a type to a concrete
    /// structural shape use this entry so the shape is rendered rather than
    /// collapsing back to the alias spelling.
    pub(crate) fn format_type_for_assignability_message_skip_object_display_alias(
        &mut self,
        ty: TypeId,
    ) -> String {
        self.ensure_relation_input_ready(ty);
        let mut formatter = self
            .ctx
            .create_assignability_type_formatter()
            .with_skip_object_display_alias();
        formatter.format(ty).into_owned()
    }

    /// Format an assignability-message type while rendering a composite
    /// (`Object` / `Union` / `Intersection`) structurally instead of repainting
    /// it with a coincidentally-shaped non-generic type-alias name.
    ///
    /// Used when the operand came from an inline / anonymous composite annotation
    /// (`const x: { a: number } = …`, `… : { a: number } | { b: string }`,
    /// or a synthesized rest-argument tuple): tsc spells the alias name only
    /// when the reference carried an `aliasSymbol`, so an anonymous annotation
    /// must render the structural shape. Nominal shapes (interfaces / classes)
    /// and generic applications keep their names.
    pub(crate) fn format_type_for_assignability_message_anonymous_composite_structural(
        &mut self,
        ty: TypeId,
    ) -> String {
        self.ensure_relation_input_ready(ty);
        let mut formatter = self
            .ctx
            .create_assignability_type_formatter()
            .with_anonymous_composite_structural();
        formatter.format(ty).into_owned()
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
        if !tsz_common::text_scan::is_ascii_identifier_start_char(first) {
            return None;
        }
        if !chars.all(tsz_common::text_scan::is_ascii_identifier_continue_char) {
            return None;
        }

        match name {
            "any" | "unknown" | "never" | "string" | "number" | "boolean" | "symbol" | "bigint"
            | "void" | "undefined" | "null" | "object" => None,
            _ => Some(name),
        }
    }

    pub(in crate::error_reporter) fn variadic_tuple_alias_structural_display(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<String> {
        let evaluated = self.evaluate_type_with_env(ty);
        if evaluated == TypeId::ERROR {
            return None;
        }

        let elements = crate::query_boundaries::common::tuple_elements(self.ctx.types, evaluated)?;
        if !elements.iter().any(|element| element.rest) {
            return None;
        }

        let other_evaluated = self.evaluate_type_for_assignability(other);
        if crate::query_boundaries::common::tuple_elements(self.ctx.types, other).is_none()
            && crate::query_boundaries::common::tuple_elements(self.ctx.types, other_evaluated)
                .is_none()
        {
            return None;
        }

        Some(self.format_type_for_assignability_message_anonymous_composite_structural(evaluated))
    }

    fn format_assignability_type_for_message_internal(
        &mut self,
        ty: TypeId,
        other: TypeId,
        strip_top_level_nullish: bool,
    ) -> String {
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

    /// Shared guard behind the nullable-union assignability display policy.
    ///
    /// Returns the non-nullish members of `ty` when `ty` is a union carrying
    /// `null`/`undefined`, `other` is a non-nullable type (directly or via its
    /// base constraint), and the strip yields a proper, non-empty subset.
    /// Returns `None` when no strip applies. Both the target-display collapse
    /// and the source-side "carries nullish the target lacks" predicate build
    /// on this; only the *interpretation* of the surviving members differs.
    fn nullish_stripped_members(&mut self, ty: TypeId, other: TypeId) -> Option<Vec<TypeId>> {
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
        // tsc never collapses the nullish members when the OTHER side is a
        // type parameter (or an intersection carrying one), constrained or
        // not: a generic operand's relation to a union defers to its
        // constraint instead of walking the union's constituents, so the
        // message keeps the full declared union (`Q` vs `string | undefined`
        // stays `string | undefined`; a constrained param drills its
        // *constraint* against the stripped member one level deeper).
        // Concrete operands keep the collapse (`number` vs
        // `string | undefined` renders `string`).
        if query_common::is_type_parameter_or_intersection_with_type_parameter(
            self.ctx.types.as_type_database(),
            other,
        ) {
            return None;
        }
        // When `other` is a generic type (type parameter or intersection of type
        // parameters), reduce it to its base constraint and check if that
        // contains null/undefined.  tsc preserves the full target union when
        // the source's base constraint is nullable.  Example:
        //   source `T & U` where constraints are `string | ... | undefined`
        //   target `string | null` must stay `string | null` (not `string`).
        let other_base = diagnostic_query::get_base_constraint_for_display(
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
        Some(filtered)
    }

    /// Collapses a nullable-union *target* to its non-nullish part for display.
    ///
    /// tsc's assignability messages drill through `getBestMatchingType`: the
    /// nullish members are dropped from the displayed target **only when a
    /// single real member survives** the strip (e.g. `string | undefined` →
    /// `string`, and a fresh literal source widens against it). When two or
    /// more real members remain, tsc keeps the *original* union — nullish
    /// members included — so this returns `None` to leave the full union in
    /// place. The relation itself always runs against the full declared union;
    /// this is display-only.
    pub(crate) fn strip_nullish_for_assignability_display(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> Option<TypeId> {
        match self.nullish_stripped_members(ty, other)?.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    pub(crate) fn should_strip_nullish_for_property_display(&self, target: TypeId) -> bool {
        crate::query_boundaries::common::union_members(self.ctx.types, target).is_some()
            || crate::query_boundaries::common::intersection_members(self.ctx.types, target)
                .is_some()
    }

    /// `tsc` never strips a *source* union's top-level `null`/`undefined`
    /// members from an assignability message. The shared assignment-source
    /// display policy, however, strips them when the target is non-nullable —
    /// correct only for the *target* side ("show the non-nullable part of the
    /// target"). Applied to the source it drops members `tsc` keeps, collapsing
    /// e.g. `string[] | undefined` to `string[]`. When that stripped source
    /// display then equals the target display, the duplicate-name TS2719 gate
    /// ("two different types with this name exist") misfires where `tsc` reports
    /// a plain TS2322 (`Type 'string[] | undefined' is not assignable to type
    /// 'string[]'`).
    ///
    /// When the rendered `source_str` has collapsed to equal `target_str` and
    /// the source structurally carries top-level `null`/`undefined` the target
    /// lacks, return a source display that preserves those members so the
    /// diagnostic stays TS2322. This is purely a display correction: the
    /// relation verdict and the failing union member are unchanged. Returns
    /// `None` (leaving the existing display, and thus the existing TS2719 path,
    /// intact) for genuinely distinct same-named nominal types, which carry no
    /// such nullish difference.
    pub(crate) fn source_display_preserving_nullish_if_collapsed_to_target(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_str: &str,
        target_str: &str,
    ) -> Option<String> {
        if source_str != target_str {
            return None;
        }
        // Structural witness that the source carries top-level `null`/`undefined`
        // the target does not: `nullish_stripped_members` yields a non-empty
        // subset in exactly that case, independent of how many real members
        // survive. Use it only as the predicate; the display itself is
        // recomputed with the nullish-preserving formatter.
        self.nullish_stripped_members(source, target)?;
        let preserved =
            self.format_assignability_type_for_message_preserving_nullish(source, target);
        (preserved != target_str).then_some(preserved)
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
        // tsc's default `typeToString` never namespace-qualifies an enum:
        // `namespace P { export enum Q {} }` renders `Q`, and a member renders
        // `Q.R`. The namespace-qualified spelling (`P.Q`) appears only through
        // `getTypeNameForErrorDisplay` (`TypeFormatFlags.UseFullyQualifiedType`),
        // which `reportRelationError` applies to a *generalized* literal-ish
        // source — see `format_fully_qualified_enum_name_for_message`.
        self.format_enum_name_for_message_internal(ty, false)
    }

    /// tsc `getTypeNameForErrorDisplay`: the enum naming with
    /// `UseFullyQualifiedType`, i.e. qualified through enclosing
    /// namespace/module declarations (`P.Q`). Reserved for the generalized
    /// relation-source display; every other message path uses the bare
    /// [`Self::format_qualified_enum_name_for_message`] spelling.
    pub(super) fn format_fully_qualified_enum_name_for_message(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        self.format_enum_name_for_message_internal(ty, true)
    }

    fn format_enum_name_for_message_internal(
        &mut self,
        ty: TypeId,
        fully_qualified: bool,
    ) -> Option<String> {
        // Accept both the evaluated `Enum` data and a still-deferred
        // `Lazy(DefId)` member ref (a type-position `E.X` annotation is
        // stabilized as a def whose binder symbol carries `ENUM_MEMBER`).
        // The lazy form is gated on the enum symbol flags below so ordinary
        // alias/interface refs never reach the enum naming.
        let enum_data_def = crate::query_boundaries::common::enum_def_id(self.ctx.types, ty);
        let def_id = enum_data_def
            .or_else(|| crate::query_boundaries::common::lazy_def_id(self.ctx.types, ty))?;
        // Parent-edge path first: it covers member defs whose binder symbol is
        // not wired (`def_to_symbol_id_with_fallback` fails and the bare
        // member name would leak), and it encodes tsc's single-member
        // identity — a single-member enum's member type IS the enum type and
        // renders as the bare enum name. The environment lookup already falls
        // back to the shared definition store; the resolver's symbol-based
        // lookup covers canonicalized twins of the decl-site def that neither
        // map saw.
        if let Some(parent_id) = self
            .ctx
            .type_env
            .try_borrow()
            .ok()
            .and_then(|env| env.get_enum_parent(def_id))
            .or_else(|| {
                tsz_solver::resolver::TypeResolver::get_enum_parent_def_id(&self.ctx, def_id)
            })
            && let Some(parent) = self.ctx.definition_store.get(parent_id)
        {
            let parent_name = self.ctx.types.resolve_atom_ref(parent.name).to_string();
            if parent.enum_members.len() == 1 {
                return Some(parent_name);
            }
            if let Some(def) = self.ctx.definition_store.get(def_id) {
                let member_name = self.ctx.types.resolve_atom_ref(def.name);
                return Some(format!("{parent_name}.{member_name}"));
            }
        }
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER) {
            let parent = self.ctx.binder.get_symbol(symbol.parent)?;
            // tsc single-member identity: the lone member's type IS the enum
            // type, so it displays as the bare enum name.
            if parent
                .exports
                .as_ref()
                .is_some_and(|members| members.len() == 1)
            {
                return Some(parent.escaped_name.clone());
            }
            return Some(format!("{}.{}", parent.escaped_name, symbol.escaped_name));
        }
        // A lazy ref that is not an enum member (interface/alias/namespace)
        // must not be renamed by the enum machinery.
        if enum_data_def.is_none() && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM) {
            return None;
        }
        if !fully_qualified {
            return Some(symbol.escaped_name.clone());
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

        // Use tsc's getSpellingSuggestion algorithm with weighted Levenshtein
        // via the shared candidate scan. tsc uses substitution cost 2.0 (0.1 for
        // case-only diffs), which means short strings like "baz" vs "bar" won't
        // trigger a suggestion.
        Self::best_spelling_suggestion(&source_str, target_literals.iter().map(String::as_str))
            // TSC wraps the suggestion in double quotes (it's a string literal type name)
            .map(|s| format!("\"{s}\""))
    }

    /// Find a TS2820 spelling suggestion for a target that may still be a
    /// type-alias application, conditional, or other deferred form rather than
    /// an already-flattened string-literal union.
    ///
    /// `find_string_literal_spelling_suggestion` only enumerates members of a
    /// *literal* `Union` node. tsc, however, computes the suggestion against the
    /// reduced target (`getReducedType`), so a target like
    /// `Strip<"prefix_a" | "prefix_b">` (a distributive conditional that
    /// captures literals via a template-`infer`) must first be evaluated to its
    /// `"a" | "b"` union before the near-miss scan can see the candidates. This
    /// walks the raw target, its environment-evaluated form, and its
    /// display-deep-reduced form, returning the first suggestion any of them
    /// yields. Structural throughout — no alias/name/text matching.
    pub(crate) fn find_string_literal_spelling_suggestion_reduced(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Option<String> {
        // A suggestion is only ever produced for a string-literal source, so
        // skip the (cached but non-trivial) target reduction work entirely for
        // the overwhelmingly common non-literal-source assignment failures.
        if !matches!(
            crate::query_boundaries::common::literal_value(self.ctx.types, source),
            Some(tsz_solver::LiteralValue::String(_))
        ) {
            return None;
        }
        if let Some(suggestion) = self.find_string_literal_spelling_suggestion(source, target) {
            return Some(suggestion);
        }
        let evaluated = self.evaluate_type_with_env(target);
        if evaluated != target
            && let Some(suggestion) =
                self.find_string_literal_spelling_suggestion(source, evaluated)
        {
            return Some(suggestion);
        }
        let deep_reduced = {
            let env = self.ctx.type_env.borrow();
            crate::query_boundaries::diagnostics::deep_reduce_for_display(
                self.ctx.types,
                &*env,
                evaluated,
            )
        };
        if deep_reduced != evaluated && deep_reduced != target {
            return self.find_string_literal_spelling_suggestion(source, deep_reduced);
        }
        None
    }

    pub(in crate::error_reporter) fn format_ts2820_target_display(
        &mut self,
        target: TypeId,
        evaluated_target: TypeId,
        target_str: &str,
    ) -> String {
        if self.ts2820_target_contains_application_surface(target)
            || self.ts2820_target_contains_alias_surface(target)
        {
            return Self::widen_numeric_member_literals_in_display_text(target_str);
        }

        self.format_type_diagnostic(evaluated_target)
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
                                crate::query_boundaries::common::find_matching_property(
                                    &source_callable.properties,
                                    prop.name,
                                )
                                .is_some()
                            } else if let Some(source_shape) =
                                crate::query_boundaries::common::object_shape_for_type(
                                    self.ctx.types,
                                    *candidate,
                                )
                            {
                                crate::query_boundaries::common::find_matching_property(
                                    &source_shape.properties,
                                    prop.name,
                                )
                                .is_some()
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

        // Reuse the already-resolved candidate arrays (`[direct, resolved,
        // evaluated]`) rather than recomputing the resolve/evaluate pipeline.
        let source_with_shape = source_candidates.into_iter().find(|candidate| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, *candidate)
                .is_some()
        })?;
        let target_with_shape = target_candidates.into_iter().find(|candidate| {
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, *candidate)
                .is_some()
        })?;

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
}
