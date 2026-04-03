//! Type formatting and diagnostic anchor helpers for error reporter.
//!
//! Contains assignability message formatting, enum name display,
//! missing property detection, and AST anchor resolution.
//!
//! Extracted from `core.rs` to keep module size manageable.

use crate::state::{CheckerState, MemberAccessLevel};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn format_type_for_assignability_message(&mut self, ty: TypeId) -> String {
        if let Some(collapsed) = self.format_union_with_collapsed_enum_display(ty) {
            return collapsed;
        }

        if let Some(keyof_inner) = tsz_solver::keyof_inner_type(self.ctx.types, ty) {
            if let Some(alias_name) = self.lookup_type_alias_name_for_display(keyof_inner) {
                return format!("keyof {alias_name}");
            }

            if let Some(shape) =
                tsz_solver::type_queries::get_object_shape(self.ctx.types, keyof_inner)
                && let Some(sym_id) = shape.symbol
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                return format!("keyof {}", symbol.escaped_name);
            }
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

        let evaluated = self.evaluate_type_for_assignability(ty);
        if self.should_use_evaluated_assignability_display(ty, evaluated) {
            return self.format_type_for_assignability_message(evaluated);
        }

        if let Some((object_type, index_type)) =
            tsz_solver::type_queries::get_index_access_types(self.ctx.types, ty)
            && let Some(extract_display) = self.format_extract_keyof_string_type(index_type)
        {
            let object_display = self.format_type_for_assignability_message(object_type);
            return format!("{object_display}[{extract_display}]");
        }

        if let Some(extract_display) = self.format_extract_keyof_string_type(ty) {
            return extract_display;
        }

        // Check for type alias names BEFORE normalization, which transforms the
        // TypeId and breaks the body_to_alias lookup.  tsc preserves alias names
        // in assignability messages (e.g. "not assignable to type 'FuncType'"
        // instead of expanding to the function signature).
        if let Some(alias_name) = self.lookup_type_alias_name_for_display(ty) {
            return alias_name;
        }

        let display_ty = self.normalize_assignability_display_type(ty);
        // Do NOT use display properties — tsc shows widened property types
        // in error messages: `{ two: number }` not `{ two: 1 }`.
        let mut formatted = self.format_type_diagnostic(display_ty);

        // Preserve generic instantiations for nominal class instance names when possible.
        if !formatted.contains('<')
            && let Some(shape) =
                tsz_solver::type_queries::get_object_shape(self.ctx.types, display_ty)
            && let Some(sym_id) = shape.symbol
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let symbol_name = symbol.escaped_name.as_str();
            if formatted == symbol_name {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                let type_param_count =
                    if let Some(type_params) = self.ctx.get_def_type_params(def_id) {
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
                    // Recover instantiation args from actual value-carrying members first.
                    // Methods tend to encode `T` as `() => T`, which produces noisy
                    // displays like `C<() => string>` instead of `C<string>`.
                    let build_candidates = |predicate: fn(&tsz_solver::PropertyInfo) -> bool| {
                        let mut candidates: Vec<(String, TypeId)> = shape
                            .properties
                            .iter()
                            .filter(|prop| predicate(prop))
                            .filter_map(|prop| {
                                let name = self.ctx.types.resolve_atom_ref(prop.name).to_string();
                                if name.starts_with("__private_brand_") {
                                    None
                                } else {
                                    Some((name, prop.type_id))
                                }
                            })
                            .collect();
                        candidates.sort_by(|a, b| a.0.cmp(&b.0));
                        candidates
                    };
                    let mut candidates =
                        build_candidates(|prop| !prop.is_method && !prop.is_class_prototype);
                    if candidates.len() < type_param_count {
                        candidates = build_candidates(|prop| !prop.is_method);
                    }
                    if candidates.len() < type_param_count {
                        candidates = build_candidates(|prop| !prop.is_class_prototype);
                    }
                    if candidates.len() < type_param_count {
                        candidates = build_candidates(|_| true);
                    }
                    let args: Vec<String> = candidates
                        .iter()
                        .take(type_param_count)
                        .map(|(_, type_id)| self.format_type_diagnostic(*type_id))
                        .collect();
                    if args.len() == type_param_count {
                        formatted = format!("{}<{}>", symbol_name, args.join(", "));
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
        formatted
    }

    pub(crate) fn format_assignability_type_for_message(
        &mut self,
        ty: TypeId,
        other: TypeId,
    ) -> String {
        if tsz_solver::literal_value(self.ctx.types, ty).is_some()
            && tsz_solver::string_intrinsic_components(self.ctx.types, other)
                .is_some_and(|(_, type_arg)| type_arg == TypeId::STRING)
        {
            let widened = self.widen_type_for_display(ty);
            return self.format_type_for_assignability_message(widened);
        }

        if let Some(enum_name) = self.format_disambiguated_enum_name_for_assignment(ty, other) {
            return enum_name;
        }

        // When displaying the TARGET type and the SOURCE is non-nullable,
        // strip null/undefined from the top-level union to match tsc's behavior.
        // tsc only shows the non-nullable part of the target since null/undefined
        // are not relevant to the structural mismatch.
        if let Some(stripped) = self.strip_nullish_for_assignability_display(ty, other) {
            return self.format_type_for_assignability_message(stripped);
        }

        self.format_type_for_assignability_message(ty)
    }

    /// When `ty` is a union containing null/undefined and `other` (the
    /// counterpart in the assignability check) is non-nullable, strip the
    /// top-level null/undefined members from `ty`.  This matches tsc which
    /// shows only the non-nullable part of the target to reduce noise.
    fn strip_nullish_for_assignability_display(
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

    fn format_union_with_collapsed_enum_display(&mut self, ty: TypeId) -> Option<String> {
        let members = crate::query_boundaries::common::union_members(self.ctx.types, ty)?;
        if members.len() < 2 {
            return None;
        }

        let mut rendered = Vec::with_capacity(members.len());
        let mut collapsed_enum = None;

        for member in members {
            let widened = self.widen_enum_member_type(member);
            if let Some(name) = self.format_qualified_enum_name_for_message(widened) {
                match collapsed_enum.as_ref() {
                    Some(existing) if existing == &name => {}
                    None => {
                        collapsed_enum = Some(name.clone());
                        rendered.push(name);
                    }
                    Some(_) => return None,
                }
            } else {
                rendered.push(self.format_type_for_assignability_message(member));
            }
        }

        if collapsed_enum.is_some() {
            Some(rendered.join(" | "))
        } else {
            None
        }
    }

    fn format_qualified_enum_name_for_message(&mut self, ty: TypeId) -> Option<String> {
        let def_id = tsz_solver::type_queries::get_enum_def_id(self.ctx.types, ty)?;
        let sym_id = self.ctx.def_to_symbol_id_with_fallback(def_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let mut parts = vec![symbol.escaped_name.clone()];
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol.declarations.first().copied()?
        };
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
                if (parent.flags
                    & (tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE
                        | tsz_binder::symbol_flags::ENUM))
                    == 0
                {
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
        let ty_symbol = self.ctx.binder.get_symbol(ty_sym)?;
        let other_symbol = self.ctx.binder.get_symbol(other_sym)?;

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

    fn is_exported_external_module_enum_symbol(&self, sym_id: tsz_binder::SymbolId) -> bool {
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
            tsz_solver::type_queries::get_function_shape(self.ctx.types, candidate).is_some()
                || tsz_solver::type_queries::get_callable_shape(self.ctx.types, candidate)
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
        use tsz_solver::type_queries;
        // Source must be a string literal
        let source_str = match tsz_solver::literal_value(self.ctx.types, source) {
            Some(tsz_solver::LiteralValue::String(atom)) => self.ctx.types.resolve_atom(atom),
            _ => return None,
        };

        // Collect target string literal members
        let target_literals: Vec<String> =
            if let Some(members) = type_queries::get_union_members(self.ctx.types, target) {
                members
                    .iter()
                    .filter_map(|&m| match tsz_solver::literal_value(self.ctx.types, m) {
                        Some(tsz_solver::LiteralValue::String(atom)) => {
                            Some(self.ctx.types.resolve_atom(atom))
                        }
                        _ => None,
                    })
                    .collect()
            } else if let Some(tsz_solver::LiteralValue::String(atom)) =
                tsz_solver::literal_value(self.ctx.types, target)
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

    pub(super) fn first_nonpublic_constructor_param_property(
        &mut self,
        ty: TypeId,
    ) -> Option<(String, MemberAccessLevel)> {
        let resolved = self.resolve_type_for_property_access(ty);
        let evaluated = self.judge_evaluate(resolved);
        let candidates = [ty, resolved, evaluated];

        let mut symbol_candidates: Vec<tsz_binder::SymbolId> = Vec::new();
        if let Some(sym) = candidates.into_iter().find_map(|candidate| {
            tsz_solver::type_queries::get_type_shape_symbol(self.ctx.types, candidate)
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
        if tsz_solver::is_primitive_type(self.ctx.types, source) {
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
                        tsz_solver::type_queries::get_callable_shape(self.ctx.types, *candidate)
                    {
                        source_callable
                            .properties
                            .iter()
                            .any(|p| p.name == required_atom)
                    } else if let Some(source_shape) =
                        tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate)
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
                    tsz_solver::type_queries::get_callable_shape(self.ctx.types, target_candidate)
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
            if let Some(target_callable) =
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, target_candidate)
            {
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
                                    tsz_solver::type_queries::get_callable_shape(
                                        self.ctx.types,
                                        *candidate,
                                    )
                                {
                                    source_callable
                                        .properties
                                        .iter()
                                        .any(|p| p.name == prop.name)
                                } else if let Some(source_shape) =
                                    tsz_solver::type_queries::get_object_shape(
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
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };
        let target_with_shape = {
            let direct = target;
            let resolved = self.resolve_type_for_property_access(direct);
            let evaluated = self.judge_evaluate(resolved);
            [direct, resolved, evaluated]
                .into_iter()
                .find(|candidate| {
                    tsz_solver::type_queries::get_object_shape(self.ctx.types, *candidate).is_some()
                })?
        };

        let source_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, source_with_shape)?;
        let target_shape =
            tsz_solver::type_queries::get_object_shape(self.ctx.types, target_with_shape)?;

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

    /// Look up a type alias name for a TypeId, returning the alias name if found.
    ///
    /// Uses the definition store's `body_to_alias` index to check if the given
    /// TypeId is the body of a non-generic type alias.  This must be called
    /// BEFORE `normalize_assignability_display_type`, which creates a new TypeId
    /// that won't match the stored body.
    fn lookup_type_alias_name_for_display(&self, ty: TypeId) -> Option<String> {
        // Only check composite types — tsc does NOT preserve alias names for
        // primitive types (number, string, etc.), literal types, or simple unions
        // of primitives.  Restricting to object/function/callable types avoids
        // regressions like `number` → `TypeOfInfinity`.
        let is_object = tsz_solver::type_queries::get_object_shape(self.ctx.types, ty).is_some();
        let is_function = if !is_object {
            if let Some(fn_shape) = tsz_solver::type_queries::get_function_shape(self.ctx.types, ty)
            {
                // Skip function types that have their own type parameters — these
                // are generic functions (including JSDoc @template callbacks) where
                // the DefInfo may report empty type_params even though the body is
                // generic. Using the alias name would lose the instantiated form.
                if !fn_shape.type_params.is_empty() {
                    return None;
                }
                true
            } else {
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, ty).is_some()
            }
        } else {
            false
        };
        if !is_object && !is_function {
            return None;
        }

        // If the type has a display alias (produced by evaluating a generic
        // Application like B<string>), let the formatter handle it — using the
        // raw alias name would lose the type arguments.
        if self.ctx.types.get_display_alias(ty).is_some() {
            return None;
        }

        // For intersection types (e.g., typeof X & Function), expand to the full
        // type representation rather than using the alias name. This matches tsc's
        // behavior in assignability messages for complex intersection types.
        if tsz_solver::type_queries::get_intersection_members(self.ctx.types, ty).is_some() {
            return None;
        }

        let def_id = self.ctx.definition_store.find_type_alias_by_body(ty)?;
        let def = self.ctx.definition_store.get(def_id)?;
        // Only use the alias for non-generic type aliases.  Generic aliases
        // need type argument display (e.g., B<string> not B).
        if !def.type_params.is_empty() {
            return None;
        }
        let name = self.ctx.types.resolve_atom_ref(def.name);
        Some(name.to_string())
    }
}
