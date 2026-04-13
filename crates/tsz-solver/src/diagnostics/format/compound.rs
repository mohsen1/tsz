//! Compound type formatting methods for `TypeFormatter`.

use super::TypeFormatter;
use super::needs_property_name_quotes;
use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, LiteralValue, MappedModifier,
    MappedType, ObjectShape, ParamInfo, PropertyInfo, SymbolRef, TemplateSpan, TupleElement,
    TypeData, TypeId, TypeParamInfo,
};
use std::borrow::Cow;
use tsz_binder::SymbolId;

impl<'a> TypeFormatter<'a> {
    pub(super) fn format_literal(&mut self, lit: &LiteralValue) -> String {
        match lit {
            LiteralValue::String(s) => {
                let raw = self.atom(*s);
                let escaped = raw
                    .replace('\\', "\\\\")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                format!("\"{escaped}\"")
            }
            LiteralValue::Number(n) => {
                let v = n.0;
                if v.is_infinite() {
                    if v.is_sign_positive() {
                        "Infinity".to_string()
                    } else {
                        "-Infinity".to_string()
                    }
                } else if v.is_nan() {
                    "NaN".to_string()
                } else {
                    format!("{v}")
                }
            }
            LiteralValue::BigInt(b) => format!("{}n", self.atom(*b)),
            LiteralValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
        }
    }

    pub(super) fn format_object(&mut self, props: &[PropertyInfo]) -> String {
        if props.is_empty() {
            return "{}".to_string();
        }
        let mut display_props: Vec<&PropertyInfo> = props.iter().collect();
        // Sort properties for display. Use declaration_order as primary key when
        // available, with tsc-compatible tiebreaking: numeric keys in numeric order,
        // then string keys in existing order (stable sort preserves Atom ID order).
        // Properties are stored sorted by Atom ID for identity/hashing, so display
        // order must be restored here.
        display_props.sort_by(|a, b| {
            // Primary: declaration_order (0 means unset, treated as equal)
            let ord = a.declaration_order.cmp(&b.declaration_order);
            if ord != std::cmp::Ordering::Equal
                && a.declaration_order > 0
                && b.declaration_order > 0
            {
                return ord;
            }
            // Tiebreak for properties with same declaration_order:
            // numeric keys get sorted numerically (tsc puts them first),
            // but string keys preserve their existing order via stable sort.
            let a_name = self.interner.resolve_atom_ref(a.name);
            let b_name = self.interner.resolve_atom_ref(b.name);
            let a_num = a_name.parse::<u64>();
            let b_num = b_name.parse::<u64>();
            match (a_num, b_num) {
                (Ok(an), Ok(bn)) => an.cmp(&bn),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                // For non-numeric keys with same decl_order, preserve existing
                // order (stable sort) — Atom ID order often matches source order
                (Err(_), Err(_)) => std::cmp::Ordering::Equal,
            }
        });
        // tsc does not truncate object properties in error messages — it uses
        // NoTruncation for diagnostics.  Only truncate when displaying extremely
        // large objects (>= 10 props) to prevent pathological output.
        if display_props.len() >= 10 {
            let first: Vec<String> = display_props
                .iter()
                .take(8)
                .map(|p| self.format_property(p))
                .collect();
            return format!("{{ {}; ...; }}", first.join("; "));
        }
        let formatted: Vec<String> = display_props
            .iter()
            .map(|p| self.format_property(p))
            .collect();
        format!("{{ {}; }}", formatted.join("; "))
    }

    pub(super) fn format_property(&mut self, prop: &PropertyInfo) -> String {
        let optional = if prop.optional { "?" } else { "" };
        let readonly = if prop.readonly { "readonly " } else { "" };
        let raw_name = self.atom(prop.name);
        let name = if needs_property_name_quotes(&raw_name) {
            // tsc uses double quotes for JSX-specific property names
            // (namespace-prefixed like "ns:attr" and data attributes like "data-foo")
            // but single quotes for all other quoted property names.
            let use_double = raw_name.contains(':')
                || raw_name.starts_with("data-")
                || raw_name.chars().next().is_some_and(|c| c.is_ascii_digit());
            if use_double {
                let escaped = raw_name.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{escaped}\"")
            } else {
                let escaped = raw_name.replace('\\', "\\\\").replace('\'', "\\'");
                format!("'{escaped}'")
            }
        } else {
            raw_name.to_string()
        };

        // Method shorthand: `name(params): return_type` instead of `name: (params) => return_type`
        if prop.is_method {
            match self.interner.lookup(prop.type_id) {
                Some(TypeData::Function(f_id)) => {
                    let shape = self.interner.function_shape(f_id);
                    let type_params = self.format_type_params(&shape.type_params);
                    let params = self.format_params(&shape.params, shape.this_type);
                    let return_str = self.format(shape.return_type);
                    return format!(
                        "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                        params = params.join(", ")
                    );
                }
                Some(TypeData::Callable(callable_id)) => {
                    let shape = self.interner.callable_shape(callable_id);
                    if let Some(sig) = shape.call_signatures.first() {
                        let type_params = self.format_type_params(&sig.type_params);
                        let params = self.format_params(&sig.params, sig.this_type);
                        let return_str = self.format(sig.return_type);
                        return format!(
                            "{readonly}{name}{optional}{type_params}({params}): {return_str}",
                            params = params.join(", ")
                        );
                    }
                }
                _ => {}
            }
        }

        // tsc displays optional object properties WITH `| undefined`:
        // `n?: number | undefined`. If the stored type doesn't already contain
        // undefined, we append it.
        let type_str: String = if prop.optional {
            let formatted = self.format(prop.type_id).into_owned();
            if self.preserve_optional_property_surface_syntax {
                formatted
            } else if prop.type_id == TypeId::NEVER {
                // `never | undefined` simplifies to `undefined`; tsc displays just `undefined`
                "undefined".to_string()
            } else if !self.type_contains_undefined(prop.type_id) {
                format!("{formatted} | undefined")
            } else {
                formatted
            }
        } else {
            self.format(prop.type_id).into_owned()
        };
        format!("{readonly}{name}{optional}: {type_str}")
    }

    /// Check if a type already contains `undefined` (as a union member or is undefined itself).
    /// Also treats `any` and `unknown` as absorbing undefined, since `any | undefined` == `any`
    /// and `unknown | undefined` == `unknown` in tsc's display.
    fn type_contains_undefined(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED || type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
            return true;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return members
                .iter()
                .any(|&m| m == TypeId::UNDEFINED || m == TypeId::ANY || m == TypeId::UNKNOWN);
        }
        false
    }

    /// Format a type while stripping `undefined` from it.
    /// Used for optional tuple elements where the `?` already implies optionality.
    fn format_stripping_undefined(&mut self, type_id: TypeId) -> String {
        if type_id == TypeId::UNDEFINED {
            // Edge case: type is just `undefined` — display it as-is since
            // there's nothing else to show.
            return self.format(type_id).into_owned();
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            let filtered: Vec<TypeId> = members
                .iter()
                .copied()
                .filter(|&m| m != TypeId::UNDEFINED)
                .collect();
            if filtered.len() < members.len() {
                // We stripped some undefined members
                return match filtered.len() {
                    0 => self.format(TypeId::NEVER).into_owned(),
                    1 => self.format(filtered[0]).into_owned(),
                    _ => self.format_union(&filtered),
                };
            }
        }
        self.format(type_id).into_owned()
    }

    pub(super) fn format_type_params(&mut self, type_params: &[TypeParamInfo]) -> String {
        if type_params.is_empty() {
            return String::new();
        }

        let mut parts = Vec::with_capacity(type_params.len());
        for tp in type_params {
            let mut part = String::new();
            if tp.is_const {
                part.push_str("const ");
            }
            part.push_str(self.atom(tp.name).as_ref());
            if let Some(constraint) = tp.constraint {
                part.push_str(" extends ");
                // tsc preserves declared generic form in constraints
                let prev = self.preserve_array_generic_form;
                self.preserve_array_generic_form = true;
                part.push_str(&self.format(constraint));
                self.preserve_array_generic_form = prev;
            }
            if let Some(default) = tp.default {
                part.push_str(" = ");
                part.push_str(&self.format(default));
            }
            parts.push(part);
        }

        format!("<{}>", parts.join(", "))
    }

    pub(super) fn format_params(
        &mut self,
        params: &[ParamInfo],
        this_type: Option<TypeId>,
    ) -> Vec<String> {
        let mut rendered = Vec::with_capacity(params.len() + usize::from(this_type.is_some()));

        if let Some(this_ty) = this_type {
            rendered.push(format!("this: {}", self.format(this_ty)));
        }

        for p in params {
            let name = p
                .name
                .map_or_else(|| "_".to_string(), |atom| self.atom(atom).to_string());
            let optional = if p.optional { "?" } else { "" };
            let rest = if p.rest { "..." } else { "" };
            let type_str: String = self.format(p.type_id).into_owned();
            rendered.push(format!("{rest}{name}{optional}: {type_str}"));
        }

        rendered
    }

    /// Format a signature with the given separator between params and return type.
    pub(super) fn format_signature(
        &mut self,
        type_params: &[TypeParamInfo],
        params: &[ParamInfo],
        this_type: Option<TypeId>,
        return_type: TypeId,
        is_construct: bool,
        is_abstract: bool,
        separator: &str,
    ) -> String {
        self.format_signature_with_predicate(
            type_params,
            params,
            this_type,
            return_type,
            None,
            is_construct,
            is_abstract,
            separator,
        )
    }

    /// Format a signature including an optional type predicate in the return type.
    ///
    /// When `type_predicate` is `Some`, the return type is formatted as
    /// `asserts v is T` or `v is T` instead of the raw return type.
    /// This matches tsc's display for assertion/type guard functions.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn format_signature_with_predicate(
        &mut self,
        type_params: &[TypeParamInfo],
        params: &[ParamInfo],
        this_type: Option<TypeId>,
        return_type: TypeId,
        type_predicate: Option<&crate::types::TypePredicate>,
        is_construct: bool,
        is_abstract: bool,
        separator: &str,
    ) -> String {
        let prefix = if is_construct && is_abstract {
            "abstract new "
        } else if is_construct {
            "new "
        } else {
            ""
        };
        let type_params = self.format_type_params(type_params);
        let params = self.format_params(params, this_type);
        let return_str: Cow<'static, str> = if let Some(pred) = type_predicate {
            let target_name = match pred.target {
                crate::types::TypePredicateTarget::This => "this".to_string(),
                crate::types::TypePredicateTarget::Identifier(atom) => self.atom(atom).to_string(),
            };
            let type_part = pred.type_id.map(|tid| format!(" is {}", self.format(tid)));
            if pred.asserts {
                Cow::Owned(format!(
                    "asserts {}{}",
                    target_name,
                    type_part.unwrap_or_default()
                ))
            } else {
                Cow::Owned(format!("{}{}", target_name, type_part.unwrap_or_default()))
            }
        } else if is_construct && return_type == TypeId::UNKNOWN {
            Cow::Borrowed("any")
        } else {
            self.format(return_type)
        };
        format!(
            "{}{}({}){} {}",
            prefix,
            type_params,
            params.join(", "),
            separator,
            return_str
        )
    }

    pub(super) fn format_object_with_index(&mut self, shape: &ObjectShape) -> String {
        let mut parts = Vec::new();

        if let Some(ref idx) = shape.string_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{ro}[{key_name}: string]: {}",
                self.format(idx.value_type)
            ));
        }
        if let Some(ref idx) = shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{ro}[{key_name}: number]: {}",
                self.format(idx.value_type)
            ));
        }
        // Sort properties by declaration_order for display (preserves source order)
        let mut display_props: Vec<&PropertyInfo> = shape.properties.iter().collect();
        let has_decl_order = display_props.iter().any(|p| p.declaration_order > 0);
        if has_decl_order {
            display_props.sort_by_key(|p| p.declaration_order);
        }
        for prop in display_props {
            parts.push(self.format_property(prop));
        }

        if parts.is_empty() {
            return "{}".to_string();
        }

        format!("{{ {}; }}", parts.join("; "))
    }

    pub(super) fn format_union(&mut self, members: &[TypeId]) -> String {
        // tsc displays union members with null/undefined at the end.
        // Reorder so non-nullish members come first, then null, then undefined.
        let mut ordered: Vec<TypeId> = Vec::with_capacity(members.len());
        let mut has_null = false;
        let mut has_undefined = false;
        for &m in members {
            if m == TypeId::NULL {
                has_null = true;
            } else if m == TypeId::UNDEFINED {
                has_undefined = true;
            } else {
                ordered.push(m);
            }
        }
        // Sort non-nullish members by source position to match tsc's display order.
        // The interner sorts Lazy types by DefId.0 (allocation order), which doesn't
        // match source declaration order. Display-time sorting fixes this for diagnostics.
        //
        // Sorting rules:
        // - Tier 0/1 (builtins/user types with source): always sort these
        // - Tier 2 objects only (all anonymous objects): sort by property count
        // - Mixed tier 2 or non-object tier 2: preserve original order
        if let Some(def_store) = self.def_store {
            let positions: Vec<_> = ordered
                .iter()
                .map(|&m| self.get_source_position_for_type(m, def_store))
                .collect();

            // Check if we should sort: either all have positions (tier < 2),
            // or all are tier 2 objects (which have property count as sort key)
            let all_tier_0_or_1 = positions.iter().all(|&(tier, _, _)| tier < 2);
            let all_tier_2_objects = positions.iter().all(|&(tier, second, _)| {
                // Tier 2 objects have second=0, non-objects have second=u32::MAX
                tier == 2 && second == 0
            });

            if all_tier_0_or_1 || all_tier_2_objects {
                let mut pairs: Vec<_> = ordered.iter().copied().zip(positions).collect();
                pairs.sort_by_key(|&(_, pos)| pos);
                ordered = pairs.into_iter().map(|(id, _)| id).collect();
            }
        }

        if has_null {
            ordered.push(TypeId::NULL);
        }
        if has_undefined {
            ordered.push(TypeId::UNDEFINED);
        }

        if !self.skip_union_optionalize
            && let Some(normalized) = self.optionalize_object_union_members_for_display(&ordered)
        {
            ordered = normalized;
        }

        if let Some(collapsed) = self.collapse_same_enum_members_for_display(&ordered) {
            return collapsed;
        }

        if ordered.len() > self.max_union_members {
            let first: Vec<String> = ordered
                .iter()
                .take(self.max_union_members)
                .map(|&m| self.format_union_member(m))
                .collect();
            return format!("{} | ...", first.join(" | "));
        }
        let formatted: Vec<String> = ordered
            .iter()
            .map(|&m| self.format_union_member(m))
            .collect();
        // Disambiguate duplicate display names by adding namespace qualification.
        // tsc shows `Foo.Yep | Bar.Yep` instead of `Yep | Yep` when two different
        // types share the same name in different namespaces.
        let disambiguated = self.disambiguate_union_member_names(&ordered, formatted);
        disambiguated.join(" | ")
    }

    fn collapse_same_enum_members_for_display(&mut self, members: &[TypeId]) -> Option<String> {
        if members.len() < 2 {
            return None;
        }

        let mut rendered = Vec::with_capacity(members.len());
        let mut shared_enum_name: Option<String> = None;
        let mut saw_enum_member = false;
        let mut enum_member_count = 0usize;
        // Track bare literals that weren't recognized as enum members.
        // After narrowing, the union may contain plain Literal types alongside
        // Enum types from the same parent (e.g., `Literal(1) | Enum(E1.b)`)
        // or alongside the full enum type (e.g., `Literal(1) | Literal(2) | Enum(E1)`).
        let mut unresolved_literals: Vec<TypeId> = Vec::new();
        // Track a recognized enum member's DefId so we can look up the parent.
        let mut any_enum_member_def_id: Option<crate::def::DefId> = None;
        // Track a full (parent-level) enum for case where the union contains
        // the enum type itself alongside bare literals.
        let mut parent_enum_structural_type: Option<TypeId> = None;
        let mut parent_enum_name: Option<String> = None;

        for &member in members {
            if member == TypeId::NULL || member == TypeId::UNDEFINED {
                rendered.push(self.format_union_member(member));
                continue;
            }

            // First try: recognize as an enum member (has ENUM_MEMBER flag).
            if let Some(enum_name) = self.enum_member_parent_name_for_display(member) {
                saw_enum_member = true;
                enum_member_count += 1;
                if any_enum_member_def_id.is_none() {
                    any_enum_member_def_id =
                        crate::type_queries::get_enum_def_id(self.interner, member);
                }
                match shared_enum_name.as_ref() {
                    Some(existing) if existing == &enum_name => {}
                    Some(_) => return None,
                    None => {
                        shared_enum_name = Some(enum_name.clone());
                        rendered.push(enum_name);
                    }
                }
                continue;
            }

            // Second try: recognize as a full (parent-level) enum type.
            if let Some(info) = self.enum_parent_name_for_display(member) {
                saw_enum_member = true;
                enum_member_count += 1;
                parent_enum_structural_type = Some(info.1);
                let enum_name = info.2;
                match shared_enum_name.as_ref() {
                    Some(existing) if existing == &enum_name => {}
                    Some(_) => return None,
                    None => {
                        parent_enum_name = Some(enum_name.clone());
                        shared_enum_name = Some(enum_name.clone());
                        rendered.push(enum_name);
                    }
                }
                continue;
            }

            // Check if this is a bare literal — it may be an enum member value
            // that lost its Enum wrapper during narrowing.
            if matches!(self.interner.lookup(member), Some(TypeData::Literal(_))) {
                unresolved_literals.push(member);
            } else {
                rendered.push(self.format_union_member(member));
            }
        }

        // If we have unresolved bare literals and a single shared enum parent,
        // check if those literals correspond to values of the same enum.
        if !unresolved_literals.is_empty() {
            let resolved = if let Some(structural) = parent_enum_structural_type {
                // We have the parent enum's structural type — check literals against it.
                self.literal_values_covered_by_structural_type(structural, &unresolved_literals)
            } else if let Some(member_def) = any_enum_member_def_id {
                // Navigate from member to parent to check.
                self.literal_values_belong_to_enum_of_member(member_def, &unresolved_literals)
            } else {
                false
            };

            if resolved {
                // All bare literals are values from the same enum — count them.
                enum_member_count += unresolved_literals.len();
            } else {
                // Can't collapse: render bare literals normally.
                for &lit in &unresolved_literals {
                    rendered.push(self.format_union_member(lit));
                }
            }
        }

        // If a parent-level enum absorbed all literals, just show the enum name.
        if parent_enum_name.is_some() && enum_member_count > 1 && rendered.len() == 1 {
            return Some(
                rendered
                    .into_iter()
                    .next()
                    .expect("rendered has exactly one element"),
            );
        }

        (saw_enum_member && enum_member_count > 1).then_some(rendered.join(" | "))
    }

    /// Recognize a full (parent-level) enum type — `TypeData::Enum(def_id, structural_type)`
    /// where the symbol has the ENUM flag (not `ENUM_MEMBER`).
    /// Returns `(DefId, structural_type, name)`.
    fn enum_parent_name_for_display(
        &mut self,
        type_id: TypeId,
    ) -> Option<(crate::def::DefId, TypeId, String)> {
        let def_id = crate::type_queries::get_enum_def_id(self.interner, type_id)?;
        let structural_type = crate::type_queries::get_enum_member_type(self.interner, type_id)?;
        let def_store = self.def_store?;
        let def_info = def_store.get(def_id)?;
        let sym_id = def_info.symbol_id?;
        let arena = self.symbol_arena?;
        let symbol = arena.get(SymbolId(sym_id))?;
        use tsz_binder::symbol_flags;
        // This is the enum declaration itself, not a member.
        if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }
        if !symbol.has_any_flags(symbol_flags::ENUM) {
            return None;
        }
        Some((def_id, structural_type, symbol.escaped_name.to_string()))
    }

    /// Check if all bare literal `TypeIds` are covered by the enum's structural type.
    ///
    /// Uses the `structural_type` from `TypeData::Enum(def_id, structural_type)`, which
    /// contains the actual resolved member type IDs (a union of literals or Enum members).
    /// This works even when `DefinitionInfo::enum_members` values are `Computed`.
    fn literal_values_covered_by_structural_type(
        &self,
        structural_type: TypeId,
        literals: &[TypeId],
    ) -> bool {
        // Collect the leaf literal TypeIds from the structural type.
        let structural_members = self.collect_leaf_literal_ids(structural_type);
        if structural_members.is_empty() {
            return false;
        }

        // Each bare literal must match one of the structural type's leaf literals
        // by TypeId equality (guaranteed by interning).
        for &lit_id in literals {
            if !structural_members.contains(&lit_id) {
                return false;
            }
        }
        true
    }

    /// Collect the leaf literal `TypeIds` from a type, recursing through unions
    /// and Enum wrappers to find the actual literal values.
    fn collect_leaf_literal_ids(&self, type_id: TypeId) -> Vec<TypeId> {
        match self.interner.lookup(type_id) {
            Some(TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                let mut result = Vec::new();
                for &m in members.iter() {
                    result.extend(self.collect_leaf_literal_ids(m));
                }
                result
            }
            Some(TypeData::Enum(_, member_type)) => self.collect_leaf_literal_ids(member_type),
            Some(TypeData::Literal(_)) => vec![type_id],
            _ => vec![],
        }
    }

    /// Check if all bare literals belong to the same parent enum as `member_def_id`.
    /// Navigates from a member to its parent enum's structural type.
    fn literal_values_belong_to_enum_of_member(
        &self,
        member_def_id: crate::def::DefId,
        literals: &[TypeId],
    ) -> bool {
        let def_store = match self.def_store {
            Some(ds) => ds,
            None => return false,
        };
        let arena = match self.symbol_arena {
            Some(a) => a,
            None => return false,
        };

        // Navigate: member DefId -> member symbol -> parent symbol -> parent DefId.
        let member_info = match def_store.get(member_def_id) {
            Some(info) => info,
            None => return false,
        };
        let sym_id = match member_info.symbol_id {
            Some(id) => id,
            None => return false,
        };
        let symbol = match arena.get(SymbolId(sym_id)) {
            Some(s) => s,
            None => return false,
        };
        let parent_def_id = match def_store.find_def_by_symbol(symbol.parent.0) {
            Some(id) => id,
            None => return false,
        };

        // Use the parent's body TypeId as the structural type.
        let parent_info = match def_store.get(parent_def_id) {
            Some(info) => info,
            None => return false,
        };
        let structural_type = match parent_info.body {
            Some(body) => body,
            None => return false,
        };
        self.literal_values_covered_by_structural_type(structural_type, literals)
    }

    fn enum_member_parent_name_for_display(&mut self, type_id: TypeId) -> Option<String> {
        let def_id = crate::type_queries::get_enum_def_id(self.interner, type_id)?;
        let def_store = self.def_store?;
        let sym_id = def_store.get(def_id)?.symbol_id?;
        let arena = self.symbol_arena?;
        let symbol = arena.get(SymbolId(sym_id))?;
        use tsz_binder::symbol_flags;
        if !symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }
        let parent = arena.get(symbol.parent)?;
        Some(parent.escaped_name.to_string())
    }

    fn optionalize_object_union_members_for_display(
        &self,
        members: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        let mut object_members = Vec::new();
        let mut suffix = Vec::new();

        for &member in members {
            if member == TypeId::NULL || member == TypeId::UNDEFINED {
                suffix.push(member);
                continue;
            }
            let shape_id = match self.interner.lookup(member) {
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => shape_id,
                _ => return None,
            };
            let shape = self.interner.object_shape(shape_id);
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return None;
            }
            object_members.push((member, shape_id, shape.as_ref().clone()));
        }

        if object_members.len() < 2 {
            return None;
        }

        let mut all_props: Vec<PropertyInfo> = Vec::new();
        for (_, _, shape) in &object_members {
            for prop in &shape.properties {
                if !all_props.iter().any(|existing| existing.name == prop.name) {
                    all_props.push(prop.clone());
                }
            }
        }

        let mut changed = false;
        let mut normalized = Vec::with_capacity(members.len());
        for (_, _, mut shape) in object_members {
            let next_order = shape
                .properties
                .iter()
                .map(|p| p.declaration_order)
                .max()
                .unwrap_or(0)
                + 1;
            let mut append_order = next_order;

            for prop in &all_props {
                if shape
                    .properties
                    .iter()
                    .any(|existing| existing.name == prop.name)
                {
                    continue;
                }
                changed = true;
                let mut synthetic = prop.clone();
                synthetic.type_id = TypeId::UNDEFINED;
                synthetic.write_type = TypeId::UNDEFINED;
                synthetic.optional = true;
                synthetic.readonly = false;
                synthetic.is_method = false;
                synthetic.declaration_order = append_order;
                append_order += 1;
                shape.properties.push(synthetic);
            }

            normalized.push(self.interner.object(shape.properties));
        }

        if !changed {
            return None;
        }

        normalized.extend(suffix);
        Some(normalized)
    }

    /// Format a union member, parenthesizing types that need disambiguation.
    /// TSC parenthesizes intersection types `(A & B) | (C & D)`, function types
    /// `(() => string) | (() => number)`, and constructor types in union positions.
    fn format_union_member(&mut self, id: TypeId) -> String {
        if let Some(enum_name) = self.short_enum_name_for_union_display(id) {
            return enum_name;
        }

        let formatted = self.format(id);
        let needs_parens = match self.interner.lookup(id) {
            Some(TypeData::Intersection(_) | TypeData::Function(_)) => true,
            Some(TypeData::Callable(_)) => {
                formatted.starts_with('(')
                    || formatted.starts_with("new ")
                    || formatted.starts_with("abstract new ")
            }
            _ => false,
        };
        if needs_parens {
            format!("({formatted})")
        } else {
            formatted.into_owned()
        }
    }

    fn short_enum_name_for_union_display(&mut self, type_id: TypeId) -> Option<String> {
        let def_id = crate::type_queries::get_enum_def_id(self.interner, type_id)?;
        let def_store = self.def_store?;
        let sym_id = def_store.get(def_id)?.symbol_id?;
        let arena = self.symbol_arena?;
        let symbol = arena.get(SymbolId(sym_id))?;
        use tsz_binder::symbol_flags;

        if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            let parent = arena.get(symbol.parent)?;
            return Some(format!("{}.{}", parent.escaped_name, symbol.escaped_name));
        }

        if symbol.has_any_flags(symbol_flags::ENUM) {
            return Some(symbol.escaped_name.to_string());
        }

        None
    }

    /// Disambiguate union member display names: when two members format to
    /// the same string (e.g., `Yep | Yep`), try to add namespace qualification
    /// so the output matches tsc (e.g., `Foo.Yep | Bar.Yep`).
    fn disambiguate_union_member_names(
        &mut self,
        members: &[TypeId],
        formatted: Vec<String>,
    ) -> Vec<String> {
        // Count occurrences of each display name
        let mut counts = std::collections::HashMap::<&str, usize>::new();
        for name in &formatted {
            *counts.entry(name.as_str()).or_default() += 1;
        }
        // If no duplicates, return as-is
        if !counts.values().any(|&c| c > 1) {
            return formatted;
        }
        // Try to qualify duplicate names
        let mut result = Vec::with_capacity(formatted.len());
        for (name, &member) in formatted.iter().zip(members.iter()) {
            if counts.get(name.as_str()).copied().unwrap_or(0) > 1
                && let Some(qualified) = self.namespace_qualified_name_for_type(member)
                && qualified != *name
            {
                result.push(qualified);
                continue;
            }
            result.push(name.clone());
        }
        result
    }

    /// Try to get a namespace-qualified name for a type (for disambiguation).
    fn namespace_qualified_name_for_type(&mut self, type_id: TypeId) -> Option<String> {
        // Try object shape symbol
        if let Some(shape) = crate::type_queries::get_object_shape(self.interner, type_id) {
            if let Some(sym_id) = shape.symbol {
                let name = self.format_symbol_name(sym_id)?;
                return Some(self.namespace_qualify_symbol_name(sym_id, name));
            }
            // Try def-store lookup for anonymous object types (type alias bodies)
            if let Some(def_store) = self.def_store
                && let Some(def_id) = def_store.find_def_by_shape(&shape)
                && let Some(def) = def_store.get(def_id)
                && let Some(sym_raw) = def.symbol_id
            {
                let name = self.format_symbol_name(SymbolId(sym_raw))?;
                return Some(self.namespace_qualify_symbol_name(SymbolId(sym_raw), name));
            }
        }
        // Try Lazy(DefId)
        if let Some(def_id) = crate::type_queries::get_lazy_def_id(self.interner, type_id) {
            let def_store = self.def_store?;
            let def = def_store.get(def_id)?;
            if let Some(sym_raw) = def.symbol_id {
                let name = self.format_symbol_name(SymbolId(sym_raw))?;
                return Some(self.namespace_qualify_symbol_name(SymbolId(sym_raw), name));
            }
        }
        // Try Enum
        if let Some(def_id) = crate::type_queries::get_enum_def_id(self.interner, type_id) {
            let def_store = self.def_store?;
            let def = def_store.get(def_id)?;
            if let Some(sym_raw) = def.symbol_id {
                let name = self.format_symbol_name(SymbolId(sym_raw))?;
                return Some(self.namespace_qualify_symbol_name(SymbolId(sym_raw), name));
            }
        }
        None
    }

    pub(super) fn format_intersection(&mut self, members: &[TypeId]) -> String {
        // Preserve the member order as stored in the TypeListId.
        // For intersections containing Lazy types (type parameters, type aliases),
        // normalize_intersection skips sorting and preserves source/declaration order.
        // tsc also preserves the original declaration order, so displaying members
        // in their stored order matches tsc's behavior.
        let formatted: Vec<String> = members
            .iter()
            .map(|&m| self.format_intersection_member(m))
            .collect();
        formatted.join(" & ")
    }

    pub(super) fn format_intersection_with_display(
        &mut self,
        members: &[TypeId],
        display_props: &[PropertyInfo],
    ) -> Option<String> {
        let replacement_idx = members
            .iter()
            .position(|&member| self.is_anonymous_object_intersection_member(member))?;

        Some(
            members
                .iter()
                .enumerate()
                .map(|(idx, &member)| {
                    if idx == replacement_idx {
                        self.format_intersection_member_with_display_props(member, display_props)
                    } else {
                        self.format_intersection_member(member)
                    }
                })
                .collect::<Vec<_>>()
                .join(" & "),
        )
    }

    /// Format an intersection member, parenthesizing union types.
    /// `(A | B) & (C | D)` is semantically different from `A | B & C | D`.
    fn format_intersection_member(&mut self, id: TypeId) -> String {
        let formatted = self.format(id);
        // Only parenthesize if the type is a union AND the formatted result
        // actually contains `|` (i.e., wasn't collapsed to a named alias).
        // Type aliases like `type T1 = "a" | "b"` display as `T1`, not
        // `"a" | "b"`, so they don't need parentheses in intersections.
        if matches!(self.interner.lookup(id), Some(TypeData::Union(_))) && formatted.contains(" | ")
        {
            format!("({formatted})")
        } else {
            formatted.into_owned()
        }
    }

    fn is_anonymous_object_intersection_member(&mut self, id: TypeId) -> bool {
        match self.interner.lookup(id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                self.resolve_object_shape_name(&shape).is_none()
            }
            _ => false,
        }
    }

    fn format_intersection_member_with_display_props(
        &mut self,
        id: TypeId,
        display_props: &[PropertyInfo],
    ) -> String {
        match self.interner.lookup(id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if self.resolve_object_shape_name(&shape).is_none() {
                    return self.format_object(display_props);
                }
                self.format_intersection_member(id)
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if self.resolve_object_shape_name(&shape).is_none() {
                    let mut display_shape = shape.as_ref().clone();
                    display_shape.properties = display_props.to_vec();
                    return self.format_object_with_index(&display_shape);
                }
                self.format_intersection_member(id)
            }
            _ => self.format_intersection_member(id),
        }
    }

    pub(super) fn format_tuple(&mut self, elements: &[TupleElement]) -> String {
        // Normalize: a tuple with a single rest element `[...T[]]` displays as `T[]`
        // to match tsc's display behavior.
        if elements.len() == 1 && elements[0].rest && elements[0].name.is_none() {
            let inner = self.format(elements[0].type_id);
            return inner.into_owned();
        }
        let formatted: Vec<String> = elements
            .iter()
            .map(|e| {
                let rest = if e.rest { "..." } else { "" };
                // Rest elements are never printed with `?` in tsc
                let optional = if e.optional && !e.rest { "?" } else { "" };
                let type_str: String = if e.optional && !e.rest {
                    self.format_stripping_undefined(e.type_id)
                } else {
                    self.format(e.type_id).into_owned()
                };
                if let Some(name_atom) = e.name {
                    let name = self.atom(name_atom);
                    format!("{rest}{name}{optional}: {type_str}")
                } else {
                    format!("{rest}{type_str}{optional}")
                }
            })
            .collect();
        format!("[{}]", formatted.join(", "))
    }

    pub(super) fn format_function(&mut self, shape: &FunctionShape) -> String {
        self.format_signature_with_predicate(
            &shape.type_params,
            &shape.params,
            shape.this_type,
            shape.return_type,
            shape.type_predicate.as_ref(),
            shape.is_constructor,
            false,
            " =>",
        )
    }

    pub(super) fn format_callable(&mut self, shape: &CallableShape) -> String {
        if !shape.construct_signatures.is_empty()
            && let Some(sym_id) = shape.symbol
            && let Some(name) = self.format_symbol_name(sym_id)
        {
            if let Some(arena) = self.symbol_arena
                && let Some(sym) = arena.get(sym_id)
            {
                use tsz_binder::symbol_flags;
                let is_namespace =
                    sym.has_any_flags(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE);
                let is_enum = sym.has_any_flags(symbol_flags::ENUM);
                let is_class = sym.has_flags(symbol_flags::CLASS);
                let is_interface = sym.has_any_flags(symbol_flags::INTERFACE);
                // Classes have both CLASS and INTERFACE flags; only skip typeof
                // for pure interfaces (no CLASS flag). Class constructors should
                // display as "typeof ClassName" to match tsc.
                if (is_interface && !is_class) || (!is_namespace && !is_enum && !is_class) {
                    return name;
                }
            }
            return format!("typeof {name}");
        }

        let has_index = shape.string_index.is_some() || shape.number_index.is_some();
        if !has_index && shape.properties.is_empty() {
            if shape.call_signatures.len() == 1 && shape.construct_signatures.is_empty() {
                let sig = &shape.call_signatures[0];
                return self.format_signature_with_predicate(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                    sig.type_predicate.as_ref(),
                    false,
                    false,
                    " =>",
                );
            }
            if shape.construct_signatures.len() == 1 && shape.call_signatures.is_empty() {
                let sig = &shape.construct_signatures[0];
                return self.format_signature(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
                    true,
                    shape.is_abstract,
                    " =>",
                );
            }
        }

        let mut parts = Vec::new();
        for sig in &shape.call_signatures {
            parts.push(self.format_call_signature(sig, false, false));
        }
        for sig in &shape.construct_signatures {
            parts.push(self.format_call_signature(sig, true, shape.is_abstract));
        }
        if let Some(ref idx) = shape.string_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{ro}[{key_name}: string]: {}",
                self.format(idx.value_type)
            ));
        }
        if let Some(ref idx) = shape.number_index {
            let key_name = idx
                .param_name
                .map(|a| self.atom(a).to_string())
                .unwrap_or_else(|| "x".to_owned());
            let ro = if idx.readonly { "readonly " } else { "" };
            parts.push(format!(
                "{ro}[{key_name}: number]: {}",
                self.format(idx.value_type)
            ));
        }
        let mut sorted_props: Vec<&PropertyInfo> = shape.properties.iter().collect();
        // Sort by declaration_order (same logic as format_object)
        sorted_props.sort_by(|a, b| {
            let ord = a.declaration_order.cmp(&b.declaration_order);
            if ord != std::cmp::Ordering::Equal
                && a.declaration_order > 0
                && b.declaration_order > 0
            {
                return ord;
            }
            let a_name = self.interner.resolve_atom_ref(a.name);
            let b_name = self.interner.resolve_atom_ref(b.name);
            let a_num = a_name.parse::<u64>();
            let b_num = b_name.parse::<u64>();
            match (a_num, b_num) {
                (Ok(an), Ok(bn)) => an.cmp(&bn),
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                (Err(_), Err(_)) => std::cmp::Ordering::Equal,
            }
        });
        for prop in sorted_props {
            parts.push(self.format_property(prop));
        }

        if parts.is_empty() {
            return "{}".to_string();
        }

        format!("{{ {}; }}", parts.join("; "))
    }

    fn format_call_signature(
        &mut self,
        sig: &CallSignature,
        is_construct: bool,
        is_abstract: bool,
    ) -> String {
        self.format_signature_with_predicate(
            &sig.type_params,
            &sig.params,
            sig.this_type,
            sig.return_type,
            sig.type_predicate.as_ref(),
            is_construct,
            is_abstract,
            ":",
        )
    }

    pub(super) fn format_conditional(&mut self, cond: &ConditionalType) -> String {
        let prev = self.preserve_optional_property_surface_syntax;
        self.preserve_optional_property_surface_syntax = true;
        let extends_type = self.format(cond.extends_type).into_owned();
        self.preserve_optional_property_surface_syntax = prev;
        format!(
            "{} extends {} ? {} : {}",
            self.format(cond.check_type),
            extends_type,
            self.format(cond.true_type),
            self.format(cond.false_type)
        )
    }

    pub(super) fn format_mapped(&mut self, mapped: &MappedType) -> String {
        if let Some(index_signature) = self.try_format_mapped_as_index_signature(mapped) {
            return index_signature;
        }
        let param_name = self.atom(mapped.type_param.name);
        let readonly_prefix = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => "readonly ",
            Some(MappedModifier::Remove) => "-readonly ",
            None => "",
        };
        let optional_suffix = match mapped.optional_modifier {
            Some(MappedModifier::Add) => "?",
            Some(MappedModifier::Remove) => "-?",
            None => "",
        };
        let template_str = self.format(mapped.template);
        // tsc displays optional mapped types with `| undefined` appended to the
        // template: `{ [P in keyof T]?: T[P] | undefined; }`. Only add when the
        // optional modifier is Add and the template doesn't already contain undefined.
        let needs_undefined = if mapped.optional_modifier == Some(MappedModifier::Add)
            && mapped.template != TypeId::UNDEFINED
            && mapped.template != TypeId::ANY
            && mapped.template != TypeId::UNKNOWN
        {
            // Check if the template type already contains undefined
            // (e.g., if it's a union that includes undefined, any, or unknown).
            if let Some(TypeData::Union(members)) = self.interner.lookup(mapped.template) {
                let list = self.interner.type_list(members);
                !list
                    .as_ref()
                    .iter()
                    .any(|&m| m == TypeId::UNDEFINED || m == TypeId::ANY || m == TypeId::UNKNOWN)
            } else {
                true
            }
        } else {
            false
        };
        let template_display = if needs_undefined {
            format!("{template_str} | undefined")
        } else {
            template_str.into_owned()
        };
        format!(
            "{{ {readonly_prefix}[{param_name} in {}]{optional_suffix}: {template_display}; }}",
            self.format(mapped.constraint),
        )
    }

    fn try_format_mapped_as_index_signature(&mut self, mapped: &MappedType) -> Option<String> {
        if mapped.name_type.is_some() || mapped.optional_modifier.is_some() {
            return None;
        }
        let key_kind = match mapped.constraint {
            TypeId::STRING => "string",
            TypeId::NUMBER => "number",
            _ => return None,
        };
        if crate::contains_type_parameter_named(
            self.interner,
            mapped.template,
            mapped.type_param.name,
        ) {
            return None;
        }
        let readonly_prefix = match mapped.readonly_modifier {
            Some(MappedModifier::Add) => "readonly ",
            Some(MappedModifier::Remove) => "-readonly ",
            None => "",
        };
        Some(format!(
            "{{ {readonly_prefix}[x: {key_kind}]: {}; }}",
            self.format(mapped.template)
        ))
    }

    pub(super) fn format_template_literal(&mut self, spans: &[TemplateSpan]) -> String {
        let mut result = String::from("`");
        for span in spans {
            match span {
                TemplateSpan::Text(text) => {
                    let text = self.atom(*text);
                    // Escape special characters consistently with string literals
                    let escaped = text
                        .replace('\\', "\\\\")
                        .replace('\n', "\\n")
                        .replace('\r', "\\r")
                        .replace('\t', "\\t");
                    result.push_str(&escaped);
                }
                TemplateSpan::Type(type_id) => {
                    result.push_str("${");
                    result.push_str(&self.format(*type_id));
                    result.push('}');
                }
            }
        }
        result.push('`');
        result
    }

    /// Resolve a `DefId` to a human-readable name via the definition store,
    /// falling back to `"<prefix>(<raw_id>)"` if unavailable.
    pub(super) fn format_def_id(
        &mut self,
        def_id: crate::def::DefId,
        fallback_prefix: &str,
    ) -> String {
        if let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
        {
            let name = self.format_def_name(&def);
            // Class constructor defs represent the static side of a class.
            // tsc displays these as "typeof ClassName".
            if def.is_class_constructor() {
                return format!("typeof {name}");
            }
            return name;
        }
        // NOTE: We do NOT use format_raw_def_id_symbol_fallback here.
        // DefId and SymbolId are independent ID spaces. A DefId's raw value
        // should never be interpreted as a SymbolId — doing so would return
        // the name of an unrelated symbol that happens to share the same u32.
        format!("{}({})", fallback_prefix, def_id.0)
    }

    /// Format a `DefId` with type parameters appended when the definition is generic.
    ///
    /// tsc displays uninstantiated generic types with their type parameter names:
    /// e.g., `B<T>` instead of just `B`. This matches that behavior for
    /// `TypeData::Lazy(DefId)` nodes that represent generic types without
    /// an `Application` wrapper.
    pub(super) fn format_def_id_with_type_params(
        &mut self,
        def_id: crate::def::DefId,
        fallback_prefix: &str,
    ) -> String {
        if let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
        {
            let name = self.format_def_name(&def);
            // Class constructor defs (DefKind::ClassConstructor) represent the
            // static side of a class. tsc displays these as "typeof ClassName".
            let prefix = if def.is_class_constructor() {
                "typeof "
            } else {
                ""
            };
            if def.type_params.is_empty() {
                return format!("{prefix}{name}");
            }
            let params: Vec<String> = def
                .type_params
                .iter()
                .map(|tp| self.atom(tp.name).to_string())
                .collect();
            return format!("{prefix}{}<{}>", name, params.join(", "));
        }
        // NOTE: We do NOT use format_raw_def_id_symbol_fallback here.
        // DefId and SymbolId are independent ID spaces — see comment above.
        format!("{}({})", fallback_prefix, def_id.0)
    }

    // NOTE: format_raw_def_id_symbol_fallback was removed.
    // It incorrectly assumed DefId.0 == SymbolId.0, which caused wrong type
    // names in diagnostics (e.g., enum "Foo" displaying as "timeout").
    // DefId and SymbolId are independent ID spaces and must not be conflated.

    /// Try to resolve a human-readable name for an object shape via symbol or def store lookup.
    pub(super) fn resolve_object_shape_name(&mut self, shape: &ObjectShape) -> Option<String> {
        if let Some(sym_id) = shape.symbol
            && let Some(name) = self.format_symbol_name(sym_id)
        {
            // Namespace/module/enum value types are displayed as `typeof Name` by tsc.
            if let Some(arena) = self.symbol_arena
                && let Some(sym) = arena.get(sym_id)
            {
                use tsz_binder::symbol_flags;
                let is_namespace =
                    sym.has_any_flags(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE);
                let is_enum = sym.has_any_flags(symbol_flags::ENUM);
                let is_class = sym.has_flags(symbol_flags::CLASS);
                let is_interface = sym.has_any_flags(symbol_flags::INTERFACE);
                // When a symbol is both an interface and a namespace (declaration
                // merging), the type-space name wins — tsc displays `B`, not
                // `typeof B`.  Similarly, classes take priority over namespaces.
                if (is_namespace || is_enum) && !is_class && !is_interface {
                    return Some(format!("typeof {name}"));
                }
            }
            return Some(name);
        }
        // Fall back to def-store structural lookup for type aliases and lib interfaces.
        // User-defined interfaces preserve their symbol through merge_interface_types, so they
        // are found via path 1 above. Anonymous types (symbol=None) cannot accidentally match
        // named interfaces (symbol=Some(...)) via find_def_by_shape because PartialEq includes symbol.
        // This path handles: (a) type aliases (always symbol=None), and (b) lib interfaces
        // (built without symbol stamps, e.g. String) whose unique structural content prevents
        // false matches.
        if let Some(def_store) = self.def_store
            && let Some(def_id) = def_store.find_def_by_shape(shape)
            && let Some(def) = def_store.get(def_id)
        {
            return Some(self.format_def_name(&def));
        }
        // Special case: detect the global Object interface by its characteristic properties.
        // The Object interface has: constructor, toString, toLocaleString, valueOf,
        // hasOwnProperty, isPrototypeOf, propertyIsEnumerable.
        // When we see an object shape with exactly these properties (in any order), display as "Object".
        if shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.len() >= 6
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "constructor")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "toString")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "toLocaleString")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "valueOf")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "hasOwnProperty")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "isPrototypeOf")
        {
            return Some("Object".to_string());
        }
        // Special case: detect the global RegExp interface by characteristic
        // members so diagnostics prefer `RegExp` over expanded structural shape.
        // This mirrors tsc display behavior in contexts like import attributes.
        if shape.string_index.is_none()
            && shape.number_index.is_none()
            && shape.properties.len() >= 10
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "exec")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "test")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "source")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "global")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "ignoreCase")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "multiline")
            && shape
                .properties
                .iter()
                .any(|p| self.atom(p.name).as_ref() == "lastIndex")
        {
            return Some("RegExp".to_string());
        }
        None
    }

    pub(super) fn format_symbol_name(&mut self, sym_id: SymbolId) -> Option<String> {
        let arena = self.symbol_arena?;
        let sym = arena.get(sym_id)?;
        let mut qualified_name = sym.escaped_name.to_string();
        let mut current_parent = sym.parent;

        use tsz_binder::symbol_flags;

        // Walk up the parent chain, qualifying with enum parents only.
        // tsc qualifies type names with their containing enum (e.g., `Choice.Yes`)
        // but uses SHORT names for types inside namespaces (e.g., `Line` not `A.Line`)
        // unless disambiguation is needed (same name in outer scope). Namespace
        // qualification requires scope-aware disambiguation not yet implemented.
        // Skip file-level module symbols (synthetic names like __test1__, "file.ts", etc.)
        // as those represent file modules, not declared namespaces.
        while current_parent != SymbolId::NONE {
            if let Some(parent_sym) = arena.get(current_parent) {
                let is_qualifying_parent = parent_sym.has_any_flags(symbol_flags::ENUM);
                let name = &parent_sym.escaped_name;
                let is_file_module = name.starts_with('"')
                    || name.starts_with("__")
                    || name.contains('/')
                    || name.contains('\\')
                    || name.is_empty();
                if is_qualifying_parent && !is_file_module {
                    qualified_name = format!("{}.{}", parent_sym.escaped_name, qualified_name);
                    current_parent = parent_sym.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Some(self.qualify_namespace_name_if_needed(sym_id, &sym.escaped_name, qualified_name))
    }

    /// Resolve a `SymbolRef` (from `TypeQuery` / `ModuleNamespace`) to a display name.
    /// Tries the symbol arena first, then falls back to the definition store's
    /// `find_def_by_symbol` lookup.
    pub(super) fn resolve_symbol_ref_name(&mut self, sym: SymbolRef) -> Option<String> {
        if let Some(name) = self.format_symbol_name(SymbolId(sym.0)) {
            return Some(name);
        }
        // Fallback: try the definition store by symbol id
        if let Some(def_store) = self.def_store
            && let Some(def_id) = def_store.find_def_by_symbol(sym.0)
            && let Some(def) = def_store.get(def_id)
        {
            return Some(self.format_def_name(&def));
        }
        None
    }

    pub(super) fn format_def_name(&mut self, def: &crate::def::DefinitionInfo) -> String {
        // Try to build a qualified name by walking the symbol parent chain.
        // tsc qualifies type names with their containing enum (e.g., `Choice.Yes`)
        // but uses SHORT names for types inside namespaces (e.g., `Line` not `A.Line`).
        let def_name = self.atom(def.name).to_string();
        if let Some(sym_raw) = def.symbol_id
            && let Some(arena) = self.symbol_arena
            && let Some(symbol) = arena.get(SymbolId(sym_raw))
        {
            use tsz_binder::symbol_flags;
            let foreign_symbol_name_collision = symbol.escaped_name != def_name
                && self
                    .current_file_id
                    .zip(def.file_id)
                    .is_some_and(|(current_file_id, def_file_id)| current_file_id != def_file_id);

            if foreign_symbol_name_collision {
                return def_name;
            }

            // For anonymous class expressions assigned to variables, the binder
            // creates a symbol named "(Anonymous class)" but tsc displays the
            // variable name instead. Prefer the definition's name in this case.
            let base_name = if symbol.escaped_name == "(Anonymous class)" {
                def_name
            } else {
                symbol.escaped_name.to_string()
            };
            let mut qualified_name = base_name;
            let mut current_parent = symbol.parent;

            while current_parent != SymbolId::NONE {
                if let Some(parent_sym) = arena.get(current_parent) {
                    // Only qualify with enum parents, not namespace/module parents.
                    let is_qualifying_parent = parent_sym.has_any_flags(symbol_flags::ENUM);
                    let name = &parent_sym.escaped_name;
                    let is_file_module = name.starts_with('"')
                        || name.starts_with("__")
                        || name.contains('/')
                        || name.contains('\\')
                        || name.is_empty();
                    if is_qualifying_parent && !is_file_module {
                        qualified_name = format!("{}.{}", parent_sym.escaped_name, qualified_name);
                        current_parent = parent_sym.parent;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            return self.qualify_namespace_name_if_needed(
                SymbolId(sym_raw),
                &symbol.escaped_name,
                qualified_name,
            );
        }

        // Fallback: use the short (unqualified) definition name.
        def_name
    }

    #[allow(clippy::missing_const_for_fn)] // Can't be const with &self in stable Rust
    fn qualify_namespace_name_if_needed(
        &self,
        _sym_id: SymbolId,
        _original_name: &str,
        current_name: String,
    ) -> String {
        // tsc uses SHORT names in general type display, even when multiple types
        // share the same name across different namespaces. Namespace qualification
        // is only added in specific contexts (union display with same-name members,
        // explicit disambiguation messages). Those paths are handled separately.
        current_name
    }

    /// Namespace-qualify a symbol name for contexts where disambiguation is needed
    /// (e.g., union display with same-name members from different namespaces).
    pub(super) fn namespace_qualify_symbol_name(
        &self,
        sym_id: SymbolId,
        current_name: String,
    ) -> String {
        let Some(arena) = self.symbol_arena else {
            return current_name;
        };
        let Some(symbol) = arena.get(sym_id) else {
            return current_name;
        };
        let mut parts = vec![current_name];
        let mut current_parent = symbol.parent;
        use tsz_binder::symbol_flags;

        while current_parent != SymbolId::NONE {
            if let Some(parent_sym) = arena.get(current_parent) {
                let is_qualifying_parent =
                    parent_sym.has_any_flags(symbol_flags::MODULE | symbol_flags::ENUM);
                let name = &parent_sym.escaped_name;
                let is_file_module = name.starts_with('"')
                    || name.starts_with("__")
                    || name.contains('/')
                    || name.contains('\\')
                    || name.is_empty();
                if is_qualifying_parent && !is_file_module {
                    parts.push(parent_sym.escaped_name.clone());
                    current_parent = parent_sym.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if parts.len() == 1 {
            return parts.pop().expect("parts has one element");
        }

        parts.reverse();
        parts.join(".")
    }

    /// Returns a sort key for intrinsic/builtin types to match tsc's display ordering.
    /// tsc orders builtins as: string(8), number(9), bigint(10), boolean(11), etc.
    const fn builtin_sort_key(id: TypeId) -> Option<u32> {
        match id {
            TypeId::NUMBER => Some(9),
            TypeId::STRING => Some(8),
            TypeId::BIGINT => Some(10),
            TypeId::BOOLEAN | TypeId::BOOLEAN_TRUE => Some(11),
            TypeId::BOOLEAN_FALSE => Some(12),
            TypeId::VOID => Some(13),
            TypeId::UNDEFINED => Some(14),
            TypeId::NULL => Some(15),
            TypeId::SYMBOL => Some(16),
            TypeId::OBJECT => Some(17),
            TypeId::FUNCTION => Some(18),
            _ if id.is_intrinsic() => Some(id.0),
            _ => None,
        }
    }

    /// Returns (tier, `file_id`, `span_start`) for a type, used for source-order sorting.
    /// - Tier 0: Builtins/intrinsics (always first)
    /// - Tier 1: User-defined types with source info (sorted by file, then position)
    /// - Tier 2: Types without source info (preserve original order by returning sentinel)
    fn get_source_position_for_type(
        &self,
        type_id: TypeId,
        def_store: &crate::def::DefinitionStore,
    ) -> (u32, u32, u32) {
        // Tier 0: Intrinsics have fixed position
        if let Some(key) = Self::builtin_sort_key(type_id) {
            return (0, 0, key);
        }

        let data = self.interner.lookup(type_id);

        // Try Lazy(DefId) - type aliases, interfaces, classes
        if let Some(TypeData::Lazy(def_id)) = &data
            && let Some(def) = def_store.get(*def_id)
            && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
        {
            return (1, file_id, span_start);
        }

        // Try Application - generic instantiation, get base type's position
        if let Some(TypeData::Application(app_id)) = &data {
            let app = self.interner.type_application(*app_id);
            return self.get_source_position_for_type(app.base, def_store);
        }

        // Try Enum
        if let Some(TypeData::Enum(def_id, _)) = &data
            && let Some(def) = def_store.get(*def_id)
            && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
        {
            return (1, file_id, span_start);
        }

        // Try Object/ObjectWithIndex with symbol
        if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) = &data {
            let shape = self.interner.object_shape(*shape_id);
            if let Some(sym_id) = shape.symbol
                && let Some(def_id) = def_store.find_def_by_symbol(sym_id.0)
                && let Some(def) = def_store.get(def_id)
                && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
            {
                return (1, file_id, span_start);
            }
        }

        // Try Callable with symbol
        if let Some(TypeData::Callable(shape_id)) = &data {
            let shape = self.interner.callable_shape(*shape_id);
            if let Some(sym_id) = shape.symbol
                && let Some(def_id) = def_store.find_def_by_symbol(sym_id.0)
                && let Some(def) = def_store.get(def_id)
                && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
            {
                return (1, file_id, span_start);
            }
        }

        // Tier 2: Fallback for anonymous types without source info.
        // For Object types, sort by property count (fewer properties first) to match
        // tsc's display order for anonymous object unions like `{} | { a: number }`.
        if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) = &data {
            let shape = self.interner.object_shape(*shape_id);
            let prop_count = shape.properties.len() as u32;
            // Use property count as the sort key. Objects with fewer properties
            // are displayed first by tsc.
            return (2, 0, prop_count);
        }

        // Other tier 2 types: sort after objects, preserve relative order
        (2, u32::MAX, u32::MAX)
    }
}
