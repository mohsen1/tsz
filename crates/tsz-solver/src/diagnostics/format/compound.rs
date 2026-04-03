//! Compound type formatting methods for `TypeFormatter`.

use super::needs_property_name_quotes;
use super::TypeFormatter;
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
            let escaped = raw_name.replace('\\', "\\\\").replace('\'', "\\'");
            format!("'{escaped}'")
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
    fn type_contains_undefined(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::UNDEFINED {
            return true;
        }
        if let Some(TypeData::Union(list_id)) = self.interner.lookup(type_id) {
            let members = self.interner.type_list(list_id);
            return members.contains(&TypeId::UNDEFINED);
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

    pub(super) fn format_params(&mut self, params: &[ParamInfo], this_type: Option<TypeId>) -> Vec<String> {
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
        let prefix = if is_construct && is_abstract {
            "abstract new "
        } else if is_construct {
            "new "
        } else {
            ""
        };
        let type_params = self.format_type_params(type_params);
        let params = self.format_params(params, this_type);
        let return_str: Cow<'static, str> = if is_construct && return_type == TypeId::UNKNOWN {
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

        let nullish_suffix_len = usize::from(has_null) + usize::from(has_undefined);
        let sortable_len = ordered.len().saturating_sub(nullish_suffix_len);
        if sortable_len > 1
            && let Some(mut named_keys) = ordered[..sortable_len]
                .iter()
                .map(|&m| self.named_union_member_display_key(m).map(|key| (m, key)))
                .collect::<Option<Vec<_>>>()
        {
            named_keys.sort_by(|(_, left), (_, right)| left.cmp(right));
            for (idx, (member, _)) in named_keys.into_iter().enumerate() {
                ordered[idx] = member;
            }
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
        formatted.join(" | ")
    }

    fn named_union_member_display_key(&mut self, type_id: TypeId) -> Option<String> {
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => self
                .interner
                .object_shape(shape_id)
                .symbol
                .map(|_| self.format(type_id).into_owned())
                .filter(|rendered| !rendered.starts_with('{')),
            Some(TypeData::Callable(shape_id)) => self
                .interner
                .callable_shape(shape_id)
                .symbol
                .map(|_| self.format(type_id).into_owned())
                .filter(|rendered| {
                    !rendered.starts_with('{')
                        && !rendered.starts_with('(')
                        && !rendered.starts_with("new ")
                        && !rendered.starts_with("abstract new ")
                }),
            Some(TypeData::Lazy(_) | TypeData::TypeQuery(_)) => {
                Some(self.format(type_id).into_owned())
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                match self.interner.lookup(app.base) {
                    Some(TypeData::Lazy(_) | TypeData::TypeQuery(_)) => {
                        Some(self.format(type_id).into_owned())
                    }
                    _ => None,
                }
            }
            _ => None,
        }
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
            return Some(rendered.into_iter().next().unwrap());
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

    /// Format an intersection member, parenthesizing union types.
    /// `(A | B) & (C | D)` is semantically different from `A | B & C | D`.
    fn format_intersection_member(&mut self, id: TypeId) -> String {
        let formatted = self.format(id);
        if matches!(self.interner.lookup(id), Some(TypeData::Union(_))) {
            format!("({formatted})")
        } else {
            formatted.into_owned()
        }
    }

    pub(super) fn format_tuple(&mut self, elements: &[TupleElement]) -> String {
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
        self.format_signature(
            &shape.type_params,
            &shape.params,
            shape.this_type,
            shape.return_type,
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
                if is_interface || (!is_namespace && !is_enum && !is_class) {
                    return name;
                }
            }
            return format!("typeof {name}");
        }

        let has_index = shape.string_index.is_some() || shape.number_index.is_some();
        if !has_index && shape.properties.is_empty() {
            if shape.call_signatures.len() == 1 && shape.construct_signatures.is_empty() {
                let sig = &shape.call_signatures[0];
                return self.format_signature(
                    &sig.type_params,
                    &sig.params,
                    sig.this_type,
                    sig.return_type,
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
        self.format_signature(
            &sig.type_params,
            &sig.params,
            sig.this_type,
            sig.return_type,
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
        format!(
            "{{ {readonly_prefix}[{param_name} in {}]{optional_suffix}: {}; }}",
            self.format(mapped.constraint),
            self.format(mapped.template)
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
                    result.push_str(text.as_ref());
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
    pub(super) fn format_def_id(&mut self, def_id: crate::def::DefId, fallback_prefix: &str) -> String {
        if let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
        {
            return self.format_def_name(&def);
        }
        if let Some(name) = self.format_raw_def_id_symbol_fallback(def_id) {
            return name;
        }
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
            if def.type_params.is_empty() {
                return name;
            }
            let params: Vec<String> = def
                .type_params
                .iter()
                .map(|tp| self.atom(tp.name).to_string())
                .collect();
            return format!("{}<{}>", name, params.join(", "));
        }
        if let Some(name) = self.format_raw_def_id_symbol_fallback(def_id) {
            return name;
        }
        format!("{}({})", fallback_prefix, def_id.0)
    }

    /// Some checker paths still materialize fallback `Lazy(DefId(symbol_id))` nodes
    /// without registering the `DefId` in the definition store. When that happens,
    /// use the raw id as a `SymbolId` if it resolves in the active symbol arena.
    pub(super) fn format_raw_def_id_symbol_fallback(&mut self, def_id: crate::def::DefId) -> Option<String> {
        let sym_id = SymbolId(def_id.0);
        self.format_symbol_name(sym_id)
    }

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
        // but uses SHORT names for types inside namespaces (e.g., `Line` not `A.Line`).
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

        Some(qualified_name)
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
        // Always use the short (unqualified) definition name.
        // Enum member qualification (e.g., `Choice.Yes`) is handled by
        // `format_symbol_name` through the `resolve_symbol_ref_name` path.
        // Using `format_symbol_name` here causes cross-binder SymbolId
        // collisions where the def's symbol_id maps to a namespace-qualified
        // symbol in the current binder (e.g., `A.B` instead of just `B`).
        self.atom(def.name).to_string()
    }
}
