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
    fn collapse_truncated_tail_part(part: &str) -> String {
        let Some((prefix, ty)) = part.split_once(": ") else {
            return part.to_string();
        };
        if ty.starts_with('{') {
            return format!("{prefix}: {{ ...; }}");
        }
        part.to_string()
    }

    /// Format object-like parts with tsc-style long-type truncation:
    /// keep a long prefix and the last member, inserting `... N more ...`.
    ///
    /// This is used for both plain object literals and object-with-index displays.
    /// tsc starts truncating only on larger member counts (roughly 22+), and
    /// preserves the tail member (often useful symbol members such as
    /// `[Symbol.unscopables]`).
    fn format_object_parts(&self, parts: Vec<String>) -> String {
        if parts.is_empty() {
            return "{}".to_string();
        }

        // Match tsc's higher truncation threshold (small/medium objects display fully).
        const TRUNCATE_THRESHOLD: usize = 22;
        if parts.len() < TRUNCATE_THRESHOLD {
            return format!("{{ {}; }}", parts.join("; "));
        }

        // Keep at most this many leading members before the omitted-count marker.
        const MAX_HEAD_PARTS: usize = 17;
        // Soft budget for head text. Long member signatures (for example,
        // `toLocaleString` overloads) reduce the number of retained heads.
        const MAX_HEAD_CHARS: usize = 300;

        let total = parts.len();
        let tail_index = parts
            .iter()
            .rposition(|part| part.starts_with("[Symbol.") || part.starts_with("readonly [Symbol."))
            .filter(|&idx| idx > 0)
            .unwrap_or(total - 1);
        let tail = Self::collapse_truncated_tail_part(&parts[tail_index]);
        let max_head_chars = if tail_index == total - 1 {
            MAX_HEAD_CHARS
        } else {
            255
        };
        let mut head_count = 0usize;
        let mut used_chars = 0usize;

        for (idx, part) in parts.iter().enumerate().take(tail_index) {
            if head_count >= MAX_HEAD_PARTS {
                break;
            }
            let part_cost = if head_count == 0 {
                part.len()
            } else {
                // "; " separator
                part.len() + 2
            };
            let next_used = used_chars + part_cost;
            let remaining_after = total - (idx + 1) - 1; // tail excluded
            let omitted_digits = remaining_after.max(1).to_string().len();
            // Reserve space for `; ... N more ...; <tail>`
            let reserve_for_marker = 2 + 4 + omitted_digits + 9;
            let reserve_for_tail = 2 + tail.len();

            // Keep at least two head parts when available; after that, enforce budget.
            if head_count >= 2 && next_used + reserve_for_marker + reserve_for_tail > max_head_chars
            {
                break;
            }

            used_chars = next_used;
            head_count += 1;
        }

        // Ensure progress even with extremely long first members.
        if head_count == 0 {
            head_count = 1;
        }

        let omitted = total.saturating_sub(head_count + 1);
        if omitted == 0 {
            return format!("{{ {}; }}", parts.join("; "));
        }

        let mut display_parts = Vec::with_capacity(head_count + 2);
        display_parts.extend(parts.iter().take(head_count).cloned());
        display_parts.push(format!("... {omitted} more ..."));
        display_parts.push(tail);
        format!("{{ {}; }}", display_parts.join("; "))
    }

    fn visible_object_properties<'b>(&self, props: &'b [PropertyInfo]) -> Vec<&'b PropertyInfo> {
        let default_name = self.interner.intern_string("default");
        let internal_default_name = self.interner.intern_string("_default");
        let default_prop = props.iter().find(|prop| prop.name == default_name);

        props
            .iter()
            .filter(|prop| {
                if prop.name != internal_default_name {
                    return true;
                }
                let Some(default_prop) = default_prop else {
                    return true;
                };

                // Some module export surfaces retain the local `_default` binding
                // alongside the real `default` export. tsc hides that duplicate
                // implementation detail in object displays.
                prop.type_id != default_prop.type_id
                    || prop.write_type != default_prop.write_type
                    || prop.optional != default_prop.optional
                    || prop.readonly != default_prop.readonly
                    || prop.is_method != default_prop.is_method
            })
            .collect()
    }

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
                // Match JS `Number.prototype.toString()` so very large/small
                // values use scientific notation (e.g. `5.46e+244`) rather
                // than Rust's default integer expansion. Also handles
                // `Infinity`, `-Infinity`, and `NaN` consistently.
                crate::utils::js_number_to_string(n.0).into_owned()
            }
            LiteralValue::BigInt(b) => format!("{}n", self.atom(*b)),
            LiteralValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
        }
    }

    pub(super) fn format_object(&mut self, props: &[PropertyInfo]) -> String {
        if props.is_empty() {
            return "{}".to_string();
        }
        let mut display_props = self.visible_object_properties(props);
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
        let formatted: Vec<String> = display_props
            .iter()
            .map(|p| self.format_property(p))
            .collect();
        self.format_object_parts(formatted)
    }

    pub(super) fn format_property(&mut self, prop: &PropertyInfo) -> String {
        let optional = if prop.optional { "?" } else { "" };
        let readonly = if prop.readonly { "readonly " } else { "" };
        let raw_name = self.atom(prop.name);
        let name = if needs_property_name_quotes(&raw_name) {
            // tsc uses double quotes for JSX-specific property names
            // (namespace-prefixed like "ns:attr" and data attributes like "data-foo"),
            // for names starting with a digit, and for names containing any
            // character outside [a-zA-Z0-9_-] (e.g. "*"). Single quotes are used
            // for all other quoted property names (e.g. 'stage-0', '').
            let use_double = raw_name.contains(':')
                || raw_name.starts_with("data-")
                || raw_name.chars().next().is_some_and(|c| c.is_ascii_digit())
                || raw_name
                    .chars()
                    .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'));
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
            let type_str: String = if p.optional {
                let formatted = self.format(p.type_id).into_owned();
                if self.preserve_optional_parameter_surface_syntax {
                    formatted
                } else if p.type_id == TypeId::NEVER {
                    "undefined".to_string()
                } else if !self.type_contains_undefined(p.type_id) {
                    format!("{formatted} | undefined")
                } else {
                    formatted
                }
            } else {
                self.format(p.type_id).into_owned()
            };
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
        let mut display_props = self.visible_object_properties(shape.properties.as_slice());
        let has_decl_order = display_props.iter().any(|p| p.declaration_order > 0);
        if has_decl_order {
            display_props.sort_by_key(|p| p.declaration_order);
        }
        for prop in display_props {
            parts.push(self.format_property(prop));
        }

        self.format_object_parts(parts)
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
        // - Tier 0/1 (builtins/user types with source): sort by source position.
        // - Tier 2 (anonymous objects without source): preserve declaration order.
        //   The previous "sort tier-2 objects by property count" heuristic matched
        //   tsc's output for some `{} | { a: number }`-style unions but reordered
        //   legitimate discriminated-union displays (e.g. TS2353/TS2322 messages)
        //   where tsc preserves declaration order of the anonymous members.
        if let Some(def_store) = self.def_store {
            let positions: Vec<_> = ordered
                .iter()
                .map(|&m| self.get_source_position_for_type(m, def_store))
                .collect();

            let all_tier_0_or_1 = positions.iter().all(|&(tier, _, _)| tier < 2);

            if all_tier_0_or_1 {
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
        let (ordered, formatted) = Self::remove_redundant_intersection_displays(ordered, formatted);
        let (ordered, formatted) = Self::order_boolean_literal_union_displays(ordered, formatted);
        // Disambiguate duplicate display names by adding namespace qualification.
        // tsc shows `Foo.Yep | Bar.Yep` instead of `Yep | Yep` when two different
        // types share the same name in different namespaces.
        let disambiguated = self.disambiguate_union_member_names(&ordered, formatted);
        disambiguated.join(" | ")
    }

    fn remove_redundant_intersection_displays(
        ordered: Vec<TypeId>,
        formatted: Vec<String>,
    ) -> (Vec<TypeId>, Vec<String>) {
        if formatted.len() <= 1 {
            return (ordered, formatted);
        }

        let keep: Vec<bool> = formatted
            .iter()
            .map(|display| {
                let inner = display
                    .strip_prefix('(')
                    .and_then(|s| s.strip_suffix(')'))
                    .unwrap_or(display);
                let parts: Vec<&str> = inner.split(" & ").collect();
                parts.len() <= 1
                    || !parts
                        .iter()
                        .any(|part| formatted.iter().any(|other| other == part))
            })
            .collect();

        let mut kept_ordered = Vec::with_capacity(ordered.len());
        let mut kept_formatted = Vec::with_capacity(formatted.len());
        for ((type_id, display), keep) in ordered.into_iter().zip(formatted).zip(keep) {
            if keep {
                kept_ordered.push(type_id);
                kept_formatted.push(display);
            }
        }
        (kept_ordered, kept_formatted)
    }

    fn order_boolean_literal_union_displays(
        ordered: Vec<TypeId>,
        formatted: Vec<String>,
    ) -> (Vec<TypeId>, Vec<String>) {
        if formatted.len() < 2 {
            return (ordered, formatted);
        }

        // Rank order matching tsc's union printer:
        //   1. primitives (string, number, bigint, ...)  → rank 0
        //   2. boolean false literal display              → rank 1
        //   3. boolean true literal display               → rank 2
        //   4. everything else (objects, classes, etc.)   → rank 3
        // Within a rank, original index breaks ties (stable sort by source order).
        // Without rank 0, `false | number` and `false | string` would stay in
        // construction order even though tsc renders them as `number | false`
        // and `string | false`. With rank 3 below boolean, `false | D1` (class)
        // remains `false | D1` because the class type ranks AFTER false, not
        // before, matching tsc's `Type 'false | D1' is not assignable …`.
        const PRIMITIVE_DISPLAYS: &[&str] = &[
            "string", "number", "bigint", "symbol", "void", "object", "any", "unknown", "never",
        ];

        let ranks: Vec<u8> = formatted
            .iter()
            .map(|display| {
                let trimmed = display.as_str();
                if PRIMITIVE_DISPLAYS.contains(&trimmed) {
                    return 0u8;
                }
                let has_false = display.contains("false");
                let has_true = display.contains("true");
                match (has_false, has_true) {
                    (true, false) => 1u8,
                    (false, true) => 2u8,
                    _ => 3u8,
                }
            })
            .collect();

        // Skip when no boolean literal participates — leave the natural order.
        if !ranks.iter().any(|&r| r == 1 || r == 2) {
            return (ordered, formatted);
        }

        let mut indexed: Vec<_> = ordered
            .into_iter()
            .zip(formatted)
            .zip(ranks)
            .enumerate()
            .map(|(idx, ((type_id, display), rank))| (idx, rank, type_id, display))
            .collect();
        indexed.sort_by_key(|&(idx, rank, _, _)| (rank, idx));
        let ordered = indexed.iter().map(|(_, _, type_id, _)| *type_id).collect();
        let formatted = indexed
            .into_iter()
            .map(|(_, _, _, display)| display)
            .collect();
        (ordered, formatted)
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
            // A union member can reach format_union_member as a wrapped form
            // (Lazy/Application/anonymous-object intersection) whose lookup is
            // not itself an Intersection. tsc still parenthesizes such members
            // when the rendered text reads like an intersection — detect that
            // by a top-level ` & ` in the output and wrap to match.
            _ => contains_top_level_intersection_separator(&formatted),
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
    /// so the output matches tsc (e.g., `Foo.Yep | Bar.Yep`). When namespace
    /// qualification still leaves collisions (or is unavailable for plain
    /// cross-file class references), fall back to `import("<specifier>").Name`.
    ///
    /// tsc also applies `import(...)` qualification to namespace-qualified slots
    /// even when no collision remains after Pass 1 — specifically for types that
    /// live in a foreign module (not from `declare global {}`). This matches the
    /// output `import("renderer2").predom.JSX.Element` rather than just
    /// `predom.JSX.Element`.
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
        // First pass: prefer namespace qualification.
        // Track which slots were namespace-qualified (changed from original name).
        let mut was_ns_qualified: Vec<bool> = vec![false; formatted.len()];
        let mut result: Vec<String> = formatted
            .iter()
            .zip(members.iter())
            .enumerate()
            .map(|(i, (name, &member))| {
                if counts.get(name.as_str()).copied().unwrap_or(0) > 1
                    && let Some(qualified) = self.namespace_qualified_name_for_type(member)
                    && qualified != *name
                {
                    was_ns_qualified[i] = true;
                    return qualified;
                }
                name.clone()
            })
            .collect();
        // Second pass: apply import-qualification to:
        //   (a) slots that still collide after namespace qualification, OR
        //   (b) slots that WERE namespace-qualified in Pass 1 — tsc always
        //       import-qualifies these when the type comes from a foreign module.
        // Exception: skip types from `declare global {}` augmentations since
        // they are globally accessible and tsc never import-qualifies them.
        let mut second_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for name in &result {
            *second_counts.entry(name.clone()).or_default() += 1;
        }
        let any_still_colliding = second_counts.values().any(|&c| c > 1);
        let any_ns_qualified = was_ns_qualified.iter().any(|&q| q);
        if any_still_colliding || any_ns_qualified {
            for (i, (slot, &member)) in result.iter_mut().zip(members.iter()).enumerate() {
                let still_collides = second_counts.get(slot.as_str()).copied().unwrap_or(0) > 1;
                let ns_qualified = was_ns_qualified[i];
                if (still_collides || ns_qualified)
                    && !self.is_global_augmentation_type(member)
                    && let Some(qualified) = self.import_qualified_name_for_type(member)
                    && qualified != *slot
                {
                    *slot = qualified;
                }
            }
        }
        result
    }

    /// Returns `true` when `type_id` comes from a `declare global {}` augmentation.
    /// Such types are globally accessible and tsc never import-qualifies them.
    fn is_global_augmentation_type(&self, type_id: TypeId) -> bool {
        let Some(def_store) = self.def_store else {
            return false;
        };
        // Try Lazy(DefId)
        if let Some(def_id) = crate::type_queries::get_lazy_def_id(self.interner, type_id)
            && let Some(def) = def_store.get(def_id)
        {
            return def.is_global_augmentation;
        }
        // Try Object/ObjectWithIndex with symbol
        if let Some(shape) = crate::type_queries::get_object_shape(self.interner, type_id)
            && let Some(sym_id) = shape.symbol
            && let Some(def_id) = def_store.find_def_by_symbol(sym_id.0)
            && let Some(def) = def_store.get(def_id)
        {
            return def.is_global_augmentation;
        }
        false
    }

    /// Format two types for a diagnostic message where both appear side by
    /// side (e.g., `Type '<a>' is not assignable to type '<b>'`). When the
    /// plain display names collide, re-qualifies via namespace / import so
    /// the reader can tell which type is which.
    pub fn format_pair_disambiguated(&mut self, a: TypeId, b: TypeId) -> (String, String) {
        let sa = self.format(a).into_owned();
        let sb = self.format(b).into_owned();
        if sa != sb || a == b {
            return (sa, sb);
        }
        let ids = [a, b];
        let disambiguated =
            self.disambiguate_union_member_names(&ids, vec![sa.clone(), sb.clone()]);
        let mut iter = disambiguated.into_iter();
        let da = iter.next().unwrap_or(sa);
        let db = iter.next().unwrap_or(sb);
        (da, db)
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

    /// Locate the primary declaring symbol for a `TypeId`, when one exists —
    /// used by cross-file disambiguation to find the `decl_file_idx`.
    fn primary_symbol_for_type(&self, type_id: TypeId) -> Option<SymbolId> {
        if let Some(shape) = crate::type_queries::get_object_shape(self.interner, type_id) {
            if let Some(sym_id) = shape.symbol {
                return Some(sym_id);
            }
            if let Some(def_store) = self.def_store
                && let Some(def_id) = def_store.find_def_by_shape(&shape)
                && let Some(def) = def_store.get(def_id)
                && let Some(sym_raw) = def.symbol_id
            {
                return Some(SymbolId(sym_raw));
            }
        }
        if let Some(def_id) = crate::type_queries::get_lazy_def_id(self.interner, type_id)
            && let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
            && let Some(sym_raw) = def.symbol_id
        {
            return Some(SymbolId(sym_raw));
        }
        if let Some(def_id) = crate::type_queries::get_enum_def_id(self.interner, type_id)
            && let Some(def_store) = self.def_store
            && let Some(def) = def_store.get(def_id)
            && let Some(sym_raw) = def.symbol_id
        {
            return Some(SymbolId(sym_raw));
        }
        None
    }

    /// Format a type using `import("<specifier>").Name` qualification when the
    /// type's primary declaring symbol lives in a file different from the file
    /// currently being checked. Returns `None` when no such qualification is
    /// available (type has no symbol, no def-store, same file, or no module
    /// specifier is registered for the foreign file).
    fn import_qualified_name_for_type(&mut self, type_id: TypeId) -> Option<String> {
        let sym_id = self.primary_symbol_for_type(type_id)?;
        let arena = self.symbol_arena?;
        let sym = arena.get(sym_id)?;
        let decl_file_idx = sym.decl_file_idx;
        if decl_file_idx == u32::MAX {
            return None;
        }
        if self.current_file_id == Some(decl_file_idx) {
            return None;
        }
        let specifier = self
            .module_path_specifiers
            .and_then(|m| m.get(&decl_file_idx))
            .or_else(|| self.module_specifiers.and_then(|m| m.get(&decl_file_idx)))?;
        // Use the namespace-qualified short name so surrounding namespace
        // ancestors (e.g. `predom.JSX.Element`) survive the `import(...)`
        // prefix and produce `import("<path>").predom.JSX.Element`.
        let base = self.format_symbol_name(sym_id)?;
        let qualified = self.namespace_qualify_symbol_name(sym_id, base);
        Some(format!("import(\"{specifier}\").{qualified}"))
    }

    pub(super) fn format_intersection(&mut self, members: &[TypeId]) -> String {
        // Preserve the member order as stored in the TypeListId.
        // For intersections containing Lazy types (type parameters, type aliases),
        // normalize_intersection skips sorting and preserves source/declaration order.
        // tsc also preserves the original declaration order, so displaying members
        // in their stored order matches tsc's behavior.
        //
        // Do NOT flatten `{ a } & { b }` into `{ a; b }` at the display layer.
        // tsc's `typeToString` preserves the intersection form (`A & B`); a merged
        // single-object display is only produced when the type is already stored
        // as a single object (e.g. via spread/apparent-type computation), not
        // when an IntersectionType is printed directly.

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

    /// Format an intersection member, parenthesizing types that contain infix
    /// operators (`|`, `=>`) to maintain correct precedence in `A & B` display.
    fn format_intersection_member(&mut self, id: TypeId) -> String {
        // tsc displays primitive members of intersection types using their apparent
        // (boxed) names: `number` → `Number`, `string` → `String`, `boolean` → `Boolean`.
        if self.capitalize_primitive_intersection_members {
            if id == TypeId::NUMBER {
                return "Number".to_string();
            }
            if id == TypeId::STRING {
                return "String".to_string();
            }
            if id == TypeId::BOOLEAN {
                return "Boolean".to_string();
            }
        }
        let formatted = self.format(id);
        let needs_parens = match self.interner.lookup(id) {
            // Unions: `A | B & C` is ambiguous
            Some(TypeData::Union(_)) => formatted.contains(" | "),
            // Function/callable types: `(a: T) => R & S` is ambiguous —
            // `&` would parse as part of the return type
            Some(TypeData::Function(_) | TypeData::Callable(_)) => formatted.contains(" => "),
            _ => false,
        };
        if needs_parens {
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
        // to match tsc's display behavior.  Named rest elements (`[...urls: string[]]`)
        // are also simplified because the label is irrelevant in type display.
        if elements.len() == 1 && elements[0].rest {
            let inner = self.format(elements[0].type_id);
            return inner.into_owned();
        }
        // Format each element's type independently, then apply namespace
        // disambiguation across elements whose display names collide —
        // e.g. `[Foo.Yep, Bar.Yep]` instead of `[Yep, Yep]` when two different
        // named types share the same short name.
        let type_strs: Vec<String> = elements
            .iter()
            .map(|e| {
                if e.optional && !e.rest {
                    self.format_stripping_undefined(e.type_id)
                } else {
                    self.format(e.type_id).into_owned()
                }
            })
            .collect();
        let type_ids: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
        let disambiguated = self.disambiguate_union_member_names(&type_ids, type_strs);

        let formatted: Vec<String> = elements
            .iter()
            .zip(disambiguated)
            .map(|(e, type_str)| {
                let rest = if e.rest { "..." } else { "" };
                // Rest elements are never printed with `?` in tsc
                let optional = if e.optional && !e.rest { "?" } else { "" };
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
        let mut call_signatures: Vec<_> = shape.call_signatures.iter().collect();
        if call_signatures.iter().any(|sig| sig.params.is_empty())
            && call_signatures.iter().any(|sig| !sig.params.is_empty())
        {
            call_signatures.sort_by_key(|sig| sig.params.len());
        }
        for sig in call_signatures {
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
        let constraint_str = self.format(mapped.constraint);
        let as_clause = if let Some(name_type) = mapped.name_type {
            format!(" as {}", self.format(name_type))
        } else {
            String::new()
        };
        format!(
            "{{ {readonly_prefix}[{param_name} in {constraint_str}{as_clause}]{optional_suffix}: {template_display}; }}"
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
        // The empty object `{}` is a universally-shared shape. `find_def_by_shape`
        // is keyed on structural hash, so any *type alias* registered with an
        // empty body (e.g., `type T52 = T50<unknown>` reducing to `{}`) would
        // repaint every user-written `{}` annotation with that alias's name.
        // Skip the def-name fallback for empty anonymous shapes when the
        // matched def is a type alias; named empty types (interfaces, classes)
        // still resolve through `shape.symbol` above this guard, and lib
        // interfaces below are unaffected because the Object-interface special
        // case handles the only realistic empty lib shape.
        let shape_is_empty_anonymous = shape.symbol.is_none()
            && shape.properties.is_empty()
            && shape.string_index.is_none()
            && shape.number_index.is_none();
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
        //
        // Exception: for empty anonymous shapes (`{}`), skip the fallback
        // when the matched def is a type alias. Any alias whose body reduces
        // to `{}` (e.g., `type T52 = T50<unknown>`) would otherwise repaint
        // every user-written `{}` annotation with the alias name; tsc shows
        // the literal `{}` in that case. Lib interfaces do not have empty
        // shapes, so the guard never hides them.
        if let Some(def_store) = self.def_store
            && let Some(def_id) = def_store.find_def_by_shape(shape)
            && let Some(def) = def_store.get(def_id)
        {
            use crate::def::DefKind;
            let skip_for_empty_alias = shape_is_empty_anonymous && def.kind == DefKind::TypeAlias;
            if !skip_for_empty_alias {
                return Some(self.format_def_name(&def));
            }
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
    /// Resolve the variable name for a unique symbol, for use in `typeof varName` display.
    /// Public so callers outside the format module can use this (e.g., TS2367 path).
    pub fn resolve_unique_symbol_name(&mut self, sym: SymbolRef) -> Option<String> {
        self.resolve_symbol_ref_name(sym)
    }

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

        // Type parameters are modeled as `TypeData::TypeParameter` and lose direct
        // declaration span information unless the checker registers their DefId.
        // When available, use that DefId span so diagnostics can display unions in
        // declaration/source order (e.g. `Top | T | U` instead of alloc-order drift).
        if matches!(data, Some(TypeData::TypeParameter(_) | TypeData::Infer(_)))
            && let Some(def_id) = def_store.find_def_for_type(type_id)
            && let Some(def) = def_store.get(def_id)
            && let (Some(file_id), Some((span_start, _))) = (def.file_id, def.span)
        {
            return (1, file_id, span_start);
        }

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

/// Detects whether `s` reads as an intersection at the top level — i.e.
/// contains a ` & ` separator outside any brackets, parens, or braces.
/// Used by union-member parenthesization when the lookup-based heuristic
/// can't see through Lazy/Application wrappers.
fn contains_top_level_intersection_separator(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b'&' if depth == 0
                && i > 0
                && bytes[i - 1] == b' '
                && i + 1 < bytes.len()
                && bytes[i + 1] == b' ' =>
            {
                return true;
            }
            _ => {}
        }
        i += 1;
    }
    false
}
