//! Spelling suggestion helpers (Levenshtein distance, property/identifier suggestions).

use crate::state::CheckerState;
use crate::state_type_analysis::cross_file_direct::is_builtin_lib_file_name;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

/// Built-in TypeScript intrinsic **type** names that tsc registers as symbols
/// in the checker's globals. Used as candidates for spelling suggestions in
/// *type* position so that typos like "sting" → "string" produce TS2552.
///
/// NOTE: Only intrinsics that tsc registers as actual *type*-meaning symbol
/// entries in its global scope are included here. Keyword literals `null`,
/// `void`, `never`, `any`, and `unknown` are parsed syntactically and do NOT
/// appear in tsc's globals map, so they must NOT be offered as suggestions.
/// `undefined` is the exception: tsc registers it as a *value* symbol (see
/// [`BUILTIN_VALUE_KEYWORDS`]), so it is a value-position candidate, not a
/// type-position one.
const BUILTIN_TYPE_KEYWORDS: &[&str] =
    &["string", "number", "boolean", "symbol", "bigint", "object"];

/// Built-in TypeScript intrinsic **value** names that tsc registers as symbols
/// in the checker's globals. `undefined` is created as a global `Property`
/// symbol (tsc's `undefinedSymbol`), so a value-position typo such as
/// `udefined` suggests `undefined` (TS2552). It is *not* a type-meaning symbol,
/// so type-position lookups never offer it — matching tsc, which reports plain
/// TS2304 for `let x: undefindd`. `NaN`/`Infinity` are already real lib.es5
/// symbols and are found through the ordinary lib-globals scan.
const BUILTIN_VALUE_KEYWORDS: &[&str] = &["undefined"];

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

        // A discarded-diagnostics context never surfaces the diagnostic this
        // suggestion would decorate; skip the property-universe scan.
        if self.ctx.diagnostics_discarded {
            return None;
        }

        let evaluated_type = self.evaluate_type_for_assignability(type_id);
        let property_names = self.collect_accessible_type_property_names(type_id, evaluated_type);
        if property_names.is_empty() {
            return None;
        }

        Self::best_spelling_suggestion(prop_name, property_names.iter().map(String::as_str))
    }

    /// Returns true when `type_id` is a class instance type whose declaration extends
    /// a base expression currently typed as `any`.
    ///
    /// In that case TypeScript treats unknown member accesses as `any` and does not
    /// surface typo suggestions (TS2551).
    ///
    /// ## Phase 1 step-2: `StableLocation`-based declaration lookup
    ///
    /// This consumer reads [`tsz_binder::Symbol::stable_value_declaration`]
    /// (with a fallback to the first entry of
    /// [`tsz_binder::Symbol::stable_declarations`]) instead of
    /// `symbol.primary_declaration()`. The resulting `StableLocation`
    /// carries `(file_idx, pos, end)` and is rehydrated to a concrete
    /// `NodeIndex` on demand via
    /// [`CheckerContext::node_at_stable_location`][nasl]. This is the
    /// first consumer migrated under the
    /// [global query graph plan][plan] (Phase 1 step 2, following
    /// PR #1055). Heritage-clause tree walking is still arena-bound and
    /// fundamentally requires a live `NodeIndex`, so the helper returns
    /// one. The load-bearing change is that declaration *identity* no
    /// longer comes from the symbol's arena-dependent `NodeIndex`.
    ///
    /// [nasl]: crate::context::CheckerContext::node_at_stable_location
    /// [plan]: ../../../../docs/plan/ROADMAP.md
    pub(super) fn class_extends_any_base(&mut self, type_id: TypeId) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_scanner::SyntaxKind;

        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id).or_else(|| {
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, type_id)
        });
        let Some(sym_id) = sym_id else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::CLASS) {
            return false;
        }

        // Phase 1 step-2: identify the primary class declaration via its
        // `StableLocation`, not via `symbol.primary_declaration()`. Prefer
        // `stable_value_declaration` when set; otherwise fall back to the
        // first `stable_declarations` entry — mirroring the existing
        // `primary_declaration()` preference order. The parallel
        // `stable_*` fields are populated in lockstep by the binder, so
        // this is equivalent whenever the legacy `NodeIndex` fields are
        // populated.
        let stable_loc = if symbol.stable_value_declaration.is_known() {
            symbol.stable_value_declaration
        } else {
            match symbol.stable_declarations.first() {
                Some(loc) if loc.is_known() => *loc,
                _ => return false,
            }
        };

        // Resolve the `StableLocation` to a live `(NodeIndex, arena)` pair
        // and collect the candidate `extends` expression node indices. We
        // eagerly collect `expr_idx` values into a small vector so that the
        // arena borrow is released before any `&mut self` calls below
        // (`get_type_of_node`). A future phase can push this rehydration
        // further down or replace it entirely with a query-side class
        // summary.
        let extends_expr_indices: smallvec::SmallVec<[NodeIndex; 2]> = {
            let Some((decl_idx, arena)) = self.ctx.node_at_stable_location(stable_loc) else {
                return false;
            };
            let Some(class_decl) = arena.get_class_at(decl_idx) else {
                return false;
            };
            let Some(heritage_clauses) = &class_decl.heritage_clauses else {
                return false;
            };

            let mut out = smallvec::SmallVec::new();
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause) = arena.get_heritage_clause_at(clause_idx) else {
                    continue;
                };
                if clause.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let Some(&type_idx) = clause.types.nodes.first() else {
                    continue;
                };
                let expr_idx = if let Some(expr_type_args) = arena.get_expr_type_args_at(type_idx) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
                out.push(expr_idx);
            }
            out
        };

        // NOTE: `get_type_of_node` still operates against
        // `self.ctx.arena` (the current file's arena). Cross-file
        // class-extends-any detection was a pre-existing latent
        // limitation and is out of scope for this Phase 1 step-2
        // migration. The `StableLocation` rehydration above returns
        // the same arena whenever the class is in the current file —
        // which is the case for every caller today.
        for expr_idx in extends_expr_indices {
            if self.get_type_of_node(expr_idx) == TypeId::ANY {
                return true;
            }
        }

        false
    }

    fn collect_accessible_type_property_names(
        &mut self,
        original_type_id: TypeId,
        type_id: TypeId,
    ) -> Vec<String> {
        // For enum types, the solver can't access binder exports.
        // Collect enum member names directly from the binder's symbol exports.
        if let Some(def_id) = crate::query_boundaries::common::enum_def_id(self.ctx.types, type_id)
            && let Some(sym_id) = self.ctx.def_to_symbol_id(def_id)
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
            && symbol.has_any_flags(tsz_binder::symbol_flags::MODULE)
            && !symbol.has_any_flags(tsz_binder::symbol_flags::ENUM)
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

        let mut property_names =
            crate::query_boundaries::diagnostics::collect_accessible_property_names_for_suggestion(
                self.ctx.types,
                resolved_type,
                5,
            )
            .into_iter()
            .map(|name| self.ctx.types.resolve_atom_ref(name).to_string())
            .collect::<Vec<_>>();

        for name in self.generic_mapped_receiver_explicit_property_names(original_type_id) {
            if !property_names.iter().any(|existing| existing == &name) {
                property_names.push(name);
            }
        }

        // ECMAScript private fields (`#foo`) are never reachable through a public
        // member access, so tsc never offers one as a "did you mean?" candidate.
        // Without this, an external `c.events` on a class with a `#event` field
        // wrongly becomes TS2551 ("Did you mean '#event'?") instead of plain
        // TS2339 (witness: msw).
        property_names.retain(|name| !name.starts_with('#'));

        property_names
    }

    /// Map a primitive type to its boxed interface type for property name collection.
    /// Returns `None` if the type is not a primitive or no boxed type is registered.
    fn resolve_primitive_to_boxed_type(&self, type_id: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::common::IntrinsicKind;
        use crate::query_boundaries::common::TypeResolver;

        let kind = if crate::query_boundaries::common::is_string_type(self.ctx.types, type_id)
            || crate::query_boundaries::checkers::iterable::is_string_literal_type(
                self.ctx.types,
                type_id,
            ) {
            IntrinsicKind::String
        } else if crate::query_boundaries::common::is_number_type(self.ctx.types, type_id)
            || crate::query_boundaries::common::is_number_literal(self.ctx.types, type_id)
        {
            IntrinsicKind::Number
        } else if crate::query_boundaries::common::is_boolean_type(self.ctx.types, type_id) {
            IntrinsicKind::Boolean
        } else if crate::query_boundaries::common::is_bigint_type(self.ctx.types, type_id) {
            IntrinsicKind::Bigint
        } else if crate::query_boundaries::common::is_symbol_type(self.ctx.types, type_id) {
            IntrinsicKind::Symbol
        } else {
            return None;
        };

        TypeResolver::get_boxed_type(self.ctx.types, kind)
    }

    /// Resolve the binder SymbolId for a namespace/module type.
    /// Uses `classify_namespace_member` to find the symbol backing the type.
    fn resolve_namespace_symbol_for_type(&self, type_id: TypeId) -> Option<tsz_binder::SymbolId> {
        use crate::query_boundaries::common::{NamespaceMemberKind, classify_namespace_member};

        match classify_namespace_member(self.ctx.types, type_id) {
            NamespaceMemberKind::Lazy(def_id) => self.ctx.def_to_symbol_id(def_id),
            NamespaceMemberKind::TypeQuery(sym_ref) => {
                Some(crate::query_boundaries::definition_identity::symbol_ref_to_symbol_id(sym_ref))
            }
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

    /// Canonical `tsc` `getSpellingSuggestion` thresholds: the single owner of
    /// the `maximumLengthDifference`/`bestDistance` init that every
    /// spelling-suggestion path shares. Returns
    /// `(maximum_length_difference, initial_best_distance)`.
    pub(crate) const fn spelling_thresholds(name_len: usize) -> (usize, f64) {
        // tsc: maximumLengthDifference = max(2, floor(name.length * 0.34))
        let maximum_length_difference = if name_len * 34 / 100 > 2 {
            name_len * 34 / 100
        } else {
            2
        };
        // tsc: initial bestDistance = floor(name.length * 0.4) + 1
        let best_distance = (name_len * 4 / 10 + 1) as f64;
        (maximum_length_difference, best_distance)
    }

    /// Canonical `tsc` `getSpellingSuggestion` candidate scan: owns the
    /// threshold init and the per-candidate loop, delegating each candidate to
    /// `consider_identifier_suggestion` (which uses the single weighted
    /// `levenshtein_with_max` scorer). All single-source spelling-suggestion
    /// call sites differ only in the candidate source they pass; this is the
    /// one owner of the algorithm.
    pub(crate) fn best_spelling_suggestion<'b>(
        name: &str,
        candidates: impl IntoIterator<Item = &'b str>,
    ) -> Option<String> {
        let name_len = name.len();
        let (maximum_length_difference, mut best_distance) = Self::spelling_thresholds(name_len);
        let mut best_candidate: Option<String> = None;

        for candidate in candidates {
            Self::consider_identifier_suggestion(
                name,
                candidate,
                name_len,
                maximum_length_difference,
                &mut best_distance,
                &mut best_candidate,
            );
        }

        best_candidate
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
        let scope_id = self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, idx)
            .unwrap_or(tsz_binder::ScopeId::NONE);
        let candidate_key = (scope_id, meaning_flags);
        let visible_names = if let Some(cached) = self
            .ctx
            .name_resolution_diagnostics
            .spelling_candidate_cache
            .borrow()
            .get(&candidate_key)
        {
            std::rc::Rc::clone(cached)
        } else {
            let candidates: std::rc::Rc<[String]> = self
                .ctx
                .binder
                .collect_visible_symbol_names_filtered(self.ctx.arena, idx, meaning_flags)
                .into();
            self.ctx
                .name_resolution_diagnostics
                .spelling_candidate_cache
                .borrow_mut()
                .insert(candidate_key, std::rc::Rc::clone(&candidates));
            candidates
        };

        let name_len = name.len();
        let (maximum_length_difference, mut best_distance) = Self::spelling_thresholds(name_len);
        let mut best_candidate: Option<String> = None;

        for candidate in visible_names.iter() {
            Self::consider_identifier_suggestion(
                name,
                candidate,
                name_len,
                maximum_length_difference,
                &mut best_distance,
                &mut best_candidate,
            );
        }

        // Production binders merge every loaded lib into the visible-symbol table,
        // so rescanning each `LibContext` would duplicate the largest candidate
        // universe for every unresolved name. Direct checker/test contexts can
        // attach lib contexts without merging them; retain the explicit fallback
        // only for that shape. TYPE-only fallback stays scoped to core lib files
        // because an unmerged DOM context historically did not participate here.
        // Built-in keywords are still searched when this slice is empty.
        let lib_globals_filtered: Vec<crate::context::LibContext>;
        let lib_globals_for_search: &[crate::context::LibContext] =
            if self.ctx.binder.lib_symbols_are_merged() {
                &[]
            } else if meaning_flags == tsz_binder::symbol_flags::TYPE {
                lib_globals_filtered = self
                    .ctx
                    .lib_contexts
                    .iter()
                    .filter(|lib_ctx| Self::lib_context_is_core_typings(lib_ctx))
                    .cloned()
                    .collect();
                &lib_globals_filtered
            } else {
                &self.ctx.lib_contexts
            };

        Self::search_global_candidates(
            name,
            name_len,
            maximum_length_difference,
            meaning_flags,
            lib_globals_for_search,
            &mut best_distance,
            &mut best_candidate,
        );
        if is_builtin_lib_file_name(&self.ctx.file_name)
            && (meaning_flags == 0 || meaning_flags & tsz_binder::symbol_flags::TYPE != 0)
        {
            Self::search_symbol_arena_candidates(
                name,
                name_len,
                maximum_length_difference,
                meaning_flags,
                self.ctx.binder.symbols.iter(),
                &mut best_distance,
                &mut best_candidate,
            );
            for lib_ctx in self.ctx.lib_contexts.iter() {
                Self::search_symbol_arena_candidates(
                    name,
                    name_len,
                    maximum_length_difference,
                    meaning_flags,
                    lib_ctx.binder.symbols.iter(),
                    &mut best_distance,
                    &mut best_candidate,
                );
            }
        }

        best_candidate.map(|c| vec![c])
    }

    /// Whether a `LibContext` represents one of the core TypeScript lib files
    /// (`lib.es*.d.ts`, `lib.scripthost.d.ts`, `lib.decorators*.d.ts`). Used to
    /// bound the explicit TYPE-only fallback scan for checker contexts whose lib
    /// symbols have not been merged into the file binder.
    fn lib_context_is_core_typings(lib_ctx: &crate::context::LibContext) -> bool {
        let Some(file_name) = lib_ctx
            .arena
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str())
        else {
            return false;
        };
        Self::lib_file_name_is_core_typings(file_name)
    }

    /// Check whether a lib file name corresponds to a core TS lib —
    /// i.e. one whose globals (Array, Promise, Map, ...) tsc surfaces in
    /// spelling suggestions for type positions in user code.
    fn lib_file_name_is_core_typings(file_name: &str) -> bool {
        // Strip any directory prefix.
        let base = file_name
            .rsplit('/')
            .next()
            .and_then(|n| n.rsplit('\\').next())
            .unwrap_or(file_name);
        base.starts_with("lib.es")
            || base.starts_with("lib.scripthost")
            || base.starts_with("lib.decorators")
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

        // Also include the built-in intrinsic keywords that tsc registers as
        // symbols in its globals map.
        //
        // The `VALUE` and `TYPE` flag masks OVERLAP (both include
        // `CLASS`/`ENUM`/`ENUM_MEMBER`), so testing `meaning_flags & TYPE`
        // (or `& VALUE`) would misclassify a pure value-position or
        // type-position lookup — e.g. a value lookup passes `meaning == VALUE`,
        // yet `VALUE & TYPE != 0`, which used to leak the *type* keywords
        // (`number`, `boolean`, …) into value position so `numbr` wrongly
        // suggested `number` instead of the value `Number`. Gate on the
        // *exclusive* bits of each meaning so type keywords are offered only to
        // type/unconstrained lookups and value keywords only to
        // value/unconstrained lookups.
        const TYPE_ONLY: u32 = tsz_binder::symbol_flags::TYPE & !tsz_binder::symbol_flags::VALUE;
        const VALUE_ONLY: u32 = tsz_binder::symbol_flags::VALUE & !tsz_binder::symbol_flags::TYPE;
        for (gate, keywords) in [
            (TYPE_ONLY, BUILTIN_TYPE_KEYWORDS),
            (VALUE_ONLY, BUILTIN_VALUE_KEYWORDS),
        ] {
            if meaning_flags == 0 || meaning_flags & gate != 0 {
                for keyword in keywords {
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
    }

    fn search_symbol_arena_candidates<'b>(
        name: &str,
        name_len: usize,
        maximum_length_difference: usize,
        meaning_flags: u32,
        symbols: impl Iterator<Item = &'b tsz_binder::Symbol>,
        best_distance: &mut f64,
        best_candidate: &mut Option<String>,
    ) {
        for symbol in symbols {
            if symbol.escaped_name.is_empty() {
                continue;
            }
            if meaning_flags != 0 && symbol.flags & meaning_flags == 0 {
                continue;
            }
            Self::consider_identifier_suggestion(
                name,
                &symbol.escaped_name,
                name_len,
                maximum_length_difference,
                best_distance,
                best_candidate,
            );
        }
    }

    /// Find a spelling suggestion for a JSDoc type name without an AST node.
    ///
    /// Used by `emit_jsdoc_cannot_find_name` where positions are computed from
    /// comment text offsets and no `NodeIndex` is available. Searches file-level
    /// scope symbols, lib globals, and built-in type keywords.
    pub(crate) fn find_jsdoc_type_spelling_suggestion(&self, name: &str) -> Option<String> {
        let name_len = name.len();
        let (maximum_length_difference, mut best_distance) = Self::spelling_thresholds(name_len);
        let mut best_candidate: Option<String> = None;

        // Search file-level symbols (the binder's file_locals).
        for (symbol_name, sym_id) in self.ctx.binder.file_locals.iter() {
            if let Some(sym) = self.ctx.binder.get_symbol(*sym_id)
                && !sym.has_any_flags(tsz_binder::symbol_flags::TYPE)
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
        let primitive_type_name = match type_id {
            TypeId::STRING => Some("String"),
            TypeId::NUMBER => Some("Number"),
            TypeId::BOOLEAN => Some("Boolean"),
            TypeId::SYMBOL => Some("Symbol"),
            TypeId::BIGINT => Some("Bigint"),
            _ => None,
        };
        if let Some(type_name) = primitive_type_name
            && let Some(lib) = get_lib_for_type_property(type_name, prop_name)
        {
            return Some(lib);
        }

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
        // Map a well-known global value identifier to the name of its
        // constructor/static type, the key under which its static members live
        // in `getScriptTargetFeatures` (e.g. `Object.fromEntries` lives under
        // `ObjectConstructor`, `Symbol.dispose` under `SymbolConstructor`).
        let constructor_name = match ident.escaped_text.as_str() {
            "Object" => "ObjectConstructor",
            "Symbol" => "SymbolConstructor",
            "Map" => "MapConstructor",
            "Atomics" => "Atomics",
            "Array" => "ArrayConstructor",
            "Math" => "Math",
            "Number" => "NumberConstructor",
            "String" => "StringConstructor",
            "Promise" => "PromiseConstructor",
            "Reflect" => "Reflect",
            "Error" => "ErrorConstructor",
            "RegExp" => "RegExpConstructor",
            "Uint8Array" => "Uint8ArrayConstructor",
            _ => return None,
        };
        let lib = get_lib_for_type_property(constructor_name, prop_name)?;
        Some((lib, Some(constructor_name)))
    }

    /// Try to resolve the symbol name for a type.
    fn get_type_symbol_name(&mut self, type_id: TypeId) -> Option<String> {
        if crate::query_boundaries::common::is_array_type(self.ctx.types, type_id) {
            return Some("Array".to_string());
        }
        if type_id == TypeId::STRING
            || crate::query_boundaries::common::is_string_literal(self.ctx.types, type_id)
        {
            return Some("String".to_string());
        }
        if type_id == TypeId::NUMBER
            || crate::query_boundaries::common::is_number_literal(self.ctx.types, type_id)
        {
            return Some("Number".to_string());
        }
        if let Some(base) =
            crate::query_boundaries::common::get_application_base(self.ctx.types, type_id)
        {
            return self.get_type_symbol_name(base);
        }

        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id).or_else(|| {
            crate::query_boundaries::common::type_shape_symbol(self.ctx.types, type_id)
        });
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

    /// Allocation-free common path for ASCII identifiers. TypeScript source
    /// identifiers and bundled-lib names are overwhelmingly ASCII, so building
    /// four heap vectors for every candidate dominated large diagnostic sets.
    /// The scalar fallback below retains exact Unicode behavior.
    fn levenshtein_ascii_with_max(s1: &[u8], s2: &[u8], max: f64) -> Option<f64> {
        if s1.len().abs_diff(s2.len()) as f64 > max {
            return None;
        }

        if s1.is_empty() {
            let dist = s2.len() as f64;
            return (dist <= max).then_some(dist);
        }
        if s2.is_empty() {
            let dist = s1.len() as f64;
            return (dist <= max).then_some(dist);
        }

        let mut previous = smallvec::SmallVec::<[f64; 64]>::new();
        previous.extend((0..=s2.len()).map(|index| index as f64));
        let mut current = smallvec::SmallVec::<[f64; 64]>::new();
        current.resize(s2.len() + 1, 0.0);
        let big = max + 0.01;

        for (row, &c1) in s1.iter().enumerate() {
            let i = row + 1;
            let min_j = if (i as f64) > max {
                ((i as f64) - max).ceil() as usize
            } else {
                1
            };
            let max_j = if (s2.len() as f64) > (max + i as f64) {
                (max + i as f64).floor() as usize
            } else {
                s2.len()
            };

            current[0] = i as f64;
            let mut col_min = i as f64;

            for value in current.iter_mut().take(min_j).skip(1) {
                *value = big;
            }

            for j in min_j..=max_j {
                let c2 = s2[j - 1];
                let substitution_distance = if c1.eq_ignore_ascii_case(&c2) {
                    previous[j - 1] + 0.1
                } else {
                    previous[j - 1] + 2.0
                };
                let dist = if c1 == c2 {
                    previous[j - 1]
                } else {
                    (previous[j] + 1.0)
                        .min(current[j - 1] + 1.0)
                        .min(substitution_distance)
                };
                current[j] = dist;
                col_min = col_min.min(dist);
            }

            for value in current.iter_mut().take(s2.len() + 1).skip(max_j + 1) {
                *value = big;
            }

            if col_min > max {
                return None;
            }

            std::mem::swap(&mut previous, &mut current);
        }

        let result = previous[s2.len()];
        (result <= max).then_some(result)
    }

    /// Levenshtein distance with threshold pruning, matching tsc's behavior.
    /// Case-only substitutions are cheaper than other substitutions.
    pub(crate) fn levenshtein_with_max(s1: &str, s2: &str, max: f64) -> Option<f64> {
        if s1.is_ascii() && s2.is_ascii() {
            return Self::levenshtein_ascii_with_max(s1.as_bytes(), s2.as_bytes(), max);
        }

        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();

        if s1_chars.len().abs_diff(s2_chars.len()) as f64 > max {
            return None;
        }

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

#[cfg(test)]
mod spelling_distance_tests {
    use super::CheckerState;

    fn assert_distance(left: &str, right: &str, max: f64, expected: Option<f64>) {
        let actual = CheckerState::levenshtein_with_max(left, right, max);
        match (actual, expected) {
            (Some(actual), Some(expected)) => assert!(
                (actual - expected).abs() < f64::EPSILON,
                "distance for {left:?} -> {right:?}: expected {expected}, got {actual}"
            ),
            (actual, expected) => assert_eq!(actual, expected),
        }
    }

    #[test]
    fn ascii_spelling_distance_preserves_weighting_and_thresholds() {
        assert_distance("ParentNode", "ParentNode", 0.0, Some(0.0));
        assert_distance("array", "Array", 1.0, Some(0.1));
        assert_distance("sting", "string", 1.0, Some(1.0));
        assert_distance("becon", "bacon", 2.0, Some(2.0));
        assert_distance("becon", "bacon", 1.9, None);
        assert_distance("", "Map", 3.0, Some(3.0));
    }

    #[test]
    fn unicode_spelling_distance_retains_scalar_case_folding() {
        assert_distance("École", "école", 1.0, Some(0.1));
        assert_distance("café", "cafe", 1.9, None);
        assert_distance("café", "cafe", 2.0, Some(2.0));
    }
}

/// Lib versions for the members every integer/float typed array shares
/// (`Int8Array` … `Float64Array`, `BigInt64Array`, `BigUint64Array`, and
/// `Uint8Array`): the es2022 `at` accessor and the es2023 copying helpers.
/// `Uint8Array` layers its own esnext base64/hex methods on top of these.
fn typed_array_shared_feature(prop_name: &str) -> Option<&'static str> {
    Some(match prop_name {
        "at" => "es2022",
        "findLastIndex" | "findLast" | "toReversed" | "toSorted" | "toSpliced" | "with" => "es2023",
        _ => return None,
    })
}

/// Look up the lib version that first introduced a (type, property) pair, used
/// to drive the TS2550 "change your target library" suggestion.
///
/// This mirrors the TypeScript compiler's `getScriptTargetFeatures` table
/// (`src/compiler/utilities.ts`): the key is the *apparent type's symbol name*
/// (e.g. `Array`, `ObjectConstructor`, `SymbolConstructor`) and the value is
/// the lib in which the property first appears. Because each property name
/// occurs under exactly one lib for a given type, a flat `(type, prop) -> lib`
/// lookup reproduces tsc's "first matching lib wins" iteration order.
///
/// Parity rules worth preserving when updating this table:
/// - Instance and constructor types are kept SEPARATE exactly as tsc does
///   (`Error` vs `ErrorConstructor`, `Promise` vs `PromiseConstructor`,
///   `Symbol` vs `SymbolConstructor`). Merging them would synthesize a false
///   TS2550 where tsc reports TS2339 (e.g. `(new Error()).isError`).
/// - No catch-all `_ => Some(lib)` arms: tsc lists each feature explicitly, so
///   an unlisted property must fall through to TS2339 / TS2551 like tsc.
/// - Types absent from tsc's table (e.g. `ReadonlyArray`, `WeakRef`) are absent
///   here; their missing members are TS2339 in tsc.
fn get_lib_for_type_property(type_name: &str, prop_name: &str) -> Option<&'static str> {
    let lib =
        match type_name {
            "Array" => {
                match prop_name {
                    "find" | "findIndex" | "fill" | "copyWithin" | "entries" | "keys"
                    | "values" => "es2015",
                    "includes" => "es2016",
                    "flat" | "flatMap" => "es2019",
                    "at" => "es2022",
                    "findLastIndex" | "findLast" | "toReversed" | "toSorted" | "toSpliced"
                    | "with" => "es2023",
                    _ => return None,
                }
            }
            "ArrayBuffer" => match prop_name {
                "maxByteLength"
                | "resizable"
                | "resize"
                | "detached"
                | "transfer"
                | "transferToFixedLength" => "es2024",
                _ => return None,
            },
            "Atomics" => match prop_name {
                "add" | "and" | "compareExchange" | "exchange" | "isLockFree" | "load" | "or"
                | "store" | "sub" | "wait" | "notify" | "xor" => "es2017",
                "waitAsync" => "es2024",
                "pause" => "esnext",
                _ => return None,
            },
            "SharedArrayBuffer" => match prop_name {
                "byteLength" | "slice" => "es2017",
                "growable" | "maxByteLength" | "grow" => "es2024",
                _ => return None,
            },
            "RegExp" => match prop_name {
                "flags" | "sticky" | "unicode" => "es2015",
                "dotAll" => "es2018",
                "unicodeSets" => "es2024",
                _ => return None,
            },
            "RegExpConstructor" => match prop_name {
                "escape" => "es2025",
                _ => return None,
            },
            "Reflect" => match prop_name {
                "apply"
                | "construct"
                | "defineProperty"
                | "deleteProperty"
                | "get"
                | "getOwnPropertyDescriptor"
                | "getPrototypeOf"
                | "has"
                | "isExtensible"
                | "ownKeys"
                | "preventExtensions"
                | "set"
                | "setPrototypeOf" => "es2015",
                _ => return None,
            },
            "ArrayConstructor" => match prop_name {
                "from" | "of" => "es2015",
                "fromAsync" => "esnext",
                _ => return None,
            },
            "ObjectConstructor" => match prop_name {
                "assign" | "getOwnPropertySymbols" | "keys" | "is" | "setPrototypeOf" => "es2015",
                "values" | "entries" | "getOwnPropertyDescriptors" => "es2017",
                "fromEntries" => "es2019",
                "hasOwn" => "es2022",
                "groupBy" => "es2024",
                _ => return None,
            },
            "NumberConstructor" => match prop_name {
                "isFinite" | "isInteger" | "isNaN" | "isSafeInteger" | "parseFloat"
                | "parseInt" => "es2015",
                _ => return None,
            },
            "Math" => match prop_name {
                "clz32" | "imul" | "sign" | "log10" | "log2" | "log1p" | "expm1" | "cosh"
                | "sinh" | "tanh" | "acosh" | "asinh" | "atanh" | "hypot" | "trunc" | "fround"
                | "cbrt" => "es2015",
                "f16round" => "es2025",
                _ => return None,
            },
            "Map" => match prop_name {
                "entries" | "keys" | "values" => "es2015",
                "getOrInsert" | "getOrInsertComputed" => "esnext",
                _ => return None,
            },
            "MapConstructor" => match prop_name {
                "groupBy" => "es2024",
                _ => return None,
            },
            "Set" => match prop_name {
                "entries" | "keys" | "values" => "es2015",
                "union"
                | "intersection"
                | "difference"
                | "symmetricDifference"
                | "isSubsetOf"
                | "isSupersetOf"
                | "isDisjointFrom" => "es2025",
                _ => return None,
            },
            "PromiseConstructor" => match prop_name {
                "all" | "race" | "reject" | "resolve" => "es2015",
                "allSettled" => "es2020",
                "any" => "es2021",
                "withResolvers" => "es2024",
                "try" => "es2025",
                _ => return None,
            },
            "Symbol" => match prop_name {
                "for" | "keyFor" => "es2015",
                "description" => "es2019",
                _ => return None,
            },
            "WeakMap" => match prop_name {
                "entries" | "keys" | "values" => "es2015",
                "getOrInsert" | "getOrInsertComputed" => "esnext",
                _ => return None,
            },
            "WeakSet" => match prop_name {
                "entries" | "keys" | "values" => "es2015",
                _ => return None,
            },
            "String" => match prop_name {
                "codePointAt" | "includes" | "endsWith" | "normalize" | "repeat" | "startsWith"
                | "anchor" | "big" | "blink" | "bold" | "fixed" | "fontcolor" | "fontsize"
                | "italics" | "link" | "small" | "strike" | "sub" | "sup" => "es2015",
                "padStart" | "padEnd" => "es2017",
                "trimStart" | "trimEnd" | "trimLeft" | "trimRight" => "es2019",
                "matchAll" => "es2020",
                "replaceAll" => "es2021",
                "at" => "es2022",
                "isWellFormed" | "toWellFormed" => "es2024",
                _ => return None,
            },
            "StringConstructor" => match prop_name {
                "fromCodePoint" | "raw" => "es2015",
                _ => return None,
            },
            "DateTimeFormat" => match prop_name {
                "formatToParts" => "es2017",
                _ => return None,
            },
            "Promise" => match prop_name {
                "finally" => "es2018",
                _ => return None,
            },
            "RegExpMatchArray" | "RegExpExecArray" => match prop_name {
                "groups" => "es2018",
                _ => return None,
            },
            "Intl" => match prop_name {
                "PluralRules" => "es2018",
                "RelativeTimeFormat" | "Locale" | "DisplayNames" => "es2020",
                "ListFormat" | "DateTimeFormat" => "es2021",
                "Segmenter" => "es2022",
                "DurationFormat" => "es2025",
                _ => return None,
            },
            "NumberFormat" => match prop_name {
                "formatToParts" => "es2018",
                _ => return None,
            },
            "SymbolConstructor" => match prop_name {
                "matchAll" => "es2020",
                "metadata" | "dispose" | "asyncDispose" => "esnext",
                _ => return None,
            },
            "DataView" => match prop_name {
                "setBigInt64" | "setBigUint64" | "getBigInt64" | "getBigUint64" => "es2020",
                "setFloat16" | "getFloat16" => "es2025",
                _ => return None,
            },
            "RelativeTimeFormat" => match prop_name {
                "format" | "formatToParts" | "resolvedOptions" => "es2020",
                _ => return None,
            },
            "Int8Array" | "Uint8ClampedArray" | "Int16Array" | "Uint16Array" | "Int32Array"
            | "Uint32Array" | "Float32Array" | "Float64Array" | "BigInt64Array"
            | "BigUint64Array" => return typed_array_shared_feature(prop_name),
            "Uint8Array" => match prop_name {
                "toBase64" | "setFromBase64" | "toHex" | "setFromHex" => "esnext",
                _ => return typed_array_shared_feature(prop_name),
            },
            "Uint8ArrayConstructor" => match prop_name {
                "fromBase64" | "fromHex" => "esnext",
                _ => return None,
            },
            "Error" => match prop_name {
                "cause" => "es2022",
                _ => return None,
            },
            "ErrorConstructor" => match prop_name {
                "isError" => "esnext",
                _ => return None,
            },
            "Date" => match prop_name {
                "toTemporalInstant" => "esnext",
                _ => return None,
            },
            _ => return None,
        };
    Some(lib)
}

// =============================================================================
// Phase 1 step-2 regression tests: `StableLocation` rehydration
// =============================================================================
//
// These tests validate the migration of `class_extends_any_base` away from
// the arena-dependent `Symbol::primary_declaration(): NodeIndex` toward the
// file-stable `Symbol::stable_value_declaration` / `stable_declarations`
// fields introduced by PR #1055. The critical invariant they lock in is
// that a `StableLocation` captured from one binder/arena pair can be
// resolved against a freshly re-parsed arena of the same source — the
// Phase 5 "bounded arena residency" precondition.

#[cfg(test)]
mod tests {
    use crate::context::{CheckerContext, CheckerOptions};
    use tsz_binder::BinderState;
    use tsz_parser::ParserState;
    use tsz_solver::construction::TypeInterner;

    /// Resolving `Symbol::stable_value_declaration` for a class via the new
    /// `node_at_stable_location` helper must return the same class node
    /// that `Symbol::value_declaration` points at in the same binder.
    #[test]
    fn stable_value_declaration_resolves_to_class_node() {
        let source = "class Foo extends Bar {}\n".to_string();

        let mut parser = ParserState::new("syn.ts".to_string(), source);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let sym_id = binder.file_locals.get("Foo").expect("class symbol Foo");
        let symbol = binder.symbols.get(sym_id).expect("symbol data");
        let stable = symbol.stable_value_declaration;
        assert!(
            stable.is_known(),
            "class Foo must have a known stable_value_declaration span"
        );
        let legacy_node_idx = symbol.value_declaration;
        assert!(
            legacy_node_idx.is_some(),
            "class Foo must have a populated value_declaration (NodeIndex)"
        );

        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena,
            &binder,
            &types,
            "syn.ts".to_string(),
            CheckerOptions::default(),
        );

        let (resolved_idx, resolved_arena) = ctx
            .node_at_stable_location(stable)
            .expect("node_at_stable_location must resolve the class span");

        assert_eq!(
            resolved_idx, legacy_node_idx,
            "StableLocation must rehydrate to the same NodeIndex as value_declaration"
        );
        let resolved_node = resolved_arena
            .get(resolved_idx)
            .expect("resolved NodeIndex must exist in arena");
        assert_eq!(resolved_node.pos, stable.pos);
        assert_eq!(resolved_node.end, stable.end);
    }

    /// The load-bearing Phase 5 scenario: capture a `StableLocation` from
    /// one arena, drop it (simulated by a fresh parser), and re-resolve
    /// the same `(pos, end)` against a newly parsed arena. The
    /// rehydrated `NodeIndex` must point at a node with matching span.
    ///
    /// This proves `node_at_stable_location` does NOT depend on arena
    /// identity — only on the `(file_idx, pos, end)` triple.
    #[test]
    fn stable_location_round_trips_across_arena_reparse() {
        let source = "class Foo extends Bar {}\nclass Qux {}\n".to_string();

        // Capture a StableLocation for `Foo` from the first binder, then
        // let the first arena/binder go out of scope.
        let captured = {
            let mut parser = ParserState::new("syn.ts".to_string(), source.clone());
            let root = parser.parse_source_file();
            let arena = parser.get_arena();
            let mut binder = BinderState::new();
            binder.bind_source_file(arena, root);
            let sym_id = binder.file_locals.get("Foo").expect("class symbol Foo");
            let symbol = binder.symbols.get(sym_id).expect("symbol data");
            symbol.stable_value_declaration
        };
        assert!(
            captured.is_known(),
            "captured StableLocation must carry a real (pos, end) span"
        );

        // Fresh parse + bind of the identical source. The captured
        // StableLocation must resolve in this new arena.
        let mut parser = ParserState::new("syn.ts".to_string(), source);
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            arena,
            &binder,
            &types,
            "syn.ts".to_string(),
            CheckerOptions::default(),
        );

        let (resolved_idx, resolved_arena) = ctx
            .node_at_stable_location(captured)
            .expect("captured StableLocation must rehydrate against a freshly parsed arena");
        let node = resolved_arena
            .get(resolved_idx)
            .expect("resolved NodeIndex must exist in the new arena");
        assert_eq!(node.pos, captured.pos);
        assert_eq!(node.end, captured.end);

        // The new binder's `value_declaration` NodeIndex should agree
        // with the helper's resolution — binder population is
        // deterministic for identical source text.
        let sym_id = binder
            .file_locals
            .get("Foo")
            .expect("class symbol Foo in reparsed binder");
        let new_symbol = binder
            .symbols
            .get(sym_id)
            .expect("symbol data in reparsed binder");
        assert_eq!(
            resolved_idx, new_symbol.value_declaration,
            "re-resolution must agree with the re-parsed binder's NodeIndex"
        );
    }

    /// `node_at_stable_location` must return `None` for the sentinel
    /// `StableLocation::NONE` (unknown span) so consumers can treat it as
    /// a clean "no declaration" signal.
    #[test]
    fn stable_location_none_resolves_to_none() {
        let arena = tsz_parser::parser::node::NodeArena::new();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let ctx = CheckerContext::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        let none = tsz_binder::symbols::StableLocation::NONE;
        assert!(ctx.node_at_stable_location(none).is_none());
    }
}
