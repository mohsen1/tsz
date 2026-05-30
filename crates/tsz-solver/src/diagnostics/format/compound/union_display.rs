impl<'a> TypeFormatter<'a> {
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
        // - Tier 0/1 (builtins/user types with source): sort by source position
        //   and place at the front of the union display.
        // - Tier 2 (anonymous objects, literal types without a source pos): keep
        //   their relative declaration order and place AFTER tier 0/1.
        //
        // The split-then-sort approach matches tsc's display preference of
        // showing named/built-in types before anonymous/literal members
        // (e.g. `Refrigerator | "foo"` rather than `"foo" | Refrigerator`),
        // while still preserving discriminated-union order for anonymous
        // object members where tsc keeps declaration order.
        if let Some(def_store) = self.def_store {
            let positions: Vec<_> = ordered
                .iter()
                .map(|&m| self.get_source_position_for_type(m, def_store))
                .collect();

            let mut named: Vec<(TypeId, (u32, u32, u32))> = Vec::new();
            let mut anonymous: Vec<TypeId> = Vec::new();
            for (&id, &pos) in ordered.iter().zip(&positions) {
                if pos.0 < 2 {
                    named.push((id, pos));
                } else {
                    anonymous.push(id);
                }
            }
            named.sort_by_key(|&(_, pos)| pos);

            ordered = named
                .into_iter()
                .map(|(id, _)| id)
                .chain(anonymous)
                .collect();
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

        // tsc's `typeof` operator produces a fixed eight-element string
        // literal union; when the diagnostic surface displays that exact
        // union, tsc renders it in JS-spec order regardless of how the
        // interner pre-sorted the union members. Without this carve-out,
        // the global literal-id allocation order can put `"symbol"` ahead
        // of `"string"`, leaking the interner's history into TS2367
        // overlap diagnostics.
        if let Some(reordered) = self.reorder_typeof_result_union_in_canonical_order(&ordered) {
            ordered = reordered;
        }

        if let Some(factored) = self.format_union_of_intersections_with_common_parts(&ordered) {
            return factored;
        }

        self.format_ordered_union_members(ordered)
    }
    pub(super) fn format_union_preserving_member_order(&mut self, members: &[TypeId]) -> String {
        self.format_ordered_union_members(members.to_vec())
    }

    fn format_ordered_union_members(&mut self, mut ordered: Vec<TypeId>) -> String {
        if let Some(collapsed) = self.collapse_same_enum_members_for_display(&ordered) {
            return collapsed;
        }

        // Drop synthetic `"__unique_<n>"` string-literal members. These come
        // from `keyof` over interfaces with unique-symbol-keyed properties:
        // tsc renders the union without the synthetic atom (e.g. `"first" |
        // "second"` instead of `"first" | "second" | "__unique_3006"`). Only
        // strip when at least one non-synthetic string literal remains so we
        // don't reduce a union to nothing.
        let synthetic_unique_count = ordered
            .iter()
            .filter(|&&m| self.is_synthetic_unique_atom_string_literal(m))
            .count();
        if synthetic_unique_count > 0 && synthetic_unique_count < ordered.len() {
            ordered.retain(|&m| !self.is_synthetic_unique_atom_string_literal(m));
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
        // If qualification couldn't break a tie (or slots were already
        // identical), tsc collapses the duplicates to a single member.
        // Mirror that — `Symbol | Symbol` should display as just `Symbol`.
        let mut deduped: Vec<String> = Vec::with_capacity(disambiguated.len());
        let mut seen: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        for name in disambiguated {
            if seen.insert(name.clone()) {
                deduped.push(name);
            }
        }
        deduped.join(" | ")
    }

    /// When the union members exactly match the eight string literals that
    /// the JavaScript `typeof` operator can produce, return them in tsc's
    /// canonical display order. Otherwise return `None` so the caller
    /// keeps the input order untouched.
    ///
    /// Detected purely by string-literal value equality on the closed
    /// `typeof` result vocabulary — this set is fixed by the JS spec, not
    /// a user-chosen identifier, so the check is structural rather than
    /// a printer-output-driven decision.
    fn reorder_typeof_result_union_in_canonical_order(
        &self,
        members: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        const TYPEOF_RESULT_ORDER: [&str; 8] = [
            "string",
            "number",
            "bigint",
            "boolean",
            "symbol",
            "undefined",
            "object",
            "function",
        ];
        if members.len() != TYPEOF_RESULT_ORDER.len() {
            return None;
        }
        let mut found: [Option<TypeId>; 8] = [None; 8];
        for &member in members {
            let Some(crate::types::TypeData::Literal(crate::types::LiteralValue::String(atom))) =
                self.interner.lookup(member)
            else {
                return None;
            };
            let value = self.interner.resolve_atom(atom);
            let idx = TYPEOF_RESULT_ORDER
                .iter()
                .position(|&v| v == value.as_str())?;
            if found[idx].is_some() {
                return None;
            }
            found[idx] = Some(member);
        }
        let reordered: Vec<TypeId> = found.into_iter().collect::<Option<Vec<_>>>()?;
        Some(reordered)
    }

    fn format_union_of_intersections_with_common_parts(
        &mut self,
        members: &[TypeId],
    ) -> Option<String> {
        if members.len() < 2 {
            return None;
        }

        let mut common: Option<Vec<TypeId>> = None;
        let mut union_remainders = Vec::with_capacity(members.len());
        for &member in members {
            let intersection_members = match self.interner.lookup(member) {
                Some(TypeData::Intersection(list_id)) => {
                    self.interner.type_list(list_id).as_ref().to_vec()
                }
                _ => return None,
            };
            if intersection_members.len() < 2 {
                return None;
            }

            let mut numeric_literal_parts = Vec::new();
            let mut non_numeric_parts = Vec::new();
            for part in intersection_members {
                if matches!(
                    self.interner.lookup(part),
                    Some(TypeData::Literal(LiteralValue::Number(_)))
                ) {
                    numeric_literal_parts.push(part);
                } else {
                    non_numeric_parts.push(part);
                }
            }

            if numeric_literal_parts.len() != 1 || non_numeric_parts.is_empty() {
                return None;
            }

            match &common {
                Some(common_parts) if common_parts == &non_numeric_parts => {}
                Some(_) => return None,
                None => common = Some(non_numeric_parts),
            }
            union_remainders.push(numeric_literal_parts[0]);
        }

        union_remainders.sort_by(|&left, &right| {
            let left_number = match self.interner.lookup(left) {
                Some(TypeData::Literal(LiteralValue::Number(number))) => number,
                _ => return std::cmp::Ordering::Equal,
            };
            let right_number = match self.interner.lookup(right) {
                Some(TypeData::Literal(LiteralValue::Number(number))) => number,
                _ => return std::cmp::Ordering::Equal,
            };
            let left_zero = left_number.0.to_bits() == 0.0f64.to_bits();
            let right_zero = right_number.0.to_bits() == 0.0f64.to_bits();
            right_zero
                .cmp(&left_zero)
                .then_with(|| right_number.0.total_cmp(&left_number.0))
        });

        let common = common?;
        let common_display = common
            .iter()
            .map(|&part| self.format_intersection_member(part))
            .collect::<Vec<_>>()
            .join(" & ");
        // Use sorted union_remainders directly rather than round-tripping through
        // self.interner.union(), which canonically re-sorts by TypeId and discards
        // the custom numeric sort order above (e.g. `0 | 2 | 1` → `0 | 1 | 2`).
        let remainder_display = self.format_ordered_union_members(union_remainders);
        Some(format!("{common_display} & ({remainder_display})"))
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

        // tsc collapses a union of enum members to the bare enum name only
        // when the union covers EVERY member of the enum. A proper subset
        // (e.g. `E.A | E.B` of a three-member enum) must render member by
        // member as `E.A | E.B`. When the full parent enum type itself is
        // present in the union, coverage is guaranteed, so this gate applies
        // only to the individual-member case (`parent_enum_name` unset).
        if parent_enum_name.is_none()
            && let Some(member_def) = any_enum_member_def_id
            && let Some(total) = self.enum_total_member_count(member_def)
            && enum_member_count < total
        {
            return None;
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

    /// Number of distinct member values declared by the enum that owns
    /// `member_def_id`, derived from the parent enum's structural type.
    /// Returns `None` when the parent or its structural type can't be
    /// resolved (the caller then keeps its existing collapse behavior).
    fn enum_total_member_count(&self, member_def_id: crate::def::DefId) -> Option<usize> {
        let def_store = self.def_store?;
        let arena = self.symbol_arena?;
        let sym_id = def_store.get(member_def_id)?.symbol_id?;
        let symbol = arena.get(SymbolId(sym_id))?;
        let parent_def_id = def_store.find_def_by_symbol(symbol.parent.0)?;
        let structural = def_store.get(parent_def_id)?.body?;
        let count = self.collect_leaf_literal_ids(structural).len();
        (count > 0).then_some(count)
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
    /// True when `id` is a string literal whose atom is one of the
    /// synthetic `__unique_<n>` placeholders for unique-symbol keys.
    fn is_synthetic_unique_atom_string_literal(&self, id: TypeId) -> bool {
        let Some(TypeData::Literal(LiteralValue::String(atom))) = self.interner.lookup(id) else {
            return false;
        };
        let s = self.interner.resolve_atom(atom);
        let Some(suffix) = s.strip_prefix("__unique_") else {
            return false;
        };
        !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
    }

    fn format_union_member(&mut self, id: TypeId) -> String {
        if let Some(enum_name) = self.short_enum_name_for_union_display(id) {
            return enum_name;
        }

        let formatted = self.format(id);
        let needs_parens = match self.interner.lookup(id) {
            Some(TypeData::Intersection(_)) => {
                !formatted.starts_with("NonNullable<")
                    && contains_top_level_intersection_separator(&formatted)
            }
            Some(TypeData::Function(_)) => true,
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
        if type_id.is_intrinsic() {
            return false;
        }
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
}
