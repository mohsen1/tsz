//! Type-parameter extraction, resolution, and required-count helpers for
//! `CheckerState`. Extracted from `type_environment/core.rs` to keep that
//! module under the 2000-LOC checker boundary; behavior is unchanged.

use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Get type parameters for a symbol (generic types).
    ///
    /// Extracts type parameter information for generic types (classes, interfaces,
    /// type aliases). Used for populating the type environment and for generic
    /// type instantiation.
    ///
    /// ## Symbol Types Handled:
    /// - **Type Alias**: Extracts type parameters from type alias declaration
    /// - **Interface**: Extracts type parameters from interface declaration
    /// - **Class**: Extracts type parameters from class declaration
    /// - **Other**: Returns empty vector (no type parameters)
    ///
    /// ## Cross-Arena Resolution:
    /// - Handles symbols defined in other arenas (e.g., imported symbols)
    /// - Creates a temporary `CheckerState` for the other arena
    /// - Delegates type parameter extraction to the temporary checker
    ///
    /// ## Type Parameter Information:
    /// - Returns Vec<TypeParamInfo> with parameter names and constraints
    /// - Includes default type arguments if present
    /// - Used by `TypeEnvironment` for generic type expansion
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // Type alias with type parameters
    /// type Pair<T, U> = [T, U];
    /// // get_type_params_for_symbol(Pair) → [T, U]
    ///
    /// // Interface with type parameters
    /// interface Box<T> {
    ///   value: T;
    /// }
    /// // get_type_params_for_symbol(Box) → [T]
    ///
    /// // Class with type parameters
    /// class Container<T> {
    ///   constructor(public item: T) {}
    /// }
    /// // get_type_params_for_symbol(Container) → [T]
    ///
    /// // Type parameters with constraints
    /// interface SortedMap<K extends Comparable, V> {}
    /// // get_type_params_for_symbol(SortedMap) → [K: Comparable, V]
    /// ```
    fn extract_type_params_from_decl(
        checker: &mut CheckerState,
        flags: u32,
        decl_idx: NodeIndex,
        sym_escaped_name: &str,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let mixed_class_interface =
            (flags & symbol_flags::CLASS) != 0 && (flags & symbol_flags::INTERFACE) != 0;
        if let Some(node) = checker.ctx.arena.get(decl_idx) {
            if flags & symbol_flags::TYPE_ALIAS != 0
                && let Some(type_alias) = checker.ctx.arena.get_type_alias(node)
            {
                // Guard against a `decl_idx` from another file's arena landing on
                // an unrelated type alias at the same numeric index (issue #13599).
                if !Self::decl_name_matches_in_arena(
                    checker.ctx.arena,
                    type_alias.name,
                    sym_escaped_name,
                ) {
                    return None;
                }
                let (params, updates) = checker.push_type_parameters(&type_alias.type_parameters);
                checker.pop_type_parameters(updates);
                return Some(params);
            }
            if !mixed_class_interface
                && flags & symbol_flags::CLASS != 0
                && let Some(class) = checker.ctx.arena.get_class(node)
            {
                if !Self::decl_name_matches_in_arena(
                    checker.ctx.arena,
                    class.name,
                    sym_escaped_name,
                ) {
                    return None;
                }
                let (params, updates) = checker.push_type_parameters(&class.type_parameters);
                checker.pop_type_parameters(updates);
                if !params.is_empty() {
                    return Some(params);
                }

                if let Some(class_jsdoc_params) =
                    Self::jsdoc_template_type_params_for_decl(checker, decl_idx, sym_escaped_name)
                {
                    return Some(class_jsdoc_params);
                }

                return Some(Vec::new());
            }
            if flags & symbol_flags::INTERFACE != 0
                && let Some(iface) = checker.ctx.arena.get_interface(node)
            {
                if !Self::decl_name_matches_in_arena(
                    checker.ctx.arena,
                    iface.name,
                    sym_escaped_name,
                ) {
                    return None;
                }
                let (params, updates) = checker.push_type_parameters(&iface.type_parameters);
                checker.pop_type_parameters(updates);
                return Some(params);
            }
        }
        None
    }

    fn jsdoc_template_params_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        use tsz_common::comments::{
            get_jsdoc_content, get_leading_comments_from_cache, is_jsdoc_comment,
        };

        let sf = arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let node = arena.get(decl_idx)?;
        let mut search_pos = node.pos;
        // EXPORT_DECLARATION wraps `export class Foo {}` so the leading JSDoc
        // attaches before the `export` keyword; walk up to find the real anchor.
        if let Some(ext) = arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent) = arena.get(ext.parent)
            && parent.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION
        {
            search_pos = parent.pos;
        }

        let leading = get_leading_comments_from_cache(comments, search_pos, source_text);
        let mut jsdoc: Option<String> = None;
        for comment in leading.iter().rev() {
            let end = comment.end as usize;
            let check = search_pos as usize;
            if end <= check
                && source_text
                    .get(end..check)
                    .is_some_and(|gap| gap.chars().all(char::is_whitespace))
                && is_jsdoc_comment(comment, source_text)
            {
                jsdoc = Some(get_jsdoc_content(comment, source_text));
                break;
            }
        }
        let jsdoc = jsdoc?;

        let names = Self::jsdoc_template_type_params(&jsdoc);
        if names.is_empty() {
            return None;
        }

        let mut params = Vec::with_capacity(names.len());
        let constraint_strs = Self::jsdoc_template_constraint_strings(&jsdoc);
        for (name, is_const, default_str) in names {
            if name.is_empty() {
                continue;
            }
            let default = default_str
                .as_deref()
                .and_then(crate::types_domain::queries::lib_resolution::keyword_name_to_type_id);
            // This `&self` cross-file extraction path cannot run full JSDoc
            // type resolution, so (like `default` above) it resolves only
            // keyword constraints (`{string}`, `{number}`, ...). Non-keyword
            // constraints fall back to `None` and are enforced on the
            // full-resolution same-file path instead.
            let constraint = constraint_strs
                .get(&name)
                .map(String::as_str)
                .and_then(crate::types_domain::queries::lib_resolution::keyword_name_to_type_id);
            params.push(tsz_solver::TypeParamInfo {
                name: self.ctx.types.intern_string(&name),
                constraint,
                default,
                is_const,
                origin: tsz_solver::TypeParamOrigin::User,
            });
        }

        if params.is_empty() {
            None
        } else {
            Some(params)
        }
    }

    pub(crate) fn extract_simple_type_params_from_decl_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        flags: u32,
        decl_idx: NodeIndex,
        sym_escaped_name: &str,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let node = arena.get(decl_idx)?;
        let mixed_class_interface =
            (flags & symbol_flags::CLASS) != 0 && (flags & symbol_flags::INTERFACE) != 0;

        let type_parameters = if flags & symbol_flags::TYPE_ALIAS != 0 {
            let type_alias = arena.get_type_alias(node)?;
            // Guard against a `decl_idx` from another file's arena landing on an
            // unrelated type alias at the same numeric index (issue #13599).
            if !Self::decl_name_matches_in_arena(arena, type_alias.name, sym_escaped_name) {
                return None;
            }
            let Some(type_parameters) = type_alias.type_parameters.as_ref() else {
                return Some(Vec::new());
            };
            type_parameters
        } else if !mixed_class_interface && flags & symbol_flags::CLASS != 0 {
            let class = arena.get_class(node)?;
            if !Self::decl_name_matches_in_arena(arena, class.name, sym_escaped_name) {
                return None;
            }
            let Some(type_parameters) = class.type_parameters.as_ref() else {
                // Class with no AST type-parameters: the slow path's only work
                // is a JSDoc @template scan that already reads from the arena.
                // Reproduce it arena-directly so we don't construct a
                // `with_parent_cache_attributed` child checker just for this.
                return Some(
                    self.jsdoc_template_params_in_arena(arena, decl_idx)
                        .unwrap_or_default(),
                );
            };
            type_parameters
        } else if flags & symbol_flags::INTERFACE != 0 {
            let Some(iface) = arena.get_interface(node) else {
                // Merged symbols such as `Array` can present a value
                // declaration before the interface declaration. This candidate
                // cannot contribute type parameters, but returning `None`
                // would force a child checker before the later interface decl
                // gets a chance to provide the real params.
                return Some(Vec::new());
            };
            if !Self::decl_name_matches_in_arena(arena, iface.name, sym_escaped_name) {
                return None;
            }
            let Some(type_parameters) = iface.type_parameters.as_ref() else {
                // Interface with no AST type parameters also has an arena-only
                // result: the slow path returns an empty parameter list.
                return Some(Vec::new());
            };
            type_parameters
        } else {
            return None;
        };

        let mut has_constraint_or_default = false;
        let mut seen_names = FxHashSet::default();
        for &param_idx in &type_parameters.nodes {
            let node = arena.get(param_idx)?;
            let data = arena.get_type_parameter(node)?;
            has_constraint_or_default |=
                data.constraint != NodeIndex::NONE || data.default != NodeIndex::NONE;

            let name = arena
                .get(data.name)
                .and_then(|name_node| arena.get_identifier(name_node))
                .map(|id_data| id_data.escaped_text.clone())?;
            if !seen_names.insert(name.clone()) {
                return None;
            }
        }

        if has_constraint_or_default {
            let lowering = tsz_lowering::TypeLowering::new(arena, self.ctx.types)
                .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type());
            let params = lowering.collect_type_parameters(type_parameters);
            if params.len() != type_parameters.nodes.len() {
                return None;
            }
            if params.iter().any(|param| {
                param.constraint.is_some_and(|ty| {
                    crate::query_boundaries::common::is_error_type(self.ctx.types, ty)
                }) || param.default.is_some_and(|ty| {
                    crate::query_boundaries::common::is_error_type(self.ctx.types, ty)
                })
            }) {
                return None;
            }
            return Some(params);
        }

        let params = type_parameters
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let node = arena.get(param_idx)?;
                let data = arena.get_type_parameter(node)?;
                let name = arena
                    .get(data.name)
                    .and_then(|name_node| arena.get_identifier(name_node))
                    .map(|id_data| id_data.escaped_text.clone())?;
                Some(tsz_solver::TypeParamInfo {
                    name: self.ctx.types.intern_string(&name),
                    constraint: None,
                    default: None,
                    is_const: arena.has_modifier(&data.modifiers, SyntaxKind::ConstKeyword),
                    origin: tsz_solver::TypeParamOrigin::User,
                })
            })
            .collect();
        Some(params)
    }

    fn jsdoc_template_type_params_for_decl(
        checker: &mut CheckerState,
        decl_idx: NodeIndex,
        _sym_escaped_name: &str,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let sf = checker.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let mut search_pos = checker.ctx.arena.get(decl_idx)?.pos;
        // For `export class Foo {}` the parser wraps the class in an
        // EXPORT_DECLARATION node. The leading JSDoc attaches before the
        // `export` keyword, so the gap between the JSDoc end and `class.pos`
        // contains `"export "` and `try_leading_jsdoc` would return None.
        // Walk up to the wrapping export to locate the JSDoc correctly.
        if let Some(ext) = checker.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent) = checker.ctx.arena.get(ext.parent)
            && parent.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION
        {
            search_pos = parent.pos;
        }
        let jsdoc = checker.try_leading_jsdoc(comments, search_pos, source_text)?;

        let names = Self::jsdoc_template_type_params(&jsdoc);
        if names.is_empty() {
            return None;
        }

        let mut params = Vec::with_capacity(names.len());
        let constraint_strs = Self::jsdoc_template_constraint_strings(&jsdoc);
        for (name, is_const, default_str) in names {
            if name.is_empty() {
                continue;
            }
            let default = default_str
                .as_deref()
                .and_then(|s| checker.resolve_jsdoc_reference(s));
            let constraint = constraint_strs
                .get(&name)
                .and_then(|s| checker.resolve_jsdoc_reference(s));
            params.push(tsz_solver::TypeParamInfo {
                name: checker.ctx.types.intern_string(&name),
                constraint,
                default,
                is_const,
                origin: tsz_solver::TypeParamOrigin::User,
            });
        }

        if params.is_empty() {
            None
        } else {
            Some(params)
        }
    }

    pub(crate) fn get_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        // Recursion depth check: prevent stack overflow from circular generic defaults
        // (e.g. type A<T = B> = T; type B<T = A> = T;)
        if !self.ctx.enter_recursion() {
            return Vec::new();
        }

        let mut sym_id = sym_id;
        let use_dynamic_symbol_owner = match self.ctx.resolve_dynamic_symbol_file_index(sym_id) {
            None => true,
            Some(file_idx) => {
                let target_is_type_alias = self
                    .ctx
                    .get_binder_for_file(file_idx)
                    .and_then(|binder| binder.get_symbol(sym_id))
                    .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_ALIAS));
                !target_is_type_alias
                    || self.should_delegate_dynamic_type_alias_owner(sym_id, file_idx)
            }
        };
        // NOTE: a *local* import alias (`import { Generic } from './m'`) is not
        // followed to its target's params here — only a cross-file overlay alias
        // is. This is asymmetric with `get_type_of_symbol`, which does follow the
        // alias to the target body. Threading re-export-chain resolution into this
        // hot getter is not a safe refactor in isolation: the symbol-path fallback
        // below (the `!checked_local` block) deliberately keeps the
        // dynamic-overlay-first owner, because preferring the declaring index for a
        // re-exported alias reads params from the wrong arena (regression in
        // `cross_file_recursive_alias_intersection_tests`). The interface-heritage
        // site compensates for this asymmetry directly via the re-export-chain
        // recovery in `merge_interface_heritage_types_inner` (#13212).
        if use_dynamic_symbol_owner
            && let Some(symbol) = self.get_cross_file_symbol(sym_id)
            && symbol.has_any_flags(symbol_flags::ALIAS)
        {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited_aliases) {
                sym_id = target;
            }
        }

        // Prefer the registered cross-file target before falling back to global
        // symbol lookup, because raw SymbolId values can collide across binders.
        // Extract needed data to avoid holding a borrow during deeper operations.
        let local_symbol;
        let source_symbol = if use_dynamic_symbol_owner {
            self.get_cross_file_symbol(sym_id)
        } else {
            local_symbol = self.ctx.binder.get_symbol(sym_id);
            local_symbol
        };
        let (flags, value_decl, declarations, sym_escaped_name) = match source_symbol {
            Some(symbol) => (
                symbol.flags,
                symbol.value_declaration,
                symbol.declarations.clone(),
                symbol.escaped_name.clone(),
            ),
            None => {
                self.ctx.leave_recursion();
                return Vec::new();
            }
        };
        let def_id = if use_dynamic_symbol_owner {
            self.ctx.get_or_create_def_id(sym_id)
        } else {
            self.ctx
                .get_or_create_def_id_for_symbol_name(sym_id, &sym_escaped_name)
        };
        let prefers_type_only_decls =
            (flags & symbol_flags::CLASS) != 0 && (flags & symbol_flags::INTERFACE) != 0;

        // Use only the local def_type_params cache, NOT get_def_type_params which
        // falls through to the DefinitionStore. The DefinitionStore may contain
        // pre-populated placeholder params (from from_semantic_defs) that have
        // constraint: None even when the actual type parameter declarations have
        // constraints. The local cache is only populated after full AST-based
        // resolution via insert_def_type_params, so it always has correct constraints.
        //
        // Merged class+interface symbols are special: class-side resolution paths can
        // seed the cache with the class arity before the interface-side defaults are
        // merged. Recompute those through the merged declaration walk instead of
        // trusting a potentially stale cache entry.
        //
        // For non-lib symbols, unconstrained params with no defaults are valid final
        // AST results, not placeholders. Keep those cached to avoid recomputing every
        // user-defined generic access.
        let cached_params = (!prefers_type_only_decls)
            .then(|| self.ctx.def_type_params.borrow().get(&def_id).cloned())
            .flatten();
        if let Some(cached) = cached_params {
            let cached_is_placeholder = !cached.is_empty()
                && cached
                    .iter()
                    .all(|param| param.constraint.is_none() && param.default.is_none());
            if cached_is_placeholder && self.ctx.binder.lib_symbol_ids.contains(&sym_id) {
                self.prime_lib_type_params(&sym_escaped_name);
                if let Some(params) = self.ctx.def_type_params.borrow().get(&def_id).cloned()
                    && !params.is_empty()
                {
                    self.ctx.leave_recursion();
                    return params;
                }
            }
            if !cached_is_placeholder {
                self.ctx.leave_recursion();
                return cached;
            }
            self.ctx.def_type_params.borrow_mut().remove(&def_id);
        }
        if !prefers_type_only_decls && self.ctx.def_no_type_params.borrow().contains(&def_id) {
            self.ctx.leave_recursion();
            return Vec::new();
        }

        // Fast path: only class/interface/type alias symbols can declare type parameters.
        if flags & (symbol_flags::TYPE_ALIAS | symbol_flags::CLASS | symbol_flags::INTERFACE) == 0 {
            self.ctx.def_no_type_params.borrow_mut().insert(def_id);
            self.ctx.leave_recursion();
            return Vec::new();
        }

        let mut decl_candidates = Vec::new();
        if !prefers_type_only_decls && value_decl != tsz_parser::parser::NodeIndex::NONE {
            decl_candidates.push(value_decl);
        }
        for &decl in &declarations {
            if decl != value_decl {
                decl_candidates.push(decl);
            }
        }

        let mut merged_params: Option<Vec<tsz_solver::TypeParamInfo>> = None;
        let mut fallback_params = None;

        for decl_idx in decl_candidates {
            let mut checked_local = false;

            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                        checked_local = true;
                        if let Some(params) = Self::extract_type_params_from_decl(
                            self,
                            flags,
                            decl_idx,
                            &sym_escaped_name,
                        ) {
                            if !params.is_empty() {
                                if let Some(ref mut merged) = merged_params {
                                    for (i, p) in params.into_iter().enumerate() {
                                        if i < merged.len()
                                            && merged[i].default.is_none()
                                            && p.default.is_some()
                                        {
                                            merged[i].default = p.default;
                                        }
                                        if i < merged.len()
                                            && merged[i].constraint.is_none()
                                            && p.constraint.is_some()
                                        {
                                            merged[i].constraint = p.constraint;
                                        }
                                    }
                                } else {
                                    merged_params = Some(params);
                                }
                            } else if fallback_params.is_none() {
                                fallback_params = Some(params);
                            }
                        }
                    } else {
                        if arena.get(decl_idx).is_none() {
                            continue;
                        }
                        if let Some(params) = self.extract_simple_type_params_from_decl_in_arena(
                            arena.as_ref(),
                            flags,
                            decl_idx,
                            &sym_escaped_name,
                        ) {
                            if !params.is_empty() {
                                if let Some(ref mut merged) = merged_params {
                                    for (i, p) in params.into_iter().enumerate() {
                                        if i < merged.len()
                                            && merged[i].default.is_none()
                                            && p.default.is_some()
                                        {
                                            merged[i].default = p.default;
                                        }
                                        if i < merged.len()
                                            && merged[i].constraint.is_none()
                                            && p.constraint.is_some()
                                        {
                                            merged[i].constraint = p.constraint;
                                        }
                                    }
                                } else {
                                    merged_params = Some(params);
                                }
                            } else if fallback_params.is_none() {
                                fallback_params = Some(params);
                            }
                            continue;
                        }
                        let cache_file_idx = self
                            .ctx
                            .get_file_idx_for_arena(arena.as_ref())
                            .map(|i| i as u32);
                        let cached = if let Some(file_idx) = cache_file_idx {
                            self.ctx
                                .cross_file_type_params_cache
                                .as_ref()
                                .and_then(|cache| {
                                    cache.get(&(file_idx, decl_idx)).map(|e| e.value().clone())
                                })
                        } else {
                            None
                        };
                        let params = if let Some(memo) = cached {
                            tsz_common::perf_counters::record_cross_file_type_params_cache_hit();
                            Some(memo)
                        } else if let Some(_cross_arena_guard) =
                            Self::enter_cross_arena_delegation()
                        {
                            tsz_common::perf_counters::record_cross_file_type_params_cache_miss();
                            let decl_binder = self
                                .ctx
                                .get_binder_for_arena(arena.as_ref())
                                .unwrap_or(self.ctx.binder);
                            let decl_file_name = arena
                                .source_files
                                .first()
                                .map(|sf| sf.file_name.clone())
                                .unwrap_or_else(|| self.ctx.file_name.clone());
                            let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
                                arena.as_ref(),
                                decl_binder,
                                self.ctx.types,
                                decl_file_name,
                                self.ctx.compiler_options.clone(),
                                self,
                                tsz_common::perf_counters::CheckerCreationReason::TypeEnvironmentCore,
                            ));
                            if let Some(file_idx) = cache_file_idx {
                                checker.ctx.current_file_idx = file_idx as usize;
                            }
                            let result = Self::extract_type_params_from_decl(
                                &mut checker,
                                flags,
                                decl_idx,
                                &sym_escaped_name,
                            );
                            if let Some(ref params) = result
                                && let (Some(file_idx), Some(cache)) = (
                                    cache_file_idx,
                                    self.ctx.cross_file_type_params_cache.as_ref(),
                                )
                            {
                                cache
                                    .entry((file_idx, decl_idx))
                                    .or_insert_with(|| params.clone());
                            }
                            result
                        } else {
                            None
                        };
                        if let Some(params) = params {
                            if !params.is_empty() {
                                if let Some(ref mut merged) = merged_params {
                                    for (i, p) in params.into_iter().enumerate() {
                                        if i < merged.len()
                                            && merged[i].default.is_none()
                                            && p.default.is_some()
                                        {
                                            merged[i].default = p.default;
                                        }
                                        if i < merged.len()
                                            && merged[i].constraint.is_none()
                                            && p.constraint.is_some()
                                        {
                                            merged[i].constraint = p.constraint;
                                        }
                                    }
                                } else {
                                    merged_params = Some(params);
                                }
                            } else if fallback_params.is_none() {
                                fallback_params = Some(params);
                            }
                        }
                    }
                }
            }

            // Symbol-path fallback: deliberately keeps the dynamic-overlay-first
            // resolver. For a re-exported alias the followed-chain owner lives
            // only in the dynamic overlay; preferring the declaring index reads
            // the type-params from the wrong arena and produces spurious member
            // mismatches through re-exported alias intersections (regression
            // witnessed by `cross_file_recursive_alias_intersection_tests`). The
            // arena-path above (`declaration_arenas`) is already order-
            // independent; the #13255 stabilization is applied at the delegation
            // cache-KEY sites (`delegate_cross_arena_*`), not here.
            if !checked_local && let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id) {
                let arena = self.ctx.get_arena_for_file(file_idx as u32);
                if !std::ptr::eq(arena, self.ctx.arena) {
                    checked_local = true;
                    if arena.get(decl_idx).is_none() {
                        continue;
                    }
                    if let Some(params) = self.extract_simple_type_params_from_decl_in_arena(
                        arena,
                        flags,
                        decl_idx,
                        &sym_escaped_name,
                    ) {
                        if !params.is_empty() {
                            if let Some(ref mut merged) = merged_params {
                                for (i, p) in params.into_iter().enumerate() {
                                    if i < merged.len()
                                        && merged[i].default.is_none()
                                        && p.default.is_some()
                                    {
                                        merged[i].default = p.default;
                                    }
                                    if i < merged.len()
                                        && merged[i].constraint.is_none()
                                        && p.constraint.is_some()
                                    {
                                        merged[i].constraint = p.constraint;
                                    }
                                }
                            } else {
                                merged_params = Some(params);
                            }
                        } else if fallback_params.is_none() {
                            fallback_params = Some(params);
                        }
                        continue;
                    }
                    let cached = self
                        .ctx
                        .cross_file_type_params_cache
                        .as_ref()
                        .and_then(|cache| {
                            cache
                                .get(&(file_idx as u32, decl_idx))
                                .map(|e| e.value().clone())
                        });
                    let params = if let Some(memo) = cached {
                        tsz_common::perf_counters::record_cross_file_type_params_cache_hit();
                        Some(memo)
                    } else if let Some(_cross_arena_guard) = Self::enter_cross_arena_delegation() {
                        tsz_common::perf_counters::record_cross_file_type_params_cache_miss();
                        let decl_binder = self
                            .ctx
                            .get_binder_for_file(file_idx)
                            .unwrap_or(self.ctx.binder);
                        let decl_file_name = arena
                            .source_files
                            .first()
                            .map(|sf| sf.file_name.clone())
                            .unwrap_or_else(|| self.ctx.file_name.clone());
                        let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
                            arena,
                            decl_binder,
                            self.ctx.types,
                            decl_file_name,
                            self.ctx.compiler_options.clone(),
                            self,
                            tsz_common::perf_counters::CheckerCreationReason::TypeEnvironmentCore,
                        ));
                        checker.ctx.current_file_idx = file_idx;
                        let result = Self::extract_type_params_from_decl(
                            &mut checker,
                            flags,
                            decl_idx,
                            &sym_escaped_name,
                        );
                        if let Some(ref params) = result
                            && let Some(ref cache) = self.ctx.cross_file_type_params_cache
                        {
                            cache
                                .entry((file_idx as u32, decl_idx))
                                .or_insert_with(|| params.clone());
                        }
                        result
                    } else {
                        None
                    };
                    if let Some(params) = params {
                        if !params.is_empty() {
                            if let Some(ref mut merged) = merged_params {
                                for (i, p) in params.into_iter().enumerate() {
                                    if i < merged.len()
                                        && merged[i].default.is_none()
                                        && p.default.is_some()
                                    {
                                        merged[i].default = p.default;
                                    }
                                    if i < merged.len()
                                        && merged[i].constraint.is_none()
                                        && p.constraint.is_some()
                                    {
                                        merged[i].constraint = p.constraint;
                                    }
                                }
                            } else {
                                merged_params = Some(params);
                            }
                        } else if fallback_params.is_none() {
                            fallback_params = Some(params);
                        }
                    }
                }
            }

            if !checked_local
                && let Some(params) =
                    Self::extract_type_params_from_decl(self, flags, decl_idx, &sym_escaped_name)
            {
                if !params.is_empty() {
                    if let Some(ref mut merged) = merged_params {
                        for (i, p) in params.into_iter().enumerate() {
                            if i < merged.len()
                                && merged[i].default.is_none()
                                && p.default.is_some()
                            {
                                merged[i].default = p.default;
                            }
                            if i < merged.len()
                                && merged[i].constraint.is_none()
                                && p.constraint.is_some()
                            {
                                merged[i].constraint = p.constraint;
                            }
                        }
                    } else {
                        merged_params = Some(params);
                    }
                } else if fallback_params.is_none() {
                    fallback_params = Some(params);
                }
            }
        }

        if let Some(params) = merged_params {
            self.ctx.insert_def_type_params(def_id, params.clone());
            self.ctx.def_no_type_params.borrow_mut().remove(&def_id);
            self.ctx.leave_recursion();
            return params;
        }

        if let Some(params) = fallback_params {
            self.ctx.def_no_type_params.borrow_mut().insert(def_id);
            self.ctx.leave_recursion();
            return params;
        }

        self.ctx.def_no_type_params.borrow_mut().insert(def_id);
        self.ctx.leave_recursion();
        Vec::new()
    }

    pub(crate) fn get_display_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let params = self.get_type_params_for_symbol(sym_id);
        if !params.is_empty() {
            return params;
        }

        self.get_type_param_names_for_symbol_from_ast(sym_id)
            .into_iter()
            .map(|name| tsz_solver::TypeParamInfo {
                name: self.ctx.types.intern_string(&name),
                constraint: None,
                default: None,
                is_const: false,
                origin: tsz_solver::TypeParamOrigin::User,
            })
            .collect()
    }

    /// Count the number of required type parameters for a symbol.
    ///
    /// A type parameter is "required" if it doesn't have a default value.
    /// This is important for validating generic type usage and error messages.
    ///
    /// ## Required vs Optional:
    /// - **Required**: Must be explicitly provided by the caller
    /// - **Optional**: Has a default value, can be omitted
    ///
    /// ## Use Cases:
    /// - Validating that enough type arguments are provided
    /// - Error messages: "Expected X type arguments but got Y"
    /// - Generic function/method overload resolution
    ///
    /// ## TypeScript Examples:
    /// ```typescript
    /// // All required
    /// interface Pair<T, U> {}
    /// // count_required_type_params(Pair) → 2
    /// const x: Pair = {};  // ❌ Error: Expected 2 type arguments
    /// const y: Pair<string, number> = {};  // ✅
    ///
    /// // One optional
    /// interface Box<T = string> {}
    /// // count_required_type_params(Box) → 0 (T has default)
    /// const a: Box = {};  // ✅ T defaults to string
    /// const b: Box<number> = {};  // ✅ Explicit number
    ///
    /// // Mixed required and optional
    /// interface Map<K, V = any> {}
    /// // count_required_type_params(Map) → 1 (K required, V optional)
    /// const m1: Map<string> = {};  // ✅ K=string, V=any
    /// const m2: Map<string, number> = {};  // ✅ Both specified
    /// const m3: Map = {};  // ❌ K is required
    /// ```
    pub(crate) fn count_required_type_params(&mut self, sym_id: SymbolId) -> usize {
        let builtin_override = self.get_symbol_globally(sym_id).and_then(|symbol| {
            match symbol.escaped_name.as_str() {
                "Iterator"
                | "Iterable"
                | "AsyncIterator"
                | "AsyncIterable"
                | "IterableIterator"
                | "AsyncIterableIterator"
                | "IteratorObject"
                | "AsyncIteratorObject" => Some(1),
                "Generator" | "AsyncGenerator" => Some(0),
                _ => None,
            }
        });

        // First try the fast AST-level check. This avoids recursive resolution
        // issues when a type parameter default references the type being declared
        // (e.g., `interface SelfRef<T = SelfRef> {}`). In such cases,
        // `get_type_params_for_symbol` would recursively try to resolve the
        // default, fail, and incorrectly report the param as required.
        if let Some(ast_count) = self.count_required_type_params_from_ast(sym_id) {
            if let Some(override_count) = builtin_override
                && ast_count > override_count
            {
                return override_count;
            }
            return ast_count;
        }
        let type_params = self.get_type_params_for_symbol(sym_id);
        let required = type_params.iter().filter(|p| p.default.is_none()).count();
        if let Some(override_count) = builtin_override
            && required > override_count
        {
            return override_count;
        }
        required
    }

    /// Count required type params by inspecting the AST directly, without resolving
    /// defaults. Returns `Some(count)` if AST-level info is available, `None` otherwise.
    pub(crate) fn count_required_type_params_from_ast(&self, sym_id: SymbolId) -> Option<usize> {
        let symbol = self.get_symbol_globally(sym_id)?;
        let flags = symbol.flags;
        let sym_escaped_name = symbol.escaped_name.as_str();
        let decl_candidates = symbol.all_declarations();

        // Track the minimum required count across all declarations.
        // For merged interfaces (e.g., local `interface Generator<T>` merged with
        // lib `interface Generator<T = unknown, TReturn = any, TNext = any>`),
        // a declaration with defaults on its type params reduces the required count.
        let mut best_required: Option<usize> = None;

        for decl_idx in decl_candidates {
            // Try the current arena first, then cross-arena lookup for lib files.
            let result = Self::count_required_params_in_arena(
                self.ctx.arena,
                flags,
                decl_idx,
                sym_escaped_name,
            )
            .or_else(|| {
                // For lib file declarations, the node lives in a different arena.
                // Look up the correct arena via declaration_arenas.
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    for arena in arenas {
                        if let Some(count) = Self::count_required_params_in_arena(
                            arena.as_ref(),
                            flags,
                            decl_idx,
                            sym_escaped_name,
                        ) {
                            return Some(count);
                        }
                    }
                }
                self.ctx
                    .binder
                    .symbol_arenas
                    .get(&sym_id)
                    .and_then(|arena| {
                        Self::count_required_params_in_arena(
                            arena.as_ref(),
                            flags,
                            decl_idx,
                            sym_escaped_name,
                        )
                    })
            });

            if let Some(required) = result {
                best_required = Some(match best_required {
                    Some(prev) => prev.min(required),
                    None => required,
                });
            }
        }
        best_required
    }

    fn get_type_param_names_for_symbol_from_ast(&self, sym_id: SymbolId) -> Vec<String> {
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return Vec::new();
        };

        let flags = symbol.flags;
        let decl_candidates = symbol.all_declarations();

        for decl_idx in decl_candidates {
            if let Some(names) = Self::type_param_names_in_arena(
                self.ctx.arena,
                flags,
                decl_idx,
                &symbol.escaped_name,
            ) && !names.is_empty()
            {
                return names;
            }

            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    if let Some(names) = Self::type_param_names_in_arena(
                        arena.as_ref(),
                        flags,
                        decl_idx,
                        &symbol.escaped_name,
                    ) && !names.is_empty()
                    {
                        return names;
                    }
                }
            }

            if let Some(names) = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .and_then(|arena| {
                    Self::type_param_names_in_arena(
                        arena.as_ref(),
                        flags,
                        decl_idx,
                        &symbol.escaped_name,
                    )
                })
                && !names.is_empty()
            {
                return names;
            }
        }

        Vec::new()
    }

    /// Count required type params for a single declaration in a specific arena.
    ///
    /// The declaration name is validated against `sym_escaped_name` so a
    /// `decl_idx` from another file's arena cannot be read against an unrelated
    /// arena where the same numeric index lands on a foreign declaration
    /// (issue #13599).
    fn count_required_params_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        flags: u32,
        decl_idx: NodeIndex,
        sym_escaped_name: &str,
    ) -> Option<usize> {
        let node = arena.get(decl_idx)?;
        let type_params_list = if flags & tsz_binder::symbol_flags::INTERFACE != 0 {
            let iface = arena.get_interface(node)?;
            if !Self::decl_name_matches_in_arena(arena, iface.name, sym_escaped_name) {
                return None;
            }
            iface.type_parameters.as_ref()
        } else if flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0 {
            let ta = arena.get_type_alias(node)?;
            if !Self::decl_name_matches_in_arena(arena, ta.name, sym_escaped_name) {
                return None;
            }
            ta.type_parameters.as_ref()
        } else if flags & tsz_binder::symbol_flags::CLASS != 0 {
            let class = arena.get_class(node)?;
            if !Self::decl_name_matches_in_arena(arena, class.name, sym_escaped_name) {
                return None;
            }
            class.type_parameters.as_ref()
        } else {
            None
        };

        if let Some(list) = type_params_list {
            let required = list
                .nodes
                .iter()
                .filter(|&&param_idx| {
                    arena
                        .get(param_idx)
                        .and_then(|n| arena.get_type_parameter(n))
                        .is_some_and(|tp| tp.default == tsz_parser::parser::NodeIndex::NONE)
                })
                .count();
            return Some(required);
        }
        None
    }

    /// Returns `true` when the declaration `name_idx` in `arena` resolves to an
    /// identifier whose text equals `sym_escaped_name`, or when the name cannot
    /// be resolved at all (anonymous/unnamed declarations are accepted for
    /// backward compatibility).
    ///
    /// This guards against a `decl_idx` (a `NodeIndex` that is only meaningful
    /// inside the arena that owns the symbol) being read against an unrelated
    /// arena, where the same numeric index can land on a foreign declaration.
    /// Without this check, a non-generic imported alias can inherit an unrelated
    /// generic's type parameters (issue #13599).
    fn decl_name_matches_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        name_idx: NodeIndex,
        sym_escaped_name: &str,
    ) -> bool {
        match arena
            .get(name_idx)
            .and_then(|name_node| arena.get_identifier(name_node))
        {
            Some(name_ident) => name_ident.escaped_text.as_str() == sym_escaped_name,
            None => true,
        }
    }

    pub(crate) fn type_param_names_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        flags: u32,
        decl_idx: NodeIndex,
        sym_escaped_name: &str,
    ) -> Option<Vec<String>> {
        let node = arena.get(decl_idx)?;
        let type_params_list = if flags & tsz_binder::symbol_flags::INTERFACE != 0 {
            let iface = arena.get_interface(node)?;
            if !Self::decl_name_matches_in_arena(arena, iface.name, sym_escaped_name) {
                return None;
            }
            iface.type_parameters.as_ref()
        } else if flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0 {
            let ta = arena.get_type_alias(node)?;
            if !Self::decl_name_matches_in_arena(arena, ta.name, sym_escaped_name) {
                return None;
            }
            ta.type_parameters.as_ref()
        } else if flags & tsz_binder::symbol_flags::CLASS != 0 {
            let class = arena.get_class(node)?;
            if !Self::decl_name_matches_in_arena(arena, class.name, sym_escaped_name) {
                return None;
            }
            class.type_parameters.as_ref()
        } else {
            None
        }?;

        let names = type_params_list
            .nodes
            .iter()
            .filter_map(|&param_idx| {
                let param_node = arena.get(param_idx)?;
                let param = arena.get_type_parameter(param_node)?;
                let name_node = arena.get(param.name)?;
                let ident = arena.get_identifier(name_node)?;
                Some(arena.resolve_identifier_text(ident).to_string())
            })
            .collect::<Vec<_>>();

        Some(names)
    }
}
