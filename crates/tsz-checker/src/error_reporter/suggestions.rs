//! Spelling suggestion helpers (Levenshtein distance, property/identifier suggestions).

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

/// Built-in TypeScript type keywords that tsc registers as intrinsic types
/// in the checker's globals. Used as candidates for spelling suggestions
/// so that typos like "sting" → "string" produce TS2552.
const BUILTIN_TYPE_KEYWORDS: &[&str] = &[
    "string",
    "number",
    "boolean",
    "symbol",
    "bigint",
    "void",
    "never",
    "any",
    "unknown",
    "undefined",
    "null",
    "object",
];

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Property Suggestion Helpers
    // =========================================================================

    /// Find a similar property name on a type for "did you mean?" suggestions (TS2551).
    /// Uses the same algorithm as tsc's `getSpellingSuggestion`.
    pub(super) fn find_similar_property(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
    ) -> Option<String> {
        // Private named fields (#foo) are a fundamentally different access mechanism;
        // never suggest public properties as "did you mean?" alternatives.
        if prop_name.starts_with('#') {
            return None;
        }

        let evaluated_type = self.evaluate_type_for_assignability(type_id);
        let property_names = self.collect_accessible_type_property_names(evaluated_type);
        if property_names.is_empty() {
            return None;
        }

        let name_len = prop_name.len();
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        let mut best_distance = (name_len * 4 / 10 + 1) as f64;
        let mut best_candidate: Option<String> = None;

        for candidate in &property_names {
            Self::consider_identifier_suggestion(
                prop_name,
                candidate,
                name_len,
                maximum_length_difference,
                &mut best_distance,
                &mut best_candidate,
            );
        }

        best_candidate
    }

    /// Returns true when `type_id` is a class instance type whose declaration extends
    /// a base expression currently typed as `any`.
    ///
    /// In that case TypeScript treats unknown member accesses as `any` and does not
    /// surface typo suggestions (TS2551).
    pub(super) fn class_extends_any_base(&mut self, type_id: TypeId) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        let sym_id = self
            .ctx
            .resolve_type_to_symbol_id(type_id)
            .or_else(|| tsz_solver::type_queries::get_type_shape_symbol(self.ctx.types, type_id));
        let Some(sym_id) = sym_id else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.flags & symbol_flags::CLASS == 0 {
            return false;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };
        let Some(class_decl) = self.ctx.arena.get_class_at(decl_idx) else {
            return false;
        };
        let Some(heritage_clauses) = &class_decl.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let Some(&type_idx) = clause.types.nodes.first() else {
                continue;
            };
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args_at(type_idx) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            if self.get_type_of_node(expr_idx) == TypeId::ANY {
                return true;
            }
        }

        false
    }

    fn collect_accessible_type_property_names(&self, type_id: TypeId) -> Vec<String> {
        // For enum types, the solver can't access binder exports.
        // Collect enum member names directly from the binder's symbol exports.
        if let Some(def_id) = tsz_solver::type_queries::get_enum_def_id(self.ctx.types, type_id)
            && let Some(&sym_id) = self.ctx.def_to_symbol.borrow().get(&def_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && let Some(exports) = symbol.exports.as_ref()
        {
            return exports.iter().map(|(name, _)| name.clone()).collect();
        }

        // For namespace/module types (e.g., `namespace A { ... }`), the solver's
        // traversal classifies TypeQuery/Lazy as Terminal and returns no properties.
        // Collect exported member names from the binder's symbol exports so that
        // spelling suggestions (TS2551 "Did you mean?") work on namespace accesses.
        if let Some(sym_id) = self.resolve_namespace_symbol_for_type(type_id)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::MODULE) != 0
            && (symbol.flags & tsz_binder::symbol_flags::ENUM) == 0
            && let Some(exports) = symbol.exports.as_ref()
        {
            return exports.iter().map(|(name, _)| name.clone()).collect();
        }

        // For primitive types (string, number, boolean, bigint, symbol), the solver's
        // traversal classifies them as Terminal and returns no properties. Resolve to
        // their boxed interface types (String, Number, etc.) so spelling suggestions
        // can find methods like `fixed` on string (TS2551 "Did you mean 'fixed'?").
        let resolved_type = self
            .resolve_primitive_to_boxed_type(type_id)
            .unwrap_or(type_id);

        crate::query_boundaries::diagnostics::collect_accessible_property_names_for_suggestion(
            self.ctx.types,
            resolved_type,
            5,
        )
        .into_iter()
        .map(|name| self.ctx.types.resolve_atom_ref(name).to_string())
        .collect()
    }

    /// Map a primitive type to its boxed interface type for property name collection.
    /// Returns `None` if the type is not a primitive or no boxed type is registered.
    fn resolve_primitive_to_boxed_type(&self, type_id: TypeId) -> Option<TypeId> {
        use tsz_solver::IntrinsicKind;
        use tsz_solver::def::resolver::TypeResolver;

        let kind = if tsz_solver::type_queries::is_string_type(self.ctx.types, type_id)
            || tsz_solver::type_queries::is_string_literal(self.ctx.types, type_id)
        {
            IntrinsicKind::String
        } else if tsz_solver::type_queries::is_number_type(self.ctx.types, type_id)
            || tsz_solver::type_queries::is_number_literal(self.ctx.types, type_id)
        {
            IntrinsicKind::Number
        } else if tsz_solver::type_queries::is_boolean_type(self.ctx.types, type_id) {
            IntrinsicKind::Boolean
        } else if tsz_solver::type_queries::is_bigint_type(self.ctx.types, type_id) {
            IntrinsicKind::Bigint
        } else if tsz_solver::type_queries::is_symbol_type(self.ctx.types, type_id) {
            IntrinsicKind::Symbol
        } else {
            return None;
        };

        TypeResolver::get_boxed_type(self.ctx.types, kind)
    }

    /// Resolve the binder SymbolId for a namespace/module type.
    /// Uses `classify_namespace_member` to find the symbol backing the type.
    fn resolve_namespace_symbol_for_type(&self, type_id: TypeId) -> Option<tsz_binder::SymbolId> {
        use tsz_solver::type_queries::{NamespaceMemberKind, classify_namespace_member};

        match classify_namespace_member(self.ctx.types, type_id) {
            NamespaceMemberKind::Lazy(def_id) => self.ctx.def_to_symbol_id(def_id),
            NamespaceMemberKind::TypeQuery(sym_ref) => Some(tsz_binder::SymbolId(sym_ref.0)),
            NamespaceMemberKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                shape.symbol
            }
            _ => None,
        }
    }

    // =========================================================================
    // Identifier Suggestion Helpers
    // =========================================================================

    pub(crate) fn consider_identifier_suggestion(
        name: &str,
        candidate: &str,
        name_len: usize,
        maximum_length_difference: usize,
        best_distance: &mut f64,
        best_candidate: &mut Option<String>,
    ) {
        if candidate == name {
            return;
        }
        let candidate_len = candidate.len();

        // tsc: skip candidates whose length is too different
        let len_diff = name_len.abs_diff(candidate_len);
        if len_diff > maximum_length_difference {
            return;
        }

        // tsc: for short candidate names (<3), only suggest if differs by case
        if candidate_len < 3 && candidate.to_lowercase() != name.to_lowercase() {
            return;
        }

        if let Some(distance) = Self::levenshtein_with_max(name, candidate, *best_distance - 0.1)
            && distance < *best_distance
        {
            *best_distance = distance;
            *best_candidate = Some(candidate.to_string());
        }
    }

    /// Find the best spelling suggestion for a name, matching tsc's `getSpellingSuggestion`.
    /// Returns `Some(best_name)` if a close-enough match is found.
    ///
    /// `meaning_flags` controls which symbol categories are eligible as candidates.
    /// Pass `symbol_flags::VALUE` for value-position lookups, `symbol_flags::TYPE` for
    /// type-position lookups, or `symbol_flags::VALUE | symbol_flags::TYPE` for both.
    pub(crate) fn find_similar_identifiers(
        &self,
        name: &str,
        idx: NodeIndex,
        meaning_flags: u32,
    ) -> Option<Vec<String>> {
        let visible_names = self.ctx.binder.collect_visible_symbol_names_filtered(
            self.ctx.arena,
            idx,
            meaning_flags,
        );

        let name_len = name.len();
        // tsc: bestDistance = (name.length + 2) * 0.34 rounded down, min 2
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        // tsc: initial bestDistance = floor(name.length * 0.4) + 1
        let mut best_distance = (name_len * 4 / 10 + 1) as f64;
        let mut best_candidate: Option<String> = None;

        for candidate in visible_names {
            Self::consider_identifier_suggestion(
                name,
                &candidate,
                name_len,
                maximum_length_difference,
                &mut best_distance,
                &mut best_candidate,
            );
        }

        Self::search_global_candidates(
            name,
            name_len,
            maximum_length_difference,
            meaning_flags,
            &self.ctx.lib_contexts,
            &mut best_distance,
            &mut best_candidate,
        );

        best_candidate.map(|c| vec![c])
    }

    /// Search lib globals and built-in type keywords for spelling suggestion
    /// candidates. Extracted so both `find_similar_identifiers` (AST-based) and
    /// `find_jsdoc_type_spelling_suggestion` (position-based) can share this.
    fn search_global_candidates(
        name: &str,
        name_len: usize,
        maximum_length_difference: usize,
        meaning_flags: u32,
        lib_contexts: &[crate::context::LibContext],
        best_distance: &mut f64,
        best_candidate: &mut Option<String>,
    ) {
        // Search lib globals (Array, Map, Set, Promise, etc.).
        // tsc searches the full scope chain including the global scope from
        // lib.d.ts, so names like `array2` → `Array` are suggested.
        for lib_ctx in lib_contexts {
            for (symbol_name, sym_id) in lib_ctx.binder.file_locals.iter() {
                // Apply the same meaning filter as the local search.
                if meaning_flags != 0
                    && let Some(sym) = lib_ctx.binder.get_symbol(*sym_id)
                    && sym.flags & meaning_flags == 0
                {
                    continue;
                }
                Self::consider_identifier_suggestion(
                    name,
                    symbol_name,
                    name_len,
                    maximum_length_difference,
                    best_distance,
                    best_candidate,
                );
            }
        }

        // When searching for TYPE meanings, also include built-in type keywords.
        // In tsc, intrinsic types (string, number, boolean, etc.) are registered
        // in the checker's globals map. We include them here as candidates so
        // that typos like "sting" → "string" are caught.
        if meaning_flags == 0 || meaning_flags & tsz_binder::symbol_flags::TYPE != 0 {
            for keyword in BUILTIN_TYPE_KEYWORDS {
                Self::consider_identifier_suggestion(
                    name,
                    keyword,
                    name_len,
                    maximum_length_difference,
                    best_distance,
                    best_candidate,
                );
            }
        }
    }

    /// Find a spelling suggestion for a JSDoc type name without an AST node.
    ///
    /// Used by `emit_jsdoc_cannot_find_name` where positions are computed from
    /// comment text offsets and no `NodeIndex` is available. Searches file-level
    /// scope symbols, lib globals, and built-in type keywords.
    pub(crate) fn find_jsdoc_type_spelling_suggestion(&self, name: &str) -> Option<String> {
        let name_len = name.len();
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        let mut best_distance = (name_len * 4 / 10 + 1) as f64;
        let mut best_candidate: Option<String> = None;

        // Search file-level symbols (the binder's file_locals).
        for (symbol_name, sym_id) in self.ctx.binder.file_locals.iter() {
            if let Some(sym) = self.ctx.binder.get_symbol(*sym_id)
                && sym.flags & tsz_binder::symbol_flags::TYPE == 0
            {
                continue;
            }
            Self::consider_identifier_suggestion(
                name,
                symbol_name,
                name_len,
                maximum_length_difference,
                &mut best_distance,
                &mut best_candidate,
            );
        }

        // Search lib globals and built-in type keywords.
        Self::search_global_candidates(
            name,
            name_len,
            maximum_length_difference,
            tsz_binder::symbol_flags::TYPE,
            &self.ctx.lib_contexts,
            &mut best_distance,
            &mut best_candidate,
        );

        best_candidate
    }

    // =========================================================================
    // Lib Target Suggestion Helpers (TS2550)
    // =========================================================================

    /// Check if a missing property is available in a newer lib version.
    pub(super) fn get_lib_suggestion_for_property(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
    ) -> Option<&'static str> {
        if let Some(type_name) = self.get_type_symbol_name(type_id)
            && let Some(lib) = get_lib_for_type_property(&type_name, prop_name)
        {
            return Some(lib);
        }
        None
    }

    /// Check if a missing property is available in a newer lib version,
    /// using the property name AST node to identify the parent object
    /// expression for well-known global constructor types.
    /// Returns `(lib_name, type_display_name)` so the caller can use the
    /// correct type name in the TS2550 message.
    pub(super) fn get_lib_suggestion_for_property_with_node(
        &mut self,
        prop_name: &str,
        type_id: TypeId,
        prop_node: NodeIndex,
    ) -> Option<(&'static str, Option<&'static str>)> {
        // First try type-based lookup
        if let Some(lib) = self.get_lib_suggestion_for_property(prop_name, type_id) {
            return Some((lib, None));
        }

        // Fallback: check if the parent expression is a well-known global identifier.
        // This handles cases like `Object.values` where the ObjectConstructor type
        // can't be resolved to its name through the type system alone.
        let parent_idx = self.ctx.arena.get_extended(prop_node)?.parent;
        let parent = self.ctx.arena.get(parent_idx)?;
        if parent.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(parent)?;
        let obj_node = self.ctx.arena.get(access.expression)?;
        let ident = self.ctx.arena.get_identifier(obj_node)?;
        let constructor_name = match ident.escaped_text.as_str() {
            "Object" => "ObjectConstructor",
            "Symbol" => "Symbol",
            "Map" => "MapConstructor",
            "Atomics" => "Atomics",
            _ => return None,
        };
        let lib = get_lib_for_type_property(constructor_name, prop_name)?;
        Some((lib, Some(constructor_name)))
    }

    /// Try to resolve the symbol name for a type.
    fn get_type_symbol_name(&mut self, type_id: TypeId) -> Option<String> {
        use tsz_solver::type_queries;

        if tsz_solver::is_array_type(self.ctx.types, type_id) {
            return Some("Array".to_string());
        }
        if type_id == TypeId::STRING || type_queries::is_string_literal(self.ctx.types, type_id) {
            return Some("String".to_string());
        }
        if type_id == TypeId::NUMBER || type_queries::is_number_literal(self.ctx.types, type_id) {
            return Some("Number".to_string());
        }
        if let Some(base) = type_queries::get_application_base(self.ctx.types, type_id) {
            return self.get_type_symbol_name(base);
        }

        let sym_id = self
            .ctx
            .resolve_type_to_symbol_id(type_id)
            .or_else(|| type_queries::get_type_shape_symbol(self.ctx.types, type_id));
        if let Some(sym_id) = sym_id {
            let lib_binders = self.get_lib_binders();
            if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                && !symbol.escaped_name.is_empty()
            {
                return Some(symbol.escaped_name.clone());
            }
            if let Some(def_id) = self.ctx.get_existing_def_id(sym_id)
                && let Some(def) = self.ctx.definition_store.get(def_id)
            {
                let name = self.ctx.types.resolve_atom(def.name);
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }

        if let Some(def_id) = self.ctx.definition_store.find_def_for_type(type_id)
            && let Some(def) = self.ctx.definition_store.get(def_id)
        {
            let name = self.ctx.types.resolve_atom(def.name);
            if !name.is_empty() {
                return Some(name);
            }
        }

        None
    }

    fn chars_equal_ignore_case(a: char, b: char) -> bool {
        a.to_lowercase().eq(b.to_lowercase())
    }

    // =========================================================================
    // String Literal Suggestion Helpers (TS2820)
    // =========================================================================

    // Find the best string literal suggestion from a union of string literals.
    // =========================================================================
    // Levenshtein Distance
    // =========================================================================

    /// Levenshtein distance with threshold pruning, matching tsc's behavior.
    /// Case-only substitutions are cheaper than other substitutions.
    pub(crate) fn levenshtein_with_max(s1: &str, s2: &str, max: f64) -> Option<f64> {
        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();

        if s1_chars.is_empty() {
            let dist = s2_chars.len() as f64;
            return (dist <= max).then_some(dist);
        }
        if s2_chars.is_empty() {
            let dist = s1_chars.len() as f64;
            return (dist <= max).then_some(dist);
        }

        let mut previous = vec![0.0; s2_chars.len() + 1];
        let mut current = vec![0.0; s2_chars.len() + 1];
        let big = max + 0.01;

        for (i, value) in previous.iter_mut().enumerate() {
            *value = i as f64;
        }

        for i in 1..=s1_chars.len() {
            let c1 = s1_chars[i - 1];
            let min_j = if (i as f64) > max {
                ((i as f64) - max).ceil() as usize
            } else {
                1
            };
            let max_j = if (s2_chars.len() as f64) > (max + i as f64) {
                (max + i as f64).floor() as usize
            } else {
                s2_chars.len()
            };

            current[0] = i as f64;
            let mut col_min = i as f64;

            for value in current.iter_mut().take(min_j).skip(1) {
                *value = big;
            }

            for j in min_j..=max_j {
                let substitution_distance = if Self::chars_equal_ignore_case(c1, s2_chars[j - 1]) {
                    previous[j - 1] + 0.1
                } else {
                    previous[j - 1] + 2.0
                };
                let dist = if c1 == s2_chars[j - 1] {
                    previous[j - 1]
                } else {
                    (previous[j] + 1.0)
                        .min(current[j - 1] + 1.0)
                        .min(substitution_distance)
                };
                current[j] = dist;
                col_min = col_min.min(dist);
            }

            for value in current.iter_mut().take(s2_chars.len() + 1).skip(max_j + 1) {
                *value = big;
            }

            if col_min > max {
                return None;
            }

            std::mem::swap(&mut previous, &mut current);
        }

        let result = previous[s2_chars.len()];
        (result <= max).then_some(result)
    }
}

/// Look up the minimum lib version required for a (type, property) pair.
fn get_lib_for_type_property(type_name: &str, prop_name: &str) -> Option<&'static str> {
    match type_name {
        "Array" | "ReadonlyArray" => match prop_name {
            "find" | "findIndex" | "fill" | "copyWithin" | "entries" | "keys" | "values" => {
                Some("es2015")
            }
            "includes" => Some("es2016"),
            "flatMap" | "flat" => Some("es2019"),
            "at" => Some("es2022"),
            "findLast" | "findLastIndex" => Some("es2023"),
            "toReversed" | "toSorted" | "toSpliced" | "with" => Some("esnext"),
            _ => None,
        },
        "SharedArrayBuffer" => match prop_name {
            "grow" | "growable" | "maxByteLength" => Some("esnext"),
            _ => Some("es2017"),
        },
        "Atomics" => match prop_name {
            "waitAsync" => Some("es2024"),
            _ => Some("es2017"),
        },
        "AsyncIterable" | "AsyncIterableIterator" | "AsyncGenerator" | "AsyncGeneratorFunction" => {
            Some("es2018")
        }
        "RegExp" => match prop_name {
            "dotAll" => Some("es2018"),
            "hasIndices" => Some("es2022"),
            _ => None,
        },
        "RegExpMatchArray" => match prop_name {
            "groups" => Some("es2018"),
            "indices" => Some("es2022"),
            _ => None,
        },
        "Symbol" => match prop_name {
            "asyncIterator" => Some("es2018"),
            "description" => Some("es2019"),
            _ => None,
        },
        "String" => match prop_name {
            "trimStart" | "trimEnd" | "trimLeft" | "trimRight" => Some("es2019"),
            "matchAll" => Some("es2020"),
            "replaceAll" => Some("es2021"),
            "at" => Some("es2022"),
            "isWellFormed" | "toWellFormed" => Some("esnext"),
            _ => None,
        },
        "ObjectConstructor" => match prop_name {
            "values" | "entries" => Some("es2017"),
            "fromEntries" => Some("es2019"),
            "hasOwn" => Some("es2022"),
            "groupBy" => Some("es2024"),
            _ => None,
        },
        "BigInt" => Some("es2020"),
        "BigInt64Array" | "BigUint64Array" => match prop_name {
            "at" => Some("es2022"),
            "findLast" | "findLastIndex" => Some("es2023"),
            "toReversed" | "toSorted" | "with" => Some("esnext"),
            _ => Some("es2020"),
        },
        "Promise" | "PromiseConstructor" => match prop_name {
            "allSettled" => Some("es2020"),
            "any" => Some("es2021"),
            _ => None,
        },
        "WeakRef" | "FinalizationRegistry" | "AggregateError" => Some("es2021"),
        "Int8Array" | "Uint8Array" | "Uint8ClampedArray" | "Int16Array" | "Uint16Array"
        | "Int32Array" | "Uint32Array" | "Float32Array" | "Float64Array" => match prop_name {
            "find" | "findIndex" | "fill" | "copyWithin" | "entries" | "keys" | "values" => {
                Some("es2015")
            }
            "includes" => Some("es2016"),
            "at" => Some("es2022"),
            "findLast" | "findLastIndex" => Some("es2023"),
            "toReversed" | "toSorted" | "with" => Some("esnext"),
            _ => None,
        },
        "Error" | "ErrorConstructor" => match prop_name {
            "cause" => Some("es2022"),
            _ => None,
        },
        "Map" | "Set" | "WeakMap" | "WeakSet" => Some("es2015"),
        "MapConstructor" => match prop_name {
            "groupBy" => Some("es2024"),
            _ => Some("es2015"),
        },
        "Intl" => match prop_name {
            "Segmenter" | "Segments" | "SegmentData" => Some("esnext"),
            _ => None,
        },
        "NumberConstructor" => match prop_name {
            "isFinite" | "isInteger" | "isNaN" | "isSafeInteger" | "parseFloat" | "parseInt"
            | "EPSILON" | "MAX_SAFE_INTEGER" | "MIN_SAFE_INTEGER" => Some("es2015"),
            _ => None,
        },
        _ => None,
    }
}
