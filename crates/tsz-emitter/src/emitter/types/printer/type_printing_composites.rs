use super::SolverLiteralValue as LiteralValue;
use super::ts7_sort_order::{
    Ts7SortNameSource, ts7_sort_literal, ts7_sort_name_source, ts7_union_sort_rank,
};
use super::{TypeId, TypePrinter, free_type_params_named, substitute_exact_types, visitor};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::interner::Atom;

impl<'a> TypePrinter<'a> {
    pub(crate) fn print_union(
        &self,
        type_id: TypeId,
        type_list_id: tsz_solver::types::TypeListId,
    ) -> String {
        let canonical_types = self.interner.type_list(type_list_id);
        let origin_types = self.interner.get_union_origin(type_id);
        let types = origin_types
            .as_deref()
            .map_or(canonical_types.as_ref(), Vec::as_slice);
        if types.is_empty() {
            return "never".to_string();
        }
        if let Some(enum_text) = self.try_print_enum_member_union_as_parent(&types) {
            return enum_text;
        }

        // Split members into real types vs nullish tail. tsc's type printer
        // emits the nullable members (`null`, `undefined`) at the end
        // of the union, in that canonical order — e.g. optional parameter
        // annotations render as `(X | undefined)[]`, not `(undefined | X)[]`.
        // Our solver stores unions in the order they were built, which for
        // `X | undefined` inferred from `a?: X[]` happens to be `undefined`
        // first. Re-ordering here keeps every other call site alone and
        // matches tsc's display without touching solver-level canonicalization.
        let mut real: Vec<TypeId> = Vec::with_capacity(types.len());
        let mut has_null = false;
        let mut has_undefined = false;
        for &type_id in types.iter() {
            // When strictNullChecks is off, filter null/undefined from unions.
            // `void` is not nullable in TypeScript 7's formatter and remains in
            // its canonical TypeFlags position.
            if !self.strict_null_checks && matches!(type_id, TypeId::NULL | TypeId::UNDEFINED) {
                continue;
            }
            match type_id {
                TypeId::NULL => has_null = true,
                TypeId::UNDEFINED => has_undefined = true,
                _ => real.push(type_id),
            }
        }

        // tsc's compareTypes orders union members by `getSortOrderFlags`,
        // which for primitives returns `TypeFlags` directly. Give primitive
        // intrinsics their TypeScript 7 flag order (`any` < `unknown` < `void`
        // < `string` < `number` < `bigint` < `boolean` < `symbol`/`object`) so an inferred
        // `number | string` prints as `string | number`. Non-primitive members
        // keep their original relative order because a sort comparator that
        // returns "equal" for them is stable.
        real.sort_by(|a, b| {
            let rank_cmp = self
                .stable_type_order_rank(*a)
                .cmp(&self.stable_type_order_rank(*b));
            if rank_cmp != std::cmp::Ordering::Equal {
                return rank_cmp;
            }
            match (
                self.stable_type_order_literal(*a),
                self.stable_type_order_literal(*b),
            ) {
                (Some(LiteralValue::String(left)), Some(LiteralValue::String(right))) => {
                    return self
                        .interner
                        .resolve_atom_ref(left)
                        .cmp(&self.interner.resolve_atom_ref(right));
                }
                (Some(LiteralValue::Number(left)), Some(LiteralValue::Number(right))) => {
                    return left.0.total_cmp(&right.0);
                }
                (Some(LiteralValue::Boolean(left)), Some(LiteralValue::Boolean(right))) => {
                    return left.cmp(&right);
                }
                _ => {}
            }
            if let (Some(left), Some(right)) = (
                self.stable_template_order_text(*a),
                self.stable_template_order_text(*b),
            ) {
                let cmp = left.cmp(&right);
                if cmp != std::cmp::Ordering::Equal {
                    return cmp;
                }
            }
            match (
                self.stable_type_order_name(*a),
                self.stable_type_order_name(*b),
            ) {
                (Some(left), Some(right)) => left.cmp(&right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        });

        // TypeScript's formatter removes nullable members from their canonical
        // positions, then appends `null` followed by `undefined`.
        let mut ordered = real;
        if has_null {
            ordered.push(TypeId::NULL);
        }
        if has_undefined {
            ordered.push(TypeId::UNDEFINED);
        }

        let mut parts = Vec::with_capacity(ordered.len());
        for type_id in ordered {
            let s = self.composition_member_text(type_id);
            // Parenthesize function/constructor types and conditional types in union position.
            // Conditional types need parens because `extends` binds more tightly than `|`:
            // `A | B extends C ? D : E` parses as `(A | B) extends C ? D : E`.
            // Intersection members need parens so the grouping round-trips
            // unambiguously: an intersection nested in a union (`A & B | C`)
            // must render as `(A & B) | C`. Mirrors `print_intersection`,
            // which parenthesizes nested unions.
            let part = if self.type_needs_parentheses_in_composition(type_id)
                || visitor::conditional_type_id(self.interner, type_id).is_some()
                || visitor::intersection_list_id(self.interner, type_id).is_some()
            {
                format!("({s})")
            } else {
                s.clone()
            };
            if !parts.iter().any(|(_, existing)| existing == &part) {
                parts.push((s, part));
            }
        }

        // If all members were filtered out, the result is `any` (widened)
        if parts.is_empty() {
            return "any".to_string();
        }
        if parts.len() == 1 {
            return parts.remove(0).0;
        }

        // Join with " | "
        parts
            .into_iter()
            .map(|(_, part)| part)
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn stable_type_order_rank(&self, type_id: TypeId) -> u32 {
        ts7_union_sort_rank(self.interner, type_id, &|def_id| {
            self.type_cache
                .and_then(|cache| cache.def_types.get(&def_id).copied())
        })
    }

    fn stable_type_order_literal(&self, type_id: TypeId) -> Option<LiteralValue> {
        ts7_sort_literal(self.interner, type_id, &|def_id| {
            self.type_cache
                .and_then(|cache| cache.def_types.get(&def_id).copied())
        })
    }

    fn stable_template_order_text(&self, type_id: TypeId) -> Option<Vec<String>> {
        let list_id = visitor::template_literal_id(self.interner, type_id)?;
        let spans = self.interner.template_list(list_id);
        Some(
            spans
                .iter()
                .filter_map(|span| {
                    span.as_text()
                        .map(|atom| self.interner.resolve_atom_ref(atom).to_string())
                })
                .collect(),
        )
    }

    fn stable_type_order_name(&self, type_id: TypeId) -> Option<String> {
        match ts7_sort_name_source(self.interner, type_id)? {
            Ts7SortNameSource::Def(def_id) => {
                if let Some(name) = self
                    .type_cache
                    .and_then(|cache| cache.def_to_name.get(&def_id))
                {
                    return Some(name.clone());
                }
                let symbol = self
                    .type_cache
                    .and_then(|cache| cache.def_to_symbol.get(&def_id))?;
                self.symbol_arena
                    .and_then(|arena| arena.get(*symbol))
                    .map(|symbol| symbol.escaped_name.to_string())
            }
            Ts7SortNameSource::Symbol(symbol) => self
                .symbol_arena
                .and_then(|arena| arena.get(symbol))
                .map(|symbol| symbol.escaped_name.to_string()),
            Ts7SortNameSource::Atom(atom) => Some(self.interner.resolve_atom_ref(atom).to_string()),
        }
    }

    pub(crate) fn try_print_enum_member_union_as_parent(&self, types: &[TypeId]) -> Option<String> {
        let cache = self.type_cache?;
        let symbols = self.symbol_arena?;
        let mut parent: Option<SymbolId> = None;
        for &type_id in types {
            let (def_id, _) = visitor::enum_components(self.interner, type_id)?;
            let member_sym = cache.def_to_symbol.get(&def_id).copied()?;
            let member = symbols.get(member_sym)?;
            if member.flags & symbol_flags::ENUM_MEMBER == 0 || !member.parent.is_some() {
                return None;
            }
            if let Some(existing) = parent {
                if existing != member.parent {
                    return None;
                }
            } else {
                parent = Some(member.parent);
            }
        }
        let parent = parent?;
        let parent_symbol = symbols.get(parent)?;
        let exports = parent_symbol.exports.as_ref()?;
        let enum_member_count = exports
            .iter()
            .filter(|(_, sym_id)| {
                symbols
                    .get(**sym_id)
                    .is_some_and(|symbol| symbol.flags & symbol_flags::ENUM_MEMBER != 0)
            })
            .count();
        if enum_member_count != types.len() {
            return None;
        }

        self.print_named_symbol_reference(parent, false)
    }

    pub(crate) fn print_intersection(&self, type_list_id: tsz_solver::types::TypeListId) -> String {
        let original_types = self.interner.type_list(type_list_id);
        let mut filtered_types = Vec::new();
        let types = if self.intersection_should_drop_global_object(&original_types) {
            filtered_types.extend(original_types.iter().copied().filter(|&member| {
                !self.is_registered_boxed_type(member, tsz_solver::types::IntrinsicKind::Object)
            }));
            filtered_types.as_slice()
        } else {
            original_types.as_ref()
        };
        if types.is_empty() {
            return "unknown".to_string(); // Intersection of 0 types is unknown
        }

        // Recover `NonNullable<T>` from a 2-member intersection of a type
        // parameter and `{}`. tsc's narrowing of a type-parameter-typed
        // value through truthy guards produces `T & {}` but tags it with
        // the `NonNullable<T>` alias so users see the meaningful name.
        // tsz's narrower constructs the intersection without storing the
        // alias on every code path, so apply the same shape detection at
        // print time (the diagnostic compound formatter already does
        // this for TS2322 messages).
        if types.len() == 2 {
            let is_type_param_like = |type_id: tsz_solver::types::TypeId| {
                visitor::type_param_info(self.interner, type_id).is_some()
            };
            let is_empty_object = |type_id: tsz_solver::types::TypeId| {
                if type_id.is_intrinsic() {
                    return false;
                }
                visitor::object_shape_id(self.interner, type_id)
                    .map(|shape_id| self.interner.object_shape(shape_id))
                    .is_some_and(|shape| {
                        shape.properties.is_empty()
                            && shape.string_index.is_none()
                            && shape.number_index.is_none()
                            && shape.symbol.is_none()
                    })
            };
            let (a, b) = (types[0], types[1]);
            let pair = if is_type_param_like(a) && is_empty_object(b) {
                Some(a)
            } else if is_type_param_like(b) && is_empty_object(a) {
                Some(b)
            } else {
                None
            };
            if let Some(t) = pair {
                return format!("NonNullable<{}>", self.print_type(t));
            }
        }

        let mut members: Vec<(u8, String)> = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            let s = self.composition_member_text(type_id);
            // Parenthesize function/constructor types, union types, and conditional types
            // in intersection position.
            // Union types need parens because `&` binds tighter than `|`:
            // `(A | B) & C` is different from `A | B & C`.
            // Conditional types need parens for the same precedence reason.
            let needs_parens = self.type_needs_parentheses_in_composition(type_id)
                || visitor::union_list_id(self.interner, type_id).is_some()
                || visitor::conditional_type_id(self.interner, type_id).is_some();
            if needs_parens {
                members.push((self.intersection_member_priority(type_id), format!("({s})")));
            } else {
                members.push((self.intersection_member_priority(type_id), s));
            }
        }
        members.sort_by_key(|(priority, _)| *priority);

        // Join with " & "
        members
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join(" & ")
    }

    fn intersection_should_drop_global_object(&self, types: &[TypeId]) -> bool {
        let has_global_object = types.iter().copied().any(|member| {
            self.is_registered_boxed_type(member, tsz_solver::types::IntrinsicKind::Object)
        });
        if !has_global_object {
            return false;
        }

        types
            .iter()
            .copied()
            .any(|member| self.has_primitive_declaration_surface(member))
    }

    fn is_registered_boxed_type(
        &self,
        type_id: TypeId,
        kind: tsz_solver::types::IntrinsicKind,
    ) -> bool {
        self.interner.get_boxed_type(kind) == Some(type_id)
            || visitor::lazy_def_id(self.interner, type_id)
                .is_some_and(|def_id| self.interner.is_boxed_def_id(def_id, kind))
    }

    fn has_primitive_declaration_surface(&self, type_id: TypeId) -> bool {
        matches!(
            visitor::apparent_intrinsic_kind(self.interner, type_id),
            Some(
                tsz_solver::types::IntrinsicKind::String
                    | tsz_solver::types::IntrinsicKind::Number
                    | tsz_solver::types::IntrinsicKind::Boolean
                    | tsz_solver::types::IntrinsicKind::Bigint
                    | tsz_solver::types::IntrinsicKind::Symbol
            )
        )
    }

    pub(crate) fn print_recursive_expansion_limit(&self, return_type: TypeId) -> String {
        let Some(type_list_id) = visitor::intersection_list_id(self.interner, return_type) else {
            return crate::ELIDED_ANY.to_string();
        };
        let types = self.interner.type_list(type_list_id);
        let mut members: Vec<(u8, String)> = Vec::with_capacity(types.len());
        for &type_id in types.iter() {
            let (priority, text) = if visitor::function_shape_id(self.interner, type_id).is_some()
                || visitor::callable_shape_id(self.interner, type_id).is_some()
            {
                (0, crate::ELIDED_ANY.to_string())
            } else {
                let text = self.composition_member_text(type_id);
                let needs_parens = self.type_needs_parentheses_in_composition(type_id)
                    || visitor::union_list_id(self.interner, type_id).is_some()
                    || visitor::conditional_type_id(self.interner, type_id).is_some();
                let text = if needs_parens {
                    format!("({text})")
                } else {
                    text
                };
                (self.intersection_member_priority(type_id), text)
            };
            members.push((priority, text));
        }
        members.sort_by_key(|(priority, _)| *priority);
        members
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join(" & ")
    }

    pub(crate) fn rename_recursive_function_type_params_for_depth(
        &self,
        type_id: TypeId,
    ) -> TypeId {
        self.rename_recursive_function_type_params_inner(type_id, 0)
    }

    fn rename_recursive_function_type_params_inner(&self, type_id: TypeId, depth: u8) -> TypeId {
        if depth >= 16 {
            return type_id;
        }

        if let Some(func_id) = visitor::function_shape_id(self.interner, type_id) {
            let func = self.interner.function_shape(func_id);
            if func.type_params.is_empty() {
                return type_id;
            }

            let mut renamed_names = Vec::with_capacity(func.type_params.len());
            let mut renamed_type_ids = Vec::with_capacity(func.type_params.len());
            let mut reserved_names = Vec::with_capacity(func.type_params.len());
            for tp in &func.type_params {
                let name = self.recursive_expansion_type_param_name(tp.name, &reserved_names);
                let placeholder =
                    self.interner
                        .fresh_type_param(tsz_solver::types::TypeParamInfo {
                            name,
                            constraint: None,
                            default: None,
                            is_const: tp.is_const,
                            origin: tsz_solver::types::TypeParamOrigin::User,
                        });
                renamed_type_ids.push(placeholder);
                reserved_names.push(self.interner.resolve_atom(name));
                renamed_names.push(name);
            }

            let owned_names: std::collections::HashSet<_> =
                func.type_params.iter().map(|param| param.name).collect();
            let mut roots = Vec::with_capacity(func.type_params.len() * 2 + func.params.len() + 3);
            for param in &func.type_params {
                roots.extend(param.constraint);
                roots.extend(param.default);
            }
            roots.extend(func.params.iter().map(|param| param.type_id));
            roots.extend(func.this_type);
            roots.push(func.return_type);
            roots.extend(func.type_predicate.and_then(|predicate| predicate.type_id));

            let mut from = Vec::new();
            let mut to = Vec::new();
            for (index, param) in func.type_params.iter().enumerate() {
                let canonical = self.interner.type_param(*param);
                from.push(canonical);
                to.push(renamed_type_ids[index]);
            }
            for (_, occurrence) in free_type_params_named(self.interner, roots, &owned_names) {
                let Some(info) = visitor::type_param_info(self.interner, occurrence) else {
                    continue;
                };
                let Some(index) = func
                    .type_params
                    .iter()
                    .position(|param| param.is_same_binder(info))
                else {
                    continue;
                };
                if !from.contains(&occurrence) {
                    from.push(occurrence);
                    to.push(renamed_type_ids[index]);
                }
            }

            let rewritten_type = substitute_exact_types(self.interner, type_id, &from, &to);
            let Some(rewritten_func_id) = visitor::function_shape_id(self.interner, rewritten_type)
            else {
                return type_id;
            };
            let rewritten = self.interner.function_shape(rewritten_func_id);

            let type_params = rewritten
                .type_params
                .iter()
                .zip(renamed_names)
                .map(|(tp, name)| tsz_solver::types::TypeParamInfo {
                    name,
                    constraint: tp.constraint,
                    default: tp.default,
                    is_const: tp.is_const,
                    origin: tsz_solver::types::TypeParamOrigin::User,
                })
                .collect();

            return self.interner.function(tsz_solver::types::FunctionShape {
                type_params,
                params: rewritten.params.clone(),
                this_type: rewritten.this_type,
                return_type: rewritten.return_type,
                type_predicate: rewritten.type_predicate,
                is_constructor: rewritten.is_constructor,
                is_method: rewritten.is_method,
            });
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, type_id) {
            let mut shape = (*self.interner.object_shape(shape_id)).clone();
            for prop in &mut shape.properties {
                let read_type = prop.type_id;
                let write_type = prop.write_type;
                prop.type_id =
                    self.rename_recursive_function_type_params_inner(read_type, depth + 1);
                prop.write_type = if write_type == read_type {
                    prop.type_id
                } else {
                    self.rename_recursive_function_type_params_inner(write_type, depth + 1)
                };
            }
            if let Some(index) = &mut shape.string_index {
                index.value_type =
                    self.rename_recursive_function_type_params_inner(index.value_type, depth + 1);
            }
            if let Some(index) = &mut shape.number_index {
                index.value_type =
                    self.rename_recursive_function_type_params_inner(index.value_type, depth + 1);
            }
            return self
                .interner
                .object_with_flags_and_symbol(shape.properties, shape.flags, None);
        }

        if let Some(shape_id) = visitor::object_with_index_shape_id(self.interner, type_id) {
            let mut shape = (*self.interner.object_shape(shape_id)).clone();
            for prop in &mut shape.properties {
                let read_type = prop.type_id;
                let write_type = prop.write_type;
                prop.type_id =
                    self.rename_recursive_function_type_params_inner(read_type, depth + 1);
                prop.write_type = if write_type == read_type {
                    prop.type_id
                } else {
                    self.rename_recursive_function_type_params_inner(write_type, depth + 1)
                };
            }
            if let Some(index) = &mut shape.string_index {
                index.value_type =
                    self.rename_recursive_function_type_params_inner(index.value_type, depth + 1);
            }
            if let Some(index) = &mut shape.number_index {
                index.value_type =
                    self.rename_recursive_function_type_params_inner(index.value_type, depth + 1);
            }
            return self.interner.object_with_index(shape);
        }

        if let Some(type_list_id) = visitor::intersection_list_id(self.interner, type_id) {
            let members = self
                .interner
                .type_list(type_list_id)
                .iter()
                .map(|&member| self.rename_recursive_function_type_params_inner(member, depth + 1))
                .collect();
            return self.interner.intersection(members);
        }

        type_id
    }

    fn recursive_expansion_type_param_name(&self, name: Atom, reserved: &[String]) -> Atom {
        let base = self.interner.resolve_atom(name);
        if !self.type_param_scope_contains_name(&base) && !reserved.iter().any(|name| name == &base)
        {
            return name;
        }
        let mut suffix = 1u32;
        loop {
            let candidate = format!("{base}_{suffix}");
            if !self.type_param_scope_contains_name(&candidate)
                && !reserved.iter().any(|name| name == &candidate)
            {
                return self.interner.intern_string(&candidate);
            }
            suffix += 1;
        }
    }

    pub(crate) fn print_tuple(&self, tuple_id: tsz_solver::types::TupleListId) -> String {
        let elements = self.interner.tuple_list(tuple_id);

        if elements.is_empty() {
            return "[]".to_string();
        }

        let mut parts = Vec::with_capacity(elements.len());
        for elem in elements.iter() {
            let mut part = String::new();

            // Handle labeled tuple members (e.g., [name: string])
            if let Some(name) = elem.name {
                part.push_str(&self.resolve_atom(name));
                // Optional marker comes after the label for labeled tuples
                if elem.optional {
                    part.push('?');
                }
                part.push_str(": ");
            }

            // Rest parameter prefix
            if elem.rest {
                part.push_str("...");
                // For unlabeled rest+optional elements, tsc places ? before the type: [...?T]
                if elem.name.is_none() && elem.optional {
                    part.push('?');
                }
            }

            // Type annotation
            part.push_str(&self.print_tuple_element_type(elem.type_id, elem.rest));

            // Optional marker for unlabeled non-rest tuples (comes after type): [T?]
            if elem.name.is_none() && elem.optional && !elem.rest {
                part.push('?');
            }

            parts.push(part);
        }

        format!("[{}]", parts.join(", "))
    }

    fn print_tuple_element_type(&self, type_id: TypeId, is_rest: bool) -> String {
        if is_rest
            && let Some(param_info) = visitor::type_param_info(self.interner, type_id)
            && !visitor::is_infer_type(self.interner, type_id)
        {
            return self.print_type_parameter(&param_info);
        }

        self.print_type(type_id)
    }

    pub(crate) fn print_function_type(
        &self,
        func_id: tsz_solver::types::FunctionShapeId,
    ) -> String {
        let func_shape = self.interner.function_shape(func_id);
        let scoped = self.with_type_param_scope(&func_shape.type_params);
        let type_params_str = if !func_shape.type_params.is_empty() {
            let params: Vec<String> = func_shape
                .type_params
                .iter()
                .map(|tp| scoped.print_type_parameter_decl(tp))
                .collect();
            format!("<{}>", params.join(", "))
        } else {
            String::new()
        };

        // Parameters
        let display_params = self
            .interner
            .display_params_for_function_shape(func_id, &func_shape.params);
        let mut params = Vec::new();
        if let Some(this_type) = func_shape.this_type {
            params.push(format!("this: {}", scoped.print_type(this_type)));
        }
        for param in display_params.iter() {
            let mut param_str = String::new();

            // Rest parameter
            if param.rest {
                param_str.push_str("...");
            }

            // Parameter name (optional in function types)
            if let Some(name) = param.name {
                param_str.push_str(&scoped.resolve_atom(name));
                if param.optional {
                    param_str.push('?');
                }
                param_str.push_str(": ");
            }

            if param.optional {
                param_str.push_str(&scoped.print_optional_param_type(param.type_id));
            } else {
                param_str.push_str(&scoped.print_type(param.type_id));
            }

            params.push(param_str);
        }

        let return_str = if let Some(ref pred) = func_shape.type_predicate {
            scoped.print_type_predicate(pred)
        } else {
            let inner = scoped.print_type(func_shape.return_type);
            // tsc parenthesises a conditional return when emitting an
            // arrow-form callable so the printed text round-trips
            // unambiguously through the parser even when nested inside
            // a larger conditional or extends position (the outer
            // conditional's `? : ` would otherwise capture the inner's
            // `? :`).  Mirror that here.
            if tsz_solver::is_conditional_type(self.interner, func_shape.return_type) {
                format!("({inner})")
            } else {
                inner
            }
        };

        format!(
            "{}({}) => {}",
            type_params_str,
            params.join(", "),
            return_str
        )
    }

    pub(crate) fn print_callable(&self, callable_id: tsz_solver::types::CallableShapeId) -> String {
        let callable = self.interner.callable_shape(callable_id);

        // For class constructor types with a visible symbol, use `typeof ClassName` form.
        // This matches tsc's behavior for declaration emit.
        if !callable.construct_signatures.is_empty()
            && let Some(sym_id) = callable.symbol
            && (self.is_symbol_visible(sym_id) || self.symbol_is_nameable(sym_id))
            && let Some(name) = self.resolve_symbol_qualified_name(sym_id)
        {
            return format!("typeof {name}");
        }

        // Simple callable: one call signature, no properties/construct/index sigs
        // → use arrow function syntax: (params) => ReturnType
        let has_properties = callable
            .properties
            .iter()
            .any(|property| !self.property_is_hidden_in_declaration_shape(property));
        if callable.call_signatures.len() == 1
            && callable.construct_signatures.is_empty()
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
        {
            return self.print_call_signature_arrow(&callable.call_signatures[0]);
        }

        // Simple constructor callable: one construct signature, no other members
        // → use `new (...) => T` (or `abstract new (...) => T`) syntax. This matches
        // tsc's declaration-emit form for `new (...args) => T` written explicitly
        // as a constructor type in source (e.g. `Record<string, new (...) => T>`
        // or in the extends clause of a conditional). For anonymous and named
        // class constructor types (which carry `symbol: Some(_)`), tsc keeps
        // the structural `{ new (): T }` object-literal form, so we leave those
        // to fall through to the multi-line rendering below.
        if callable.symbol.is_none()
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
        {
            return self.print_construct_signature_arrow(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }
        // Abstract constructor callables historically used the arrow form even
        // when they carried a synthetic class symbol; preserve that to avoid
        // regressing `abstract new () => { ... }` cases.
        if callable.is_abstract
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !has_properties
            && callable.string_index.is_none()
            && callable.number_index.is_none()
        {
            return self.print_construct_signature_arrow(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
        }

        let mut signature_parts = Vec::new();
        for sig in &callable.call_signatures {
            signature_parts.push(self.print_call_signature(sig, false, false));
        }
        for sig in &callable.construct_signatures {
            signature_parts.push(self.print_call_signature(sig, true, callable.is_abstract));
        }

        let mut member_parts = Vec::new();

        // Add index signatures (tsc emits these before properties).
        if let Some(ref idx) = callable.number_index {
            let readonly = if idx.readonly { "readonly " } else { "" };
            let param = idx
                .param_name
                .map(|a| self.resolve_atom(a))
                .unwrap_or_else(|| "x".to_string());
            let widened = self.widen_synthesized_method_return_type(idx.value_type);
            member_parts.push(format!(
                "{}[{}: number]: {}",
                readonly,
                param,
                self.print_type(widened)
            ));
        }
        if let Some(ref idx) = callable.string_index {
            let readonly = if idx.readonly { "readonly " } else { "" };
            let param = idx
                .param_name
                .map(|a| self.resolve_atom(a))
                .unwrap_or_else(|| "x".to_string());
            let widened = self.widen_synthesized_method_return_type(idx.value_type);
            member_parts.push(format!(
                "{}[{}: string]: {}",
                readonly,
                param,
                self.print_type(widened)
            ));
        }

        // Add properties (filter out internal props tsc strips from .d.ts)
        for prop in &callable.properties {
            if self.property_is_hidden_in_declaration_shape(prop) {
                continue;
            }

            // Try to emit as method syntax if the property is a method
            if prop.is_method
                && let Some(method_str) = self.print_property_as_method(prop, callable.symbol)
            {
                member_parts.push(method_str);
                continue;
            }

            if let Some(accessors) = self.print_property_as_accessors(prop) {
                member_parts.extend(accessors);
                continue;
            }

            let readonly = if prop.readonly { "readonly " } else { "" };
            let optional = if prop.optional { "?" } else { "" };
            member_parts.push(format!(
                "{}{}{}: {}",
                readonly,
                self.declaration_property_name_text(prop),
                optional,
                self.print_type(prop.type_id)
            ));
        }

        if callable.is_abstract
            && callable.call_signatures.is_empty()
            && callable.construct_signatures.len() == 1
            && !member_parts.is_empty()
        {
            let constructor_type = self.print_construct_signature_arrow(
                &callable.construct_signatures[0],
                callable.is_abstract,
            );
            let member_type = self.format_type_literal_parts(&member_parts);
            return format!("({constructor_type}) & {member_type}");
        }

        let mut parts = signature_parts;
        parts.extend(member_parts);

        if parts.is_empty() {
            return "{}".to_string();
        }

        self.format_type_literal_parts(&parts)
    }
}
