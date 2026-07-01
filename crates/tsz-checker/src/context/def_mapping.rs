//! DefId migration helpers for `CheckerContext`.
//!
//! Handles bidirectional mapping between `SymbolId` and `DefId`, lazy type
//! references, type parameter registration, and resolved-type registration
//! in the `TypeEnvironment`.

use std::sync::Arc;

use crate::query_boundaries::common::TypeResolver;
use tracing::trace;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

use crate::context::CheckerContext;
use crate::context::def_mapping_env_writes::{
    apply_or_defer_env_write, drain_env_write_queue_into, flush_env_write_queue,
};
use crate::context::deferred_flow_env_write::DeferredFlowEnvWrite;
use crate::query_boundaries::common::TypeEnvironment;

/// Whether to eagerly bulk-copy every cross-file symbol of the whole program
/// into each per-file checker's local `symbol_to_def` / `def_to_symbol` maps.
///
/// Default is `false`: local caches populate lazily on demand (every reader has
/// an authoritative O(1) fallback into the shared `DefinitionStore`), so the
/// per-file cost stays independent of total program size. Setting
/// `TSZ_EAGER_WARM_LOCAL_CACHES=1` restores the legacy eager copy as a rollback
/// escape hatch. The value is read once and cached.
fn eager_warm_local_caches() -> bool {
    use std::sync::OnceLock;
    static EAGER: OnceLock<bool> = OnceLock::new();
    *EAGER.get_or_init(|| {
        std::env::var("TSZ_EAGER_WARM_LOCAL_CACHES")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

impl CheckerContext<'_> {
    /// Get or create a `DefId` for a symbol.
    ///
    /// If the symbol already has a `DefId`, return it.
    /// Otherwise, create a new `DefId` and store the mapping.
    ///
    /// This is used during the migration from `SymbolRef` to `DefId`.
    /// Eventually, all type references will use `DefId` directly.
    ///
    /// ## Lookup strategy
    ///
    /// 1. **Local cache** (`symbol_to_def`): O(1) `FxHashMap` lookup, no locking.
    /// 2. **Authoritative index** (`DefinitionStore::symbol_def_index`): O(1)
    ///    `DashMap` lookup keyed by `(symbol_id, file_idx)`. This naturally
    ///    disambiguates the same raw `SymbolId(u32)` across different binders
    ///    and eliminates the expensive multi-binder name-validation that was
    ///    previously done on every cache hit.
    /// 3. **Create**: look up the symbol, build `DefinitionInfo`, register in
    ///    both the store and the index.
    pub fn get_or_create_def_id(&self, sym_id: SymbolId) -> DefId {
        use tsz_solver::def::DefinitionInfo;
        let local_symbol = self.binder.symbols.get(sym_id);
        let authoritative_file_idx = self.resolve_symbol_file_index(sym_id);
        let has_cross_file_collision = authoritative_file_idx
            .is_some_and(|file_idx| file_idx != self.current_file_idx)
            && local_symbol.is_some()
            && authoritative_file_idx
                .and_then(|file_idx| self.get_binder_for_file(file_idx))
                .and_then(|binder| binder.get_symbol(sym_id))
                .is_some();
        let prefer_local_symbol = local_symbol.is_some_and(|symbol| {
            let authoritative_is_current =
                authoritative_file_idx.is_none_or(|file_idx| file_idx == self.current_file_idx);
            authoritative_is_current
                && symbol.import_module().is_none()
                && !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
        });
        let symbol_name = self
            .binder
            .symbols
            .get(sym_id)
            .or_else(|| {
                self.lib_contexts
                    .iter()
                    .find_map(|lib_ctx| lib_ctx.binder.symbols.get(sym_id))
            })
            .or_else(|| {
                self.all_binders.as_ref().and_then(|binders| {
                    binders.iter().find_map(|binder| binder.symbols.get(sym_id))
                })
            })
            .map(|symbol| symbol.escaped_name.clone());

        // ---- Step 1: local cache fast path ----
        if let Some(def_id) = self.symbol_to_def.borrow().get(&sym_id).copied() {
            let authoritative = authoritative_file_idx.and_then(|file_idx| {
                self.get_binder_for_file(file_idx).and_then(|binder| {
                    binder.get_symbol(sym_id).and_then(|_| {
                        self.definition_store
                            .lookup_by_symbol(sym_id.0, file_idx as u32)
                    })
                })
            });
            let cached_file_idx = self
                .definition_store
                .get(def_id)
                .and_then(|info| info.file_id)
                .map(|file_idx| file_idx as usize);
            let cached_matches_name = self.definition_store.get(def_id).is_some_and(|info| {
                symbol_name
                    .as_ref()
                    .is_some_and(|name| self.types.resolve_atom(info.name) == *name)
            });
            let cached_matches_cross_file =
                has_cross_file_collision && cached_file_idx == authoritative_file_idx;
            if cached_matches_cross_file
                || (cached_matches_name
                    && !has_cross_file_collision
                    && authoritative.is_none_or(|auth| auth == def_id))
            {
                return def_id;
            }
        }

        // ---- Step 2: authoritative symbol-only index (O(1)) ----
        // Check the DefinitionStore's symbol_only_index before doing any binder
        // lookups. This avoids O(N) lib_contexts/all_binders scans for symbols
        // that already have DefIds from pre-population or previous contexts.
        let symbol = authoritative_file_idx
            .and_then(|file_idx| self.get_binder_for_file(file_idx))
            .and_then(|binder| binder.get_symbol(sym_id))
            .filter(|_| !prefer_local_symbol)
            .or_else(|| {
                local_symbol
                    .or_else(|| {
                        self.lib_contexts
                            .iter()
                            .find_map(|lib_ctx| lib_ctx.binder.symbols.get(sym_id))
                    })
                    .or_else(|| {
                        self.all_binders.as_ref().and_then(|binders| {
                            binders.iter().find_map(|binder| binder.symbols.get(sym_id))
                        })
                    })
            });
        if let Some(symbol) = symbol
            && let Some(def_id) = self.definition_store.lookup_by_symbol(
                sym_id.0,
                authoritative_file_idx
                    .map(|idx| idx as u32)
                    .unwrap_or(symbol.decl_file_idx),
            )
            && self.definition_store.get(def_id).is_some_and(|info| {
                symbol_name
                    .as_ref()
                    .is_some_and(|name| self.types.resolve_atom(info.name) == *name)
            })
        {
            // Populate local caches for future fast-path hits.
            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return def_id;
        }

        // ---- Step 3: look up the symbol to get its file_idx ----
        // We need the symbol to determine which binder it came from.
        // This O(N) scan only runs for truly new DefIds (not yet in DefinitionStore).
        let symbol = authoritative_file_idx
            .and_then(|file_idx| self.get_binder_for_file(file_idx))
            .and_then(|binder| binder.get_symbol(sym_id))
            .or_else(|| {
                self.binder
                    .symbols
                    .get(sym_id)
                    .or_else(|| {
                        self.lib_contexts
                            .iter()
                            .find_map(|lib_ctx| lib_ctx.binder.symbols.get(sym_id))
                    })
                    .or_else(|| {
                        self.all_binders.as_ref().and_then(|binders| {
                            binders.iter().find_map(|binder| binder.symbols.get(sym_id))
                        })
                    })
            });

        let symbol = match symbol {
            Some(s) => s,
            None => return DefId::INVALID,
        };

        let file_idx = if prefer_local_symbol && self.current_file_idx != usize::MAX {
            self.current_file_idx as u32
        } else {
            authoritative_file_idx
                .map(|idx| idx as u32)
                .unwrap_or(symbol.decl_file_idx)
        };
        // ---- Step 3b: composite key lookup ----
        // The composite key (symbol_id, file_idx) uniquely identifies a symbol
        // across all binders, so no name-validation is needed.
        if let Some(def_id) = self.definition_store.lookup_by_symbol(sym_id.0, file_idx) {
            // Populate local caches for future fast-path hits.
            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return def_id;
        }

        // ---- Step 4: create new DefId ----
        let name = self.types.intern_string(&symbol.escaped_name);

        // Determine DefKind from symbol flags.
        // CLASS is checked before INTERFACE because declaration merging can give
        // a symbol both flags (e.g., `class Component<P,S>` + interface augmentation).
        // A class-with-interface-merge is semantically still a class.
        let kind = if symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            tsz_solver::def::DefKind::TypeAlias
        } else if symbol.has_any_flags(tsz_binder::symbol_flags::CLASS) {
            tsz_solver::def::DefKind::Class
        } else if symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE) {
            tsz_solver::def::DefKind::Interface
        } else if symbol.has_any_flags(tsz_binder::symbol_flags::ENUM) {
            tsz_solver::def::DefKind::Enum
        } else if symbol.has_any_flags(
            tsz_binder::symbol_flags::NAMESPACE_MODULE | tsz_binder::symbol_flags::VALUE_MODULE,
        ) {
            tsz_solver::def::DefKind::Namespace
        } else if symbol.has_any_flags(tsz_binder::symbol_flags::FUNCTION) {
            tsz_solver::def::DefKind::Function
        } else if symbol.has_any_flags(
            tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE
                | tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE,
        ) {
            tsz_solver::def::DefKind::Variable
        } else {
            // Default to TypeAlias for remaining symbols (type parameters, etc.)
            tsz_solver::def::DefKind::TypeAlias
        };

        // Create a placeholder DefinitionInfo - body will be set lazily.
        // Prefer binder-owned stable declaration spans over raw NodeIndex-based
        // reconstruction so fallback identity does not treat syntax handles as
        // semantic coordinates.
        let span = symbol.first_declaration_span().or_else(|| {
            if symbol.value_declaration.is_some() {
                symbol.value_declaration_span()
            } else {
                None
            }
        });

        let info = DefinitionInfo {
            kind,
            name,
            type_params: Vec::new(), // Will be populated when type is resolved
            body: None,              // Lazy: computed on first access
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(), // Will be populated for namespaces/modules
            file_id: Some(file_idx),
            span,
            symbol_id: Some(sym_id.0),
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        };

        // Atomic mint: under parallel fresh checking, a sibling checker can
        // race this same `(symbol, file)` identity between the lookup above
        // and this registration; the entry-guarded form converges both
        // checkers on one `DefId` (issue #13255 identity splits).
        let (def_id, minted) = self
            .definition_store
            .register_for_symbol(sym_id.0, file_idx, info);
        if minted {
            trace!(
                symbol_name = %symbol.escaped_name,
                symbol_id = %sym_id.0,
                def_id = %def_id.0,
                kind = ?kind,
                "DefId fallback: created new DefId on demand (not pre-populated)"
            );

            // Track fallback firings for observability. If this counter grows
            // unexpectedly, it indicates binder semantic_defs coverage gaps.
            self.def_fallback_count
                .set(self.def_fallback_count.get() + 1);
        }

        // Populate local caches.
        self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
        self.def_to_symbol.borrow_mut().insert(def_id, sym_id);

        // Propagate DefKind to both TypeEnvironments so both the evaluator
        // and flow-analyzer can query it.
        self.register_def_kind_in_envs(def_id, kind);

        def_id
    }

    /// Get or create a `DefId` for a symbol when the syntactic reference name is known.
    ///
    /// Raw `SymbolId` values are only unique within a binder. Type lowering can see a
    /// lib/cross-file symbol through a raw id that collides with another binder's
    /// symbol. Use the source name to select the matching binder symbol before falling
    /// back to the general raw-id path.
    pub fn get_or_create_def_id_for_symbol_name(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> DefId {
        let authoritative_file_idx = self.resolve_symbol_file_index(sym_id);
        let authoritative_symbol = authoritative_file_idx
            .and_then(|file_idx| self.get_binder_for_file(file_idx))
            .and_then(|binder| binder.get_symbol(sym_id))
            .filter(|symbol| symbol.escaped_name == expected_name);
        let authoritative_symbol_exists = authoritative_symbol.is_some();
        let cached_def_id = self.symbol_to_def.borrow().get(&sym_id).copied();
        if let Some(def_id) = cached_def_id {
            let cached_matches_authoritative_decl = if let (Some(file_idx), Some(symbol)) =
                (authoritative_file_idx, authoritative_symbol)
            {
                self.definition_store.get(def_id).is_some_and(|info| {
                    self.def_info_matches_symbol_declaration(
                        &info,
                        sym_id,
                        symbol,
                        file_idx as u32,
                        expected_name,
                    )
                })
            } else {
                self.definition_store.get(def_id).is_some_and(|info| {
                    self.types.resolve_atom(info.name) == expected_name
                        && authoritative_file_idx.is_none_or(|file_idx| {
                            let file_idx = file_idx as u32;
                            self.definition_store.lookup_by_symbol(sym_id.0, file_idx)
                                == Some(def_id)
                                || info.file_id == Some(file_idx)
                        })
                })
            };
            if cached_matches_authoritative_decl {
                return def_id;
            }

            if !authoritative_symbol_exists
                && let Some(lib_sym_id) = self.lib_contexts.iter().find_map(|lib_ctx| {
                    lib_ctx
                        .binder
                        .file_locals
                        .get(expected_name)
                        .filter(|&candidate| candidate != sym_id)
                        .filter(|&candidate| {
                            lib_ctx
                                .binder
                                .get_symbol(candidate)
                                .is_some_and(|symbol| symbol.escaped_name == expected_name)
                        })
                })
            {
                return self.get_canonical_lib_def_id(expected_name, lib_sym_id);
            }
        }

        let matching_symbol = authoritative_symbol.or_else(|| {
            self.binder
                .symbols
                .get(sym_id)
                .filter(|symbol| symbol.escaped_name == expected_name)
                .or_else(|| {
                    self.lib_contexts.iter().find_map(|lib_ctx| {
                        lib_ctx
                            .binder
                            .symbols
                            .get(sym_id)
                            .filter(|symbol| symbol.escaped_name == expected_name)
                    })
                })
                .or_else(|| {
                    self.all_binders.as_ref().and_then(|binders| {
                        binders.iter().find_map(|binder| {
                            binder
                                .symbols
                                .get(sym_id)
                                .filter(|symbol| symbol.escaped_name == expected_name)
                        })
                    })
                })
        });

        let Some(symbol) = matching_symbol else {
            return self.get_or_create_def_id(sym_id);
        };
        // Lib provenance must include current-binder *clones*:
        // `merge_lib_contexts_into_binder` remaps standard-lib symbols into
        // each per-file binder's `file_locals`, so the narrow arena-pointer
        // check (`symbol_is_from_actual_lib`) misses them. Treating a cloned
        // lib symbol as a current-file local mints a file-attributed `DefId`
        // for a lib type in *every* per-file checker — those defs never
        // receive the heritage-merged finalized body, and under parallel
        // fresh checking sibling checkers resolve them to pre-heritage
        // intermediate forms (issue #13255 witness 3 false TS2741).
        // Deliberately checks `lib_symbol_ids` directly rather than the
        // broader `symbol_is_from_actual_or_cloned_lib`: that helper also
        // classifies arena-less / unmapped-arena symbols as lib, which
        // would misattribute cross-arena program symbols here.
        let is_current_file_local_symbol = self.binder.file_locals.get(expected_name)
            == Some(sym_id)
            && self
                .binder
                .symbols
                .get(sym_id)
                .is_some_and(|symbol| symbol.escaped_name == expected_name)
            && !self.symbol_is_from_actual_lib(sym_id)
            && !self.binder.lib_symbol_ids.contains(&sym_id);
        let symbol_index_matches_name = self
            .definition_store
            .find_def_by_symbol(sym_id.0)
            .and_then(|def_id| self.definition_store.get(def_id))
            .is_some_and(|info| self.types.resolve_atom(info.name) == expected_name);
        if symbol.decl_file_idx == u32::MAX
            && !is_current_file_local_symbol
            && !symbol_index_matches_name
            && let Some(lib_sym_id) = self.lib_contexts.iter().find_map(|lib_ctx| {
                lib_ctx
                    .binder
                    .file_locals
                    .get(expected_name)
                    .filter(|&candidate| candidate != sym_id)
                    .filter(|&candidate| {
                        lib_ctx
                            .binder
                            .get_symbol(candidate)
                            .is_some_and(|symbol| symbol.escaped_name == expected_name)
                    })
            })
        {
            return self.get_canonical_lib_def_id(expected_name, lib_sym_id);
        }
        let file_idx = if is_current_file_local_symbol && self.current_file_idx != usize::MAX {
            self.current_file_idx as u32
        } else {
            symbol.decl_file_idx
        };
        if let Some(def_id) = self.definition_store.lookup_by_symbol(sym_id.0, file_idx)
            && self.definition_store.get(def_id).is_some_and(|info| {
                self.def_info_matches_symbol_declaration(
                    &info,
                    sym_id,
                    symbol,
                    file_idx,
                    expected_name,
                )
            })
        {
            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return def_id;
        }

        self.mint_def_for_symbol_in_file(sym_id, symbol, file_idx)
    }

    /// Get or create a `DefId` for a lib symbol.
    ///
    /// Lib symbols *should* already have `DefIds` from pre-population
    /// (`pre_populate_def_ids_from_lib_binders`). This method first checks
    /// the pre-populated index and only falls back to `get_or_create_def_id`
    /// as a safety net, logging a trace when the fallback fires.
    ///
    /// Use this instead of the manual `get_existing_def_id().unwrap_or_else(||
    /// get_or_create_def_id())` pattern in lib resolution paths.
    pub fn get_lib_def_id(&self, sym_id: SymbolId) -> DefId {
        if let Some(def_id) = self.get_existing_def_id(sym_id) {
            return def_id;
        }
        // Pre-population missed this symbol — create on demand but log it.
        // If this fires frequently for a specific symbol kind, the binder's
        // `record_semantic_def` coverage should be extended.
        trace!(
            symbol_id = %sym_id.0,
            "lib symbol not pre-populated, creating DefId on demand"
        );
        self.get_or_create_def_id(sym_id)
    }

    /// Return the canonical `SymbolId` for a lib symbol name.
    ///
    /// Prefers the main (merged) binder's `file_locals` entry because that
    /// identity is what `DefId`s are keyed to after `merge_lib_contexts_into_binder`.
    /// Falls back to `per_lib_sym_id` (from an individual lib context binder)
    /// only when the main binder doesn't carry the symbol yet — a scenario that
    /// can happen with lazily-loaded or target-gated lib files.
    ///
    /// Callers should use this instead of the inline `main_sym_id.unwrap_or(sym_id)`
    /// recovery pattern.
    /// True when `sym_id` is a local *value-only* binding (no type-position
    /// meaning) that is not itself a lib symbol — i.e. a value that merely
    /// shadows a same-named global lib type (`const Readonly: unique symbol`
    /// vs the lib `Readonly<T>`). Such a binding occupies only the value
    /// namespace, so it must never stand in for the lib type's canonical
    /// identity in type position.
    fn is_value_only_lib_shadow(&self, sym_id: SymbolId) -> bool {
        use tsz_binder::symbol_flags;
        !self.symbol_is_from_actual_or_cloned_lib(sym_id)
            && self.binder.get_symbol(sym_id).is_some_and(|s| {
                s.has_any_flags(symbol_flags::VALUE)
                    && !s.has_any_flags(
                        symbol_flags::TYPE
                            | symbol_flags::NAMESPACE_MODULE
                            | symbol_flags::VALUE_MODULE
                            | symbol_flags::ALIAS,
                    )
            })
    }

    pub fn canonical_lib_sym_id(&self, name: &str, per_lib_sym_id: SymbolId) -> SymbolId {
        if let Some(sym_id) = self.binder.file_locals.get(name)
            && !self.symbol_has_current_file_type_declaration(sym_id, name)
        {
            // A local *value-only* binding that merely shares a name with a
            // global lib type (e.g. `export declare const Readonly: unique
            // symbol`) must not be treated as the canonical lib symbol. It
            // carries no type-position meaning, so the lib type stays visible;
            // keying the lib type's `DefId` to this value symbol corrupts
            // deferred reduction — the lib `Readonly<…>` application then no
            // longer matches the mapped-type body registered for the lib symbol,
            // so it stays opaque (#8432 `deeplyNestedMappedTypes.ts`). Type-
            // position resolution already routes such shadows to the recorded
            // lib TYPE symbol via `lib_type_namespace`; mirror that here so the
            // application-base def and the body-registration def agree on the lib
            // symbol instead of the value. Genuine merged lib symbols, and any
            // symbol carrying type meaning, keep the existing fast path.
            if self.is_value_only_lib_shadow(sym_id) {
                if let Some(&lib_type_sym_id) = self.binder.lib_type_namespace.get(name) {
                    return lib_type_sym_id;
                }
                // No recorded lib type shadow: fall through to the lib-symbol
                // search below rather than returning the value symbol.
            } else {
                return sym_id;
            }
        }

        if let Some(sym_id) = self
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
            .and_then(|entries| {
                entries
                    .iter()
                    .filter(|&&(_, sym_id)| {
                        self.symbol_is_from_actual_or_cloned_lib(sym_id)
                            && !self.symbol_has_current_file_type_declaration(sym_id, name)
                    })
                    .max_by_key(|(_, sym)| sym.0)
            })
            .map(|&(_, sym)| sym)
        {
            return sym_id;
        }

        // HEURISTIC (NOT identity): when the local binder does not contain
        // merged lib symbols and the global index doesn't either, fall back to
        // scanning all known binders and picking the largest `SymbolId`.
        //
        // The assumption is that merged binders allocate from a wider shared
        // symbol space while per-lib binders frequently reuse low IDs. This is
        // an *allocator-order* property, not a semantic one — if allocation
        // order ever changes (e.g. the binder switches to a smaller-IDs-first
        // strategy, or an unrelated symbol gets a larger ID), this routine can
        // pick the wrong identity. The right long-term fix is to ask
        // `DefinitionStore` / a merged-binder marker for the canonical identity
        // explicitly; tracked under
        // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md` item #4.
        //
        // We log (trace) every time the heuristic fires AND chooses a non-input
        // candidate, so drift is visible without changing behavior.
        let mut best = per_lib_sym_id;
        let initial = per_lib_sym_id;
        if let Some(all_binders) = self.all_binders.as_ref() {
            for binder in all_binders.iter() {
                if let Some(sym_id) = binder.file_locals.get(name)
                    && sym_id.0 > best.0
                {
                    best = sym_id;
                }
            }
        }
        for lib_ctx in self.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name)
                && sym_id.0 > best.0
            {
                best = sym_id;
            }
        }
        if best != initial {
            tracing::trace!(
                target: "tsz_checker::canonical_lib_sym_id",
                name,
                input_sym_id = ?initial,
                chosen_sym_id = ?best,
                "fell back to largest-SymbolId heuristic — see ROBUSTNESS_AUDIT_2026-04-26 item #4"
            );
        }
        best
    }

    /// Return the `DefId` for a lib symbol, canonicalizing the `SymbolId` first.
    ///
    /// Combines [`canonical_lib_sym_id`] and [`get_lib_def_id`] into a single
    /// call. Use this in per-lib-context lowering paths (e.g.,
    /// `resolve_lib_type_with_params`) where the `SymbolId` comes from an
    /// individual lib binder and must be mapped to the merged-binder identity
    /// before creating/looking up the `DefId`.
    pub fn get_canonical_lib_def_id(&self, name: &str, per_lib_sym_id: SymbolId) -> DefId {
        let canonical_sym = self.canonical_lib_sym_id(name, per_lib_sym_id);
        if canonical_sym == per_lib_sym_id {
            let atom = self.types.intern_string(name);
            self.definition_store
                .find_defs_by_name(atom)
                .and_then(|defs| {
                    defs.into_iter()
                        .filter(|def_id| {
                            self.definition_store.get(*def_id).is_some_and(|info| {
                                info.symbol_id.is_some_and(|sym_id| {
                                    self.symbol_is_from_actual_or_cloned_lib(SymbolId(sym_id))
                                }) && matches!(
                                    info.kind,
                                    tsz_solver::def::DefKind::TypeAlias
                                        | tsz_solver::def::DefKind::Interface
                                        | tsz_solver::def::DefKind::Class
                                        | tsz_solver::def::DefKind::Enum
                                        | tsz_solver::def::DefKind::Namespace
                                )
                            })
                        })
                        .max_by_key(|def_id| {
                            self.definition_store
                                .get(*def_id)
                                .and_then(|info| info.symbol_id)
                                .unwrap_or_default()
                        })
                })
                .unwrap_or_else(|| self.get_lib_def_id(canonical_sym))
        } else {
            self.get_lib_def_id(canonical_sym)
        }
    }

    /// Resolve a lib symbol's `DefId`, verifying the def actually names
    /// `expected_name`.
    ///
    /// Raw `SymbolId`s are binder-relative, and every lib-binder symbol
    /// shares the `u32::MAX` declaration-file sentinel, so the raw-id
    /// resolution in [`get_lib_def_id`](Self::get_lib_def_id) can answer with
    /// a colliding def registered by a *different* lib binder for an
    /// unrelated name (and `get_existing_def_id`'s name guard self-confirms
    /// the collision because it derives the name from the first lib binder
    /// owning the raw id). When the caller knows which name the symbol must
    /// bind, verify the resolved def against it and, on mismatch, route
    /// through the canonical name-keyed lib def resolution instead.
    pub fn lib_def_id_verified(&self, expected_name: &str, sym_id: SymbolId) -> DefId {
        let def_id = self.get_lib_def_id(sym_id);
        if self
            .definition_store
            .get(def_id)
            .is_some_and(|info| self.types.resolve_atom(info.name) == expected_name)
        {
            def_id
        } else {
            self.get_canonical_lib_def_id(expected_name, sym_id)
        }
    }

    /// Cache type parameters for a canonical lib symbol (without body registration).
    ///
    /// Combines [`get_canonical_lib_def_id`] + [`insert_def_type_params`] into a
    /// single call.  Used in `resolve_lib_type_with_params` where the type body
    /// is still being accumulated across multiple lib contexts and should not be
    /// registered in the type environments yet.
    ///
    /// Returns the `DefId` for subsequent use.
    pub fn cache_canonical_lib_type_params(
        &self,
        name: &str,
        per_lib_sym_id: SymbolId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) -> DefId {
        let def_id = self.get_canonical_lib_def_id(name, per_lib_sym_id);
        self.insert_def_type_params(def_id, params);
        def_id
    }

    /// Register a lib type's DefId, type parameters, and body in one step.
    ///
    /// Combines name-verified def resolution + `insert_def_type_params` +
    /// `register_def_auto_params_in_envs` into a single call, eliminating the
    /// repeated three-step pattern in `resolve_lib_type_by_name` (interface and
    /// type-alias branches) and `resolve_lib_type_with_params`.
    ///
    /// `expected_name` is the bare name the resolved type binds (for
    /// namespace-qualified lib types, the export name). The def resolution is
    /// name-verified because this function *publishes* `body` into the shared
    /// `DefinitionStore`: a raw-`SymbolId` resolution that collides across lib
    /// binders would publish the body onto an unrelated def (the misdirected
    /// write family the boxed-type identity fix closed).
    ///
    /// Returns the `DefId` for subsequent use (e.g., creating `Lazy(DefId)`).
    pub fn register_lib_def_resolved(
        &self,
        expected_name: &str,
        sym_id: SymbolId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) -> DefId {
        let def_id = self.get_or_create_def_id_for_symbol_name(sym_id, expected_name);
        self.insert_def_type_params(def_id, params.clone());
        self.register_def_auto_params_in_envs(def_id, body, params);

        // Mirror onto the canonical name-keyed def when it differs, but only
        // when the main binder's symbol at this raw id actually names
        // `expected_name` — otherwise the raw id belongs to an unrelated
        // main-binder symbol and the mirror would target a colliding def.
        if self
            .binder
            .get_symbol(sym_id)
            .is_some_and(|symbol| symbol.escaped_name == expected_name)
        {
            let canonical_def_id = self.get_canonical_lib_def_id(expected_name, sym_id);
            if canonical_def_id != def_id {
                let canonical_params = self.get_def_type_params(def_id).unwrap_or_default();
                self.insert_def_type_params(canonical_def_id, canonical_params.clone());
                self.register_def_auto_params_in_envs(canonical_def_id, body, canonical_params);
            }
        }

        def_id
    }

    /// Ensure both `TypeEnvironment` instances end up with a reference to the
    /// shared `DefinitionStore`, by writing it to the authoritative evaluator
    /// env and letting reconcile propagate it to the flow-analyzer env.
    ///
    /// `type_env` (primary evaluator) needs the `DefinitionStore` fallback so
    /// that `get_def_kind` can locate entries not written directly due to
    /// `RefCell` borrow conflicts during recursive resolution; it is written
    /// here (race-safe, via the same deferral discipline as every other
    /// evaluator-env registration).
    ///
    /// The flow-analyzer env (`type_environment`) receives the *same* `Arc`
    /// through `overlay_missing_from` at the file-prep reconcile boundary
    /// (`reconcile_flow_and_evaluator_envs`), which runs before flow analysis
    /// reads the env. The `DefinitionStore` is a single content-identical shared
    /// pointer regardless of which env holds it, so eagerly *mirroring* it into
    /// the flow env is pure redundancy with the reconcile vacancy-fill. Dropping
    /// that mirror removes the `SetDefinitionStore` op from the flow-env mirror
    /// set as the first provably zero-delta step of the dual-`TypeEnvironment`
    /// collapse (#14348) — the eager mirror added work (and a possible deferred
    /// replay) for a pointer reconcile already installs.
    ///
    /// `set_definition_store` is idempotent when the same `Arc` pointer is
    /// reinstalled (checked via `Arc::ptr_eq`), so calling this function
    /// multiple times across registration sites is safe.
    pub fn ensure_both_envs_have_definition_store(&self) {
        self.apply_to_eval_env(DeferredFlowEnvWrite::SetDefinitionStore(Arc::clone(
            &self.definition_store,
        )));
    }

    // ---- Dual-environment registration helpers ----
    //
    // `type_env` (primary evaluator env) and `type_environment` (flow-analyzer
    // snapshot) are separate `TypeEnvironment` instances.  When a definition or
    // class-instance type is registered, both must be updated so that narrowing
    // contexts and the evaluator see the same data.
    //
    // These helpers eliminate the duplicated `try_borrow_mut` blocks that were
    // scattered across lib resolution, symbol-type resolution, and augmentation
    // merge paths.
    //
    // **Borrow-conflict handling.** Both envs are owned through `RefCell`s, so a
    // registration can race with another live borrow during recursive
    // resolution (which can hold either, including `type_env` itself). Each env
    // uses the *same* deferral discipline: on a successful borrow, replay any
    // previously-deferred writes then apply `op`; on a borrow conflict, queue
    // `op` and replay it the next time the env is borrowable, so it is never
    // dropped. Dropping the authoritative `type_env` write (the previous "warn
    // and skip" behaviour) also dropped the shared-`DefinitionStore`
    // write-through that lives inside the env mutator, collapsing a
    // class-instance / def body to `never` for every later consumer.
    // The flow-env mirror was already deferred via `deferred_flow_env_writes`
    // (TODO #8269); the evaluator env now uses `deferred_eval_env_writes`.
    // See `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md` item 1 (PR #A).
    pub(super) fn register_in_envs(&self, op: DeferredFlowEnvWrite) {
        self.apply_to_eval_env(op.clone());
        self.mirror_to_flow_env(op);
    }

    /// Seed a definition body that is already authoritative in the shared
    /// `DefinitionStore` into the local evaluator env and flow-analyzer env.
    ///
    /// Shared-store warm-up may hold a long-lived mutable borrow of `type_env`
    /// while iterating symbols. When that borrow exists, apply the evaluator
    /// write directly through the same queued-write drain order; when it does
    /// not, route through the ordinary race-safe evaluator queue. In both cases
    /// the flow-analyzer env receives the exact same op through
    /// [`Self::mirror_to_flow_env`].
    pub(crate) fn seed_shared_store_def_in_envs(
        &self,
        eval_env: Option<&mut TypeEnvironment>,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        let op = DeferredFlowEnvWrite::insert_def_choosing_params(def_id, body, params, None);
        if let Some(env) = eval_env {
            drain_env_write_queue_into(&self.deferred_eval_env_writes, env);
            op.apply(env);
        } else {
            self.apply_to_eval_env(op.clone());
        }
        self.mirror_to_flow_env(op);
    }

    /// Apply (or defer) one registration to the authoritative evaluator env
    /// (`type_env`), through the shared race-safe write discipline.
    fn apply_to_eval_env(&self, op: DeferredFlowEnvWrite) {
        apply_or_defer_env_write(&self.type_env, &self.deferred_eval_env_writes, op);
    }

    /// Apply (or defer) a single registration to the flow-analyzer env
    /// (`type_environment`), through the shared race-safe write discipline.
    ///
    /// Exposed `pub(crate)` so the `get_type_of_symbol` caching path can mirror
    /// its `DefId` writes through this race-safe queue instead of a direct
    /// `try_borrow_mut` that silently drops on contention (#13086/#13944).
    pub(crate) fn mirror_to_flow_env(&self, op: DeferredFlowEnvWrite) {
        apply_or_defer_env_write(&self.type_environment, &self.deferred_flow_env_writes, op);
    }

    /// Replay any deferred flow-analyzer-env writes that lost the borrow race.
    ///
    /// Called at the file-preparation boundary (and reusable elsewhere) so the
    /// flow-analyzer env is a complete mirror of the evaluator env before flow
    /// analysis reads it. A no-op when nothing was deferred and when the env is
    /// momentarily unborrowable (the next successful mirror-write drains it).
    pub fn flush_deferred_flow_env_writes(&self) {
        flush_env_write_queue(&self.type_environment, &self.deferred_flow_env_writes);
    }

    /// Replay any deferred evaluator-env (`type_env`) writes that lost the
    /// borrow race during recursive resolution. Called at the file-preparation
    /// boundary, before the flow-analyzer env is reconciled from `type_env`, so
    /// the authoritative env holds every class-instance / def entry before any
    /// consumer (method-body checking, `overlay_missing_from`) reads it. A no-op
    /// when nothing was deferred or `type_env` is momentarily unborrowable.
    pub fn flush_deferred_eval_env_writes(&self) {
        flush_env_write_queue(&self.type_env, &self.deferred_eval_env_writes);
    }

    /// Number of registrations still waiting to be mirrored into the
    /// flow-analyzer env. Used by reconciliation assertions and tests.
    pub fn deferred_flow_env_write_count(&self) -> usize {
        self.deferred_flow_env_writes.borrow().len()
    }

    /// Number of authoritative registrations still waiting to be replayed into
    /// the evaluator env after losing the `type_env` borrow race. Used by tests.
    pub fn deferred_eval_env_write_count(&self) -> usize {
        self.deferred_eval_env_writes.borrow().len()
    }

    /// Record that `def_id` has been published with body `body`, returning
    /// whether that body was **already** in the def's published-body history.
    ///
    /// Used to detect the benign re-resolution oscillation where a generic
    /// alias re-lowers to one of a small set of structurally-equivalent interned
    /// bodies on every application. The history is tiny (bounded by the number
    /// of distinct equivalent re-lowerings, typically 1-2), so the linear scan
    /// over the `SmallVec` is effectively constant.
    fn record_published_body(&self, def_id: DefId, body: TypeId) -> bool {
        let mut map = self.def_published_bodies.borrow_mut();
        let history = map.entry(def_id).or_default();
        if history.contains(&body) {
            true
        } else {
            history.push(body);
            false
        }
    }

    /// Register a non-generic definition body in **both** type environments.
    #[track_caller]
    pub fn register_def_in_envs(&self, def_id: DefId, body: TypeId) {
        let prev_body = self.definition_store.get_body(def_id);
        let body_changed = prev_body != Some(body);
        // Skip the expensive env-eval cache sweep when a def re-publishes a body
        // it already held (a benign re-resolution oscillation). See
        // `def_published_bodies` and `register_def_with_params_in_envs`.
        let body_seen_before = body_changed && self.record_published_body(def_id, body);
        self.definition_store.set_body(def_id, body);
        if body_changed {
            // First publication (`None -> Some`) needs no env-eval/narrowing
            // sweep, for the same reason `invalidate_application_evals_on_body_rewrite`
            // skips it: no cached entry can reference `def_id` before it had a
            // resolvable body (the solver refuses to persist results computed
            // against an unresolved def, `mark_unresolved_def_seen`). Sweeping
            // on every first registration is `O(env_eval_cache)` per def, which
            // is `O(N^2)` across a file of `N` aliases whose closed-but-deferred
            // bodies (e.g. `Uppercase<"..">`) each seed one new cache entry. Gate
            // the sweep on a genuine rewrite (`Some(old) -> Some(new)`).
            if !body_seen_before && prev_body.is_some() {
                self.clear_type_evaluation_caches_for_def(def_id);
            }
            self.invalidate_application_evals_on_body_rewrite(def_id, prev_body);
        }
        self.register_in_envs(DeferredFlowEnvWrite::InsertDef { def_id, body });
    }

    /// Drop stale per-file application-eval entries when an already-published
    /// definition body is *rewritten* to different content.
    ///
    /// First publication (`None -> Some`) needs no sweep: the solver's
    /// evaluator refuses to persist application/closed-eval results computed
    /// while the def had no resolvable body (`mark_unresolved_def_seen`), so
    /// no entry derived from the body-less window can exist. Rewrites
    /// (`Some(old) -> Some(new)`, e.g. a partial pre-merge interface body
    /// upgraded to its heritage-merged form) do leave stale entries — those
    /// were legitimately cached against `old`. The sweep is def-keyed and
    /// rewrite-gated so the common first-registration path never pays the
    /// cache scan.
    fn invalidate_application_evals_on_body_rewrite(
        &self,
        def_id: DefId,
        prev_body: Option<TypeId>,
    ) {
        if prev_body.is_some() {
            self.types.invalidate_application_eval_cache_for_def(def_id);
        }
    }

    /// Register a generic definition body (with type parameters) in **both**
    /// type environments.
    ///
    /// Body and params are published to the shared `DefinitionStore` in one
    /// atomic write so concurrent readers never observe a generic alias whose
    /// body is visible but whose parameter list is still missing (which would
    /// mis-instantiate every application of the alias).
    #[track_caller]
    pub fn register_def_with_params_in_envs(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        let prev_body = self.definition_store.get_body(def_id);
        let body_changed = prev_body != Some(body);
        let params_changed = self
            .definition_store
            .get_type_params(def_id)
            .is_none_or(|existing| existing != params);
        // A generic alias re-resolves on every application; the re-lowering can
        // emit one of a few structurally-equivalent interned bodies, so
        // `body_changed` oscillates. Re-publishing a body the def already held
        // introduces no new staleness beyond the first occurrence, so the
        // expensive `O(env_eval_cache)` `clear_type_evaluation_caches_for_def`
        // sweep can be skipped for that benign rewrite (see
        // `def_published_bodies`). A genuinely new body, or any params change,
        // still sweeps. The cheap, def-keyed application-eval invalidation still
        // runs on every real body change.
        let body_seen_before = body_changed && self.record_published_body(def_id, body);
        self.definition_store
            .set_body_with_params(def_id, body, Some(params.clone()));
        // First publication (`prev_body == None`) needs no sweep: no cached
        // entry can reference `def_id` before it had a resolvable body (see
        // `register_def_in_envs` and `invalidate_application_evals_on_body_rewrite`).
        // A params-only change without a prior body cannot occur — params are
        // published atomically with the body — so gating on `prev_body.is_some()`
        // preserves every genuine-rewrite sweep while removing the per-first-
        // registration `O(env_eval_cache)` scan that is `O(N^2)` across a file.
        if prev_body.is_some() && ((body_changed && !body_seen_before) || params_changed) {
            self.clear_type_evaluation_caches_for_def(def_id);
        }
        if body_changed {
            self.invalidate_application_evals_on_body_rewrite(def_id, prev_body);
        }
        let declared_variances = TypeResolver::get_type_param_variance(self, def_id);
        self.register_in_envs(DeferredFlowEnvWrite::InsertDefWithParams {
            def_id,
            body,
            params,
            variances: declared_variances,
        });
    }

    /// Register a definition body in **both** type environments, choosing
    /// `insert_def` or `insert_def_with_params` based on whether `params` is
    /// empty.
    #[track_caller]
    pub fn register_def_auto_params_in_envs(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        if params.is_empty() {
            self.register_def_in_envs(def_id, body);
        } else {
            self.register_def_with_params_in_envs(def_id, body, params);
        }
    }

    /// Register a class instance type in **both** type environments.
    pub fn register_class_instance_in_envs(&self, def_id: DefId, instance_type: TypeId) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertClassInstance {
            def_id,
            instance_type,
        });
    }

    /// Register a class `extends` relationship in **both** type environments.
    ///
    /// This is required so the `FlowAnalyzer`'s `NarrowingContext` (which uses
    /// `type_environment`) can resolve nominal instanceof relationships just as
    /// the evaluator (`type_env`) can.  Without this, `is_class_ancestor` always
    /// returns `false` for user-defined class hierarchies during narrowing, causing
    /// `D1 & C1` intersections instead of the correct `D1` narrowed type.
    pub fn register_class_extends_in_envs(&self, def_id: DefId, parent_def_id: DefId) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterClassExtends {
            def_id,
            parent_def_id,
        });
    }

    /// Register a `DefId` ↔ `SymbolId` bridge in **both** type environments.
    ///
    /// This keeps evaluator and flow-analyzer resolution paths aligned for
    /// `TypeQuery`, inheritance, and solver-side DefId identity lookups.
    pub fn register_def_symbol_mapping_in_envs(&self, def_id: DefId, sym_id: SymbolId) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterDefSymbolMapping { def_id, sym_id });
    }

    /// Register a `DefId` ↔ `SymbolId` bridge in the flow-analyzer environment.
    ///
    /// `register_resolved_type` historically populated this bridge only in
    /// `type_environment`. Keep that path scoped so resolving a symbol's body
    /// does not also change evaluator-side TypeQuery/Lazy resolution order. On a
    /// borrow conflict the write is deferred and replayed rather than dropped.
    pub fn register_def_symbol_mapping_in_type_environment(&self, def_id: DefId, sym_id: SymbolId) {
        self.mirror_to_flow_env(DeferredFlowEnvWrite::RegisterDefSymbolMapping { def_id, sym_id });
    }

    /// Register an augmented definition body in **both** type environments.
    ///
    /// If the definition is a class (or already has a class-instance entry),
    /// updates the class-instance type. Otherwise, preserves existing type
    /// parameters (if any) when re-inserting the definition body.
    pub fn register_augmented_def_in_envs(&self, def_id: DefId, augmented: TypeId, is_class: bool) {
        self.register_in_envs(DeferredFlowEnvWrite::RegisterAugmentedDef {
            def_id,
            augmented,
            is_class,
        });
    }

    /// Register a `DefKind` for a `DefId` in **both** type environments.
    ///
    /// This ensures the evaluator (`type_env`) and flow-analyzer (`type_environment`)
    /// both see the `DefKind`, which is needed for `Lazy(DefId)` resolution and
    /// semantic queries (e.g., distinguishing class vs interface callables).
    ///
    /// Prior to this helper, pre-population and fallback paths only propagated
    /// `DefKind` to `type_env`, leaving `type_environment` without the mapping
    /// until the full checker walk populated it incidentally.
    pub(crate) fn register_def_kind_in_envs(&self, def_id: DefId, kind: tsz_solver::def::DefKind) {
        self.register_in_envs(DeferredFlowEnvWrite::InsertDefKind { def_id, kind });
    }

    /// Create a Lazy type reference from a symbol.
    ///
    /// This returns `TypeData::Lazy(DefId)` for use in the new `DefId` system.
    /// During migration, this is called alongside or instead of creating
    /// `TypeData::Ref(SymbolRef)`.
    pub fn create_lazy_type_ref(&mut self, sym_id: SymbolId) -> TypeId {
        let def_id = self.get_or_create_def_id(sym_id);
        self.types.lazy(def_id)
    }

    /// Look up the `SymbolId` for a `DefId` (reverse mapping).
    ///
    /// Uses a two-tier lookup strategy:
    /// 1. **Local cache** (`def_to_symbol`): O(1) `FxHashMap` lookup, no locking.
    /// 2. **Shared store** (`DefinitionStore`): O(1) `DashMap` lookup via
    ///    `get_symbol_id`. On hit, populates the local cache for future fast-path.
    ///
    /// This fallback ensures that `DefIds` created in child checker contexts
    /// (e.g., cross-file delegation) are visible to the parent without
    /// explicit merge-back of the `def_to_symbol` map.
    pub fn def_to_symbol_id(&self, def_id: DefId) -> Option<SymbolId> {
        // Fast path: local cache
        if let Some(sym_id) = self.def_to_symbol.borrow().get(&def_id).copied() {
            return Some(sym_id);
        }

        // Fallback: shared DefinitionStore (thread-safe, cross-context)
        if let Some(raw_sym_id) = self.definition_store.get_symbol_id(def_id) {
            let sym_id = SymbolId(raw_sym_id);
            // Populate local cache for future fast-path hits
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            self.symbol_to_def
                .borrow_mut()
                .entry(sym_id)
                .or_insert(def_id);
            return Some(sym_id);
        }

        None
    }

    /// Look up the `SymbolId` for a `DefId`, with fallback to the shared
    /// `DefinitionStore` for cross-context `DefIds`.
    ///
    /// **Deprecated**: `def_to_symbol_id()` now includes the `DefinitionStore`
    /// fallback directly. This method is retained for backward compatibility
    /// but delegates to `def_to_symbol_id()`.
    pub fn def_to_symbol_id_with_fallback(&self, def_id: DefId) -> Option<SymbolId> {
        self.def_to_symbol_id(def_id)
    }

    /// Look up the file index where a `DefId`'s definition was registered.
    pub fn def_file_idx(&self, def_id: DefId) -> Option<u32> {
        self.definition_store
            .get(def_id)
            .and_then(|info| info.file_id)
    }

    /// Resolve the binder symbol identity for a `DefId` without losing file/name identity.
    ///
    /// Raw `SymbolId`s are binder-local. A `DefId` carries the name and declaring
    /// file that disambiguate those raw ids, so DefId-based lazy resolution should
    /// consult that metadata before calling `get_type_of_symbol(SymbolId(...))`.
    pub fn def_symbol_identity(&self, def_id: DefId) -> Option<(SymbolId, Option<usize>)> {
        let info = self.definition_store.get(def_id)?;
        let expected_name = self.types.resolve_atom(info.name);
        let raw_sym_id = info.symbol_id.map(SymbolId);

        let valid_file_idx = |file_id: u32| (file_id != u32::MAX).then_some(file_id as usize);

        if let (Some(sym_id), Some(file_idx)) = (raw_sym_id, info.file_id.and_then(valid_file_idx))
            && self
                .get_binder_for_file(file_idx)
                .and_then(|binder| binder.get_symbol(sym_id))
                .is_some_and(|symbol| symbol.escaped_name == expected_name)
        {
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return Some((sym_id, Some(file_idx)));
        }

        if let Some(sym_id) = raw_sym_id {
            if self
                .binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.escaped_name == expected_name)
            {
                self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
                return Some((
                    sym_id,
                    (self.current_file_idx != usize::MAX).then_some(self.current_file_idx),
                ));
            }

            for lib_ctx in self.lib_contexts.iter() {
                if let Some(symbol) = lib_ctx.binder.get_symbol(sym_id)
                    && symbol.escaped_name == expected_name
                {
                    self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
                    return Some((sym_id, valid_file_idx(symbol.decl_file_idx)));
                }
            }

            if let Some(binders) = self.all_binders.as_ref() {
                for (file_idx, binder) in binders.iter().enumerate() {
                    if let Some(symbol) = binder.get_symbol(sym_id)
                        && symbol.escaped_name == expected_name
                    {
                        self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
                        return Some((sym_id, Some(file_idx)));
                    }
                }
            }
        }

        if let Some(sym_id) = self.binder.file_locals.get(expected_name.as_str())
            && self
                .binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.escaped_name == expected_name)
        {
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return Some((
                sym_id,
                (self.current_file_idx != usize::MAX).then_some(self.current_file_idx),
            ));
        }

        if let Some((file_idx, sym_id)) = self
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(expected_name.as_str()))
            .and_then(|entries| entries.iter().max_by_key(|(_, sym_id)| sym_id.0))
            .copied()
            && self
                .get_binder_for_file(file_idx)
                .and_then(|binder| binder.get_symbol(sym_id))
                .is_some_and(|symbol| symbol.escaped_name == expected_name)
        {
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return Some((sym_id, Some(file_idx)));
        }

        if let Some(binders) = self.all_binders.as_ref() {
            for (file_idx, binder) in binders.iter().enumerate() {
                if let Some(sym_id) = binder.file_locals.get(expected_name.as_str())
                    && binder
                        .get_symbol(sym_id)
                        .is_some_and(|symbol| symbol.escaped_name == expected_name)
                {
                    self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
                    return Some((sym_id, Some(file_idx)));
                }
            }
        }

        for lib_ctx in self.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(expected_name.as_str())
                && let Some(symbol) = lib_ctx.binder.get_symbol(sym_id)
                && symbol.escaped_name == expected_name
            {
                self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
                return Some((sym_id, valid_file_idx(symbol.decl_file_idx)));
            }
        }

        raw_sym_id.map(|sym_id| (sym_id, info.file_id.and_then(valid_file_idx)))
    }

    /// Get or create a `DefId` for a symbol and register its type parameters in one step.
    ///
    /// Consolidates the common two-step pattern of `get_or_create_def_id` +
    /// `insert_def_type_params` into a single call. Empty params are a no-op
    /// (just returns the DefId).
    pub fn get_or_create_def_id_with_params(
        &self,
        sym_id: SymbolId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) -> DefId {
        let def_id = self.get_or_create_def_id(sym_id);
        self.insert_def_type_params(def_id, params);
        def_id
    }

    /// Insert type parameters for a `DefId` (Phase 4.2.1: generic type alias support).
    ///
    /// This enables the Solver to expand Application(Lazy(DefId), Args) by providing
    /// the type parameters needed for generic substitution.
    ///
    /// # Example
    /// ```text
    /// // For type List<T> = { value: T; next: List<T> | null }
    /// let def_id = ctx.get_or_create_def_id(list_sym_id);
    /// let params = vec![TypeParamInfo { name: "T", ...,
    /// let params = vec![TypeParamInfo { name: "T", ... origin: tsz_solver::TypeParamOrigin::User,
    /// let params = vec![TypeParamInfo { name: "T", ... }];
    /// ctx.insert_def_type_params(def_id, params);
    /// ```
    pub fn insert_def_type_params(
        &self,
        def_id: DefId,
        mut params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        if params.is_empty() {
            return;
        }

        let existing = self
            .def_type_params
            .borrow()
            .get(&def_id)
            .cloned()
            .or_else(|| self.definition_store.get_type_params(def_id));
        if let Some(existing) = existing
            && existing.len() == params.len()
        {
            for (param, existing_param) in params.iter_mut().zip(existing) {
                if param.constraint.is_none() {
                    param.constraint = existing_param.constraint;
                }
                if param.default.is_none() {
                    param.default = existing_param.default;
                }
            }
        }

        // Sync type params into the DefinitionStore so the TypeFormatter
        // can display generic types with their type parameter names
        // (e.g., `MyClass<T>` instead of just `MyClass`).
        self.definition_store
            .set_type_params(def_id, params.clone());
        self.def_type_params.borrow_mut().insert(def_id, params);
    }

    /// Get type parameters for a `DefId`.
    ///
    /// Returns None if the `DefId` has no type parameters or hasn't been registered yet.
    /// Falls back to the shared `DefinitionStore` when the same interface has multiple
    /// `DefIds` (e.g., lib types like `PromiseLike` that get different `DefIds` in
    /// different contexts).
    pub fn get_def_type_params(&self, def_id: DefId) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        // ---- Step 1: local cache fast path ----
        let params = self.def_type_params.borrow();
        if let Some(result) = params.get(&def_id) {
            return Some(result.clone());
        }
        drop(params);

        // ---- Step 2: DefinitionStore direct lookup (O(1)) ----
        // The store has type params for this exact DefId if they were set via
        // insert_def_type_params (which calls definition_store.set_type_params).
        if let Some(store_params) = self.definition_store.get_type_params(def_id)
            && !store_params.is_empty()
        {
            self.def_type_params
                .borrow_mut()
                .insert(def_id, store_params.clone());
            return Some(store_params);
        }

        // ---- Step 3: cross-DefId fallback via SymbolId (O(1)) ----
        // Multiple DefIds can map to the same symbol when lib interfaces are
        // referenced from different checker contexts. Use the symbol_only_index
        // to find the canonical DefId and retrieve its type params.
        let sym_id = self.def_to_symbol.borrow().get(&def_id).copied()?;
        let requesting_def_name = self.definition_store.get(def_id).map(|info| info.name)?;
        let canonical_def_id = self.definition_store.find_def_by_symbol(sym_id.0)?;
        if canonical_def_id != def_id
            && self
                .definition_store
                .get(canonical_def_id)
                .is_some_and(|info| info.name == requesting_def_name)
            && let Some(canonical_params) = self.definition_store.get_type_params(canonical_def_id)
            && !canonical_params.is_empty()
        {
            // Cache for future lookups under the requesting DefId.
            self.def_type_params
                .borrow_mut()
                .insert(def_id, canonical_params.clone());
            return Some(canonical_params);
        }

        None
    }

    /// Resolve a `TypeId` to its underlying `SymbolId` if it is a reference type.
    ///
    /// This helper bridges the DefId-based Solver and SymbolId-based Binder.
    /// It handles the indirection automatically: `TypeId` → `DefId` → `SymbolId`.
    ///
    /// # Example
    /// ```text
    /// // Old (broken):
    /// if let Some(sym_ref) = get_ref_symbol(self.ctx.types, type_id) {
    ///     let sym_id = SymbolId(sym_ref.0); // BROKEN CAST
    /// }
    ///
    /// // New (correct):
    /// if let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(type_id) {
    ///     // use sym_id
    /// }
    /// ```
    pub fn resolve_type_to_symbol_id(&self, type_id: TypeId) -> Option<SymbolId> {
        // 0. Direct TypeQuery(typeof X) resolves to X's value symbol.
        if let crate::query_boundaries::common::TypeQueryKind::TypeQuery(sym_ref) =
            crate::query_boundaries::common::classify_type_query(self.types, type_id)
        {
            return Some(SymbolId(sym_ref.0));
        }

        // 1. Try to get DefId from Lazy type - Phase 4.2+
        // Use with_fallback because get_or_create_def_id can invalidate per-context
        // DefId→SymbolId mappings when the same symbol gets a new DefId (e.g., lib
        // types like Promise referenced multiple times). The DefinitionStore retains
        // the symbol_id even after the per-context map entry is removed.
        if let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.types, type_id) {
            return self.def_to_symbol_id_with_fallback(def_id);
        }

        // 2. Try to get DefId from Enum type
        if let Some(def_id) = crate::query_boundaries::common::enum_def_id(self.types, type_id) {
            return self.def_to_symbol_id(def_id);
        }

        // 3. Try to get SymbolId from ObjectShape (Object or ObjectWithIndex)
        if let Some(sym_id) = crate::query_boundaries::common::object_symbol(self.types, type_id) {
            return Some(sym_id);
        }

        None
    }

    /// Look up an existing `DefId` for a symbol without creating a new one.
    ///
    /// Returns None if the symbol doesn't have a `DefId` yet.
    /// This is used by the `DefId` resolver in `TypeLowering` to prefer
    /// `DefId` when available but fall back to `SymbolRef` otherwise.
    ///
    /// ## Lookup strategy
    ///
    /// 1. **Local cache** (`symbol_to_def`): O(1) `FxHashMap` lookup, no locking.
    /// 2. **Authoritative index** (`DefinitionStore::symbol_only_index`): O(1)
    ///    `DashMap` lookup. This catches `DefIds` created in other checker contexts
    ///    (e.g., cross-file references, lib types) that aren't yet in the local cache.
    ///    On a hit, the local caches are populated for future fast-path access.
    pub fn get_existing_def_id(&self, sym_id: SymbolId) -> Option<DefId> {
        let symbol_name = self
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.get_binder_for_file(file_idx))
            .and_then(|binder| binder.symbols.get(sym_id))
            .or_else(|| self.binder.symbols.get(sym_id))
            .or_else(|| {
                self.lib_contexts
                    .iter()
                    .find_map(|lib_ctx| lib_ctx.binder.symbols.get(sym_id))
            })
            .or_else(|| {
                self.all_binders.as_ref().and_then(|binders| {
                    binders.iter().find_map(|binder| binder.symbols.get(sym_id))
                })
            })
            .map(|symbol| symbol.escaped_name.clone());

        // Fast path: local cache
        if let Some(def_id) = self.symbol_to_def.borrow().get(&sym_id).copied() {
            let authoritative = self.resolve_symbol_file_index(sym_id).and_then(|file_idx| {
                self.get_binder_for_file(file_idx).and_then(|binder| {
                    binder.get_symbol(sym_id).and_then(|_| {
                        self.definition_store
                            .lookup_by_symbol(sym_id.0, file_idx as u32)
                    })
                })
            });
            let cached_matches_name = self.definition_store.get(def_id).is_some_and(|info| {
                symbol_name
                    .as_ref()
                    .is_some_and(|name| self.types.resolve_atom(info.name) == *name)
            });
            if cached_matches_name && authoritative.is_none_or(|auth| auth == def_id) {
                return Some(def_id);
            }
        }

        // Fallback: authoritative index (catches cross-context DefIds)
        let symbol = self
            .binder
            .symbols
            .get(sym_id)
            .or_else(|| {
                self.lib_contexts
                    .iter()
                    .find_map(|lib_ctx| lib_ctx.binder.symbols.get(sym_id))
            })
            .or_else(|| {
                self.all_binders.as_ref().and_then(|binders| {
                    binders.iter().find_map(|binder| binder.symbols.get(sym_id))
                })
            });
        if let Some(symbol) = symbol
            && let Some(def_id) = self.definition_store.lookup_by_symbol(
                sym_id.0,
                self.resolve_symbol_file_index(sym_id)
                    .map(|idx| idx as u32)
                    .unwrap_or(symbol.decl_file_idx),
            )
            && self.definition_store.get(def_id).is_some_and(|info| {
                symbol_name
                    .as_ref()
                    .is_some_and(|name| self.types.resolve_atom(info.name) == *name)
            })
        {
            // Populate local caches for future fast-path hits
            self.symbol_to_def.borrow_mut().insert(sym_id, def_id);
            self.def_to_symbol.borrow_mut().insert(def_id, sym_id);
            return Some(def_id);
        }

        None
    }

    /// Register a resolved type in the `TypeEnvironment` for both `SymbolRef` and `DefId`.
    ///
    /// This ensures that both the old `TypeData::Ref(SymbolRef)` and new `TypeData::Lazy(DefId)`
    /// paths can resolve the type during evaluation.
    ///
    /// The `SymbolRef` mapping is mirrored into `type_environment` through the
    /// deferred-write path (legacy flow-analyzer path). The DefId mapping is
    /// written to **both** environments via the dual-env helpers so the
    /// evaluator (`type_env`) and flow analyzer (`type_environment`) stay
    /// consistent.
    ///
    /// Should be called when a symbol's type is resolved via `get_type_of_symbol`.
    pub fn register_resolved_type(
        &self,
        sym_id: SymbolId,
        type_id: TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        use tsz_solver::SymbolRef;

        // Insert SymbolRef key into type_environment only (legacy path —
        // type_env never uses this resolved-type mirror). Route through the
        // deferred-write path so a live flow-analysis borrow queues the write
        // instead of dropping it, and preserve generic params on replay.
        self.mirror_symbol_type_in_type_environment(
            SymbolRef(sym_id.0),
            type_id,
            type_params.clone(),
        );

        // Insert DefId key into BOTH environments via dual-env helpers.
        // Previously this only wrote to type_environment, leaving type_env
        // without the DefId mapping — a consistency bug that could cause
        // resolve_lazy(DefId) to return None in the evaluator.
        if let Some(def_id) = self.get_existing_def_id(sym_id) {
            self.register_def_auto_params_in_envs(def_id, type_id, type_params);

            // Register mapping for InheritanceGraph bridge (Phase 3.2)
            // This enables Lazy(DefId) types to use the O(1) InheritanceGraph
            self.register_def_symbol_mapping_in_type_environment(def_id, sym_id);

            // Set the body on the DefinitionInfo so the type formatter can
            // find type alias names via find_type_alias_by_body(). Without
            // this, type aliases show their structural expansion in diagnostics
            // (e.g., "{ r: number; g: number; b: number }" instead of "Color").
            self.definition_store.set_body(def_id, type_id);
        }
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from the binder's
    /// `semantic_defs` index (Phase 1 DefId-first stable identity).
    ///
    /// Called once during checker construction so that `get_or_create_def_id`
    /// finds stable `DefIds` already present for top-level declarations. This
    /// moves identity creation to bind time (deterministic, early) rather than
    /// being recovered on-demand in hot checker paths (late, order-dependent).
    ///
    /// Returns the number of `DefIds` pre-populated.
    pub fn pre_populate_def_ids_from_binder(&self) -> usize {
        self.populate_def_ids_from_semantic_defs(&self.binder.semantic_defs)
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from all lib binders'
    /// `semantic_defs` indices.
    ///
    /// Lib binders contain definitions for standard library types (Array, Promise,
    /// Error, Map, etc.). Without this, every `get_or_create_def_id` call for a
    /// lib symbol falls through to the Step 3 O(N) `lib_contexts.iter()` scan to
    /// find the symbol and create its DefId on demand. By pre-populating here, these
    /// symbols hit the O(1) `find_def_by_symbol` path in Step 2 instead.
    ///
    /// Returns the total number of `DefIds` pre-populated across all lib binders.
    pub fn pre_populate_def_ids_from_lib_binders(&self) -> usize {
        let mut total = 0;
        for lib_ctx in self.lib_contexts.iter() {
            total += self.populate_def_ids_from_semantic_defs(&lib_ctx.binder.semantic_defs);
        }
        total
    }

    /// Pre-populate `symbol_to_def` and `def_to_symbol` from all cross-file
    /// binders' `semantic_defs` indices (multi-file stable identity).
    ///
    /// In multi-file compilation, each file has its own binder with its own
    /// `semantic_defs`. Without this, cross-file type references (e.g.,
    /// importing a class from another file) hit the O(N) `all_binders` scan
    /// in `get_or_create_def_id` Step 3 and create `DefIds` on demand.
    ///
    /// By pre-populating here, those `SymbolIds` are already registered in the
    /// `DefinitionStore`'s `symbol_only_index`, so `get_or_create_def_id`
    /// Step 2 finds them in O(1) without the repair path.
    ///
    /// Called from `ProgramContext::apply_to` after `set_all_binders`.
    /// Safe to overlap with `pre_populate_def_ids_from_binder` (the current
    /// file's binder may also appear in `all_binders`); the dedup check in
    /// `populate_def_ids_from_semantic_defs` skips already-registered entries.
    ///
    /// Returns the total number of new `DefIds` pre-populated.
    pub fn pre_populate_def_ids_from_all_binders(&self) -> usize {
        let Some(ref binders) = self.all_binders else {
            return 0;
        };
        let mut total = 0;
        for binder in binders.iter() {
            total += self.populate_def_ids_from_semantic_defs(&binder.semantic_defs);
        }
        total
    }

    /// Core helper: populate DefId mappings from a `semantic_defs` map.
    ///
    /// Used by both `pre_populate_def_ids_from_binder` (primary binder) and
    /// `pre_populate_def_ids_from_lib_binders` (lib binders). The logic is
    /// identical: convert `SemanticDefEntry` to `DefinitionInfo`, register in
    /// the `DefinitionStore`, and populate local caches.
    fn populate_def_ids_from_semantic_defs(
        &self,
        semantic_defs: &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
    ) -> usize {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        if semantic_defs.is_empty() {
            return 0;
        }

        // Sort by span_start so that DefIds are allocated in source declaration
        // order. FxHashMap iteration order is non-deterministic, which previously
        // caused type alias DefIds to be allocated out of source order. This broke
        // union member sorting (which compares Lazy(DefId) bases by DefId value)
        // and produced wrong display order in diagnostics (e.g., `Z | Y | X`
        // instead of `X | Y | Z`).
        let mut sorted_entries: Vec<_> = semantic_defs.iter().collect();
        sorted_entries.sort_by_key(|(_, entry)| entry.span_start);

        let mut count = 0;
        // Hold s2d/d2s borrows across the whole loop. Both register_def_kind_in_envs
        // and definition_store.register only touch type_env/type_environment and the
        // shared DashMap, never s2d/d2s. Saves 2 RefCell ops per inserted entry.
        let mut s2d = self.symbol_to_def.borrow_mut();
        let mut d2s = self.def_to_symbol.borrow_mut();
        for (&sym_id, entry) in sorted_entries {
            // Skip if already mapped (e.g., from a previous lib merge pass
            // or the primary binder's pre-population).
            if s2d.contains_key(&sym_id) {
                continue;
            }

            // Also skip if the DefinitionStore already has a mapping for this
            // symbol (e.g., from another lib binder that declared the same
            // global interface via declaration merging).
            if self.definition_store.find_def_by_symbol(sym_id.0).is_some() {
                continue;
            }

            let intern = |s: &str| self.types.intern_string(s);
            let info = DefinitionInfo::from_semantic_def(entry, sym_id.0, &intern);
            let kind = info.kind;

            let def_id = self.definition_store.register(info);
            trace!(
                symbol_name = %entry.name,
                symbol_id = %sym_id.0,
                def_id = %def_id.0,
                kind = ?kind,
                "Pre-populated DefId from semantic_defs"
            );

            // Register in the authoritative index so other checker contexts
            // can find this DefId via lookup_by_symbol() without creating
            // duplicates. This closes the gap where pre-populated DefIds
            // were only in the local cache but invisible to the shared store.
            self.definition_store
                .register_symbol_mapping(sym_id.0, entry.file_id, def_id);

            s2d.insert(sym_id, def_id);
            d2s.insert(def_id, sym_id);

            // Propagate DefKind to both TypeEnvironments (evaluator + flow-analyzer)
            self.register_def_kind_in_envs(def_id, kind);

            // For classes, create a ClassConstructor companion DefId so the
            // checker can reuse stable identity instead of creating one on demand.
            // The body is left empty (filled lazily during type checking).
            if kind == DefKind::Class {
                let ctor_info =
                    DefinitionInfo::class_constructor_from_semantic_def(entry, sym_id.0, &intern);
                let ctor_def_id = self.definition_store.register(ctor_info);
                self.definition_store
                    .register_constructor_companion(def_id, ctor_def_id);
                self.register_def_kind_in_envs(ctor_def_id, DefKind::ClassConstructor);
            }

            count += 1;
        }

        // Pass 2: Wire namespace exports from parent_namespace relationships.
        // After all DefIds are created/warmed, walk entries with parent_namespace
        // and register them as exports of their parent's DefinitionInfo.
        for (&sym_id, entry) in semantic_defs {
            if let Some(parent_sym) = entry.parent_namespace {
                let child_def = self.definition_store.find_def_by_symbol(sym_id.0);
                let parent_def = self.definition_store.find_def_by_symbol(parent_sym.0);
                if let (Some(child_def_id), Some(parent_def_id)) = (child_def, parent_def) {
                    let name = self.types.intern_string(&entry.name);
                    self.definition_store
                        .add_export(parent_def_id, name, child_def_id);
                }
            }
        }

        count
    }

    /// Warm local `symbol_to_def` / `def_to_symbol` caches from the shared
    /// `DefinitionStore` in a single pass.
    ///
    /// When the checker receives a pre-populated `DefinitionStore` from the
    /// merge pipeline (via `with_options_and_shared_def_store`), this method
    /// is more efficient than `pre_populate_def_ids_from_binder()` +
    /// `pre_populate_def_ids_from_lib_binders()` because it reads directly
    /// from the store's authoritative symbol→DefId index instead of
    /// re-iterating each binder's `semantic_defs` and re-converting
    /// `SemanticDefEntry` → `DefinitionInfo`.
    ///
    /// Also discovers `ClassConstructor` companion `DefId`s for any class `DefId`s
    /// encountered, ensuring constructor identity is warm from the start
    /// rather than being created on-demand in checker hot paths.
    ///
    /// Returns the number of mappings warmed.
    pub fn warm_local_caches_from_shared_store(&self) -> usize {
        // Fast path: skip if already warmed (avoids iterating all mappings a
        // second time when the constructor already called this method).
        if self.local_caches_warmed.get() {
            return 0;
        }

        if self.definition_store.is_empty() {
            return 0;
        }

        // Lazy-warm (default): the eager bulk copy below installs every
        // *cross-file* symbol of the whole program into this single file's
        // local `symbol_to_def` / `def_to_symbol` maps. The snapshot it reads
        // (`all_symbol_mappings_snapshot`) holds one entry per program symbol,
        // so the loop is O(program-symbols) per file and copies
        // O(program-symbols) entries into every per-file checker — an
        // O(files × program-symbols) time-and-memory term that rises with
        // total program size (the scale cliff).
        //
        // Every consumer of these local maps already has an authoritative
        // O(1) fallback into the shared store on a cache miss: cross-file
        // `SymbolId → DefId` resolves through `get_or_create_def_id` /
        // `symbol_to_def_id` (`DefinitionStore::lookup_by_symbol`), and the
        // reverse `DefId → SymbolId` resolves through `def_to_symbol_id`
        // (`DefinitionStore::get_symbol_id`). Skipping the eager copy makes
        // each file populate only the cross-file symbols it actually
        // references, on demand, restoring per-file cost that is independent
        // of total program size. Set `TSZ_EAGER_WARM_LOCAL_CACHES=1` to
        // restore the legacy eager bulk copy (rollback escape hatch).
        if !eager_warm_local_caches() {
            self.local_caches_warmed.set(true);
            return 0;
        }

        let mappings = self.definition_store.all_symbol_mappings_snapshot();
        let mut count = 0;

        // Hold both RefCell borrows for the entire loop. The previous
        // implementation re-borrowed `symbol_to_def` and `def_to_symbol`
        // on every iteration (one borrow + two borrow_mut per mapping),
        // which adds ~3 RefCell ops per entry. With 10k+ mappings on
        // large repos this is measurable. The body never recurses into
        // these RefCells, so a single mut borrow is safe.
        let symbols = self.binder.get_symbols();
        let mut s2d = self.symbol_to_def.borrow_mut();
        let mut d2s = self.def_to_symbol.borrow_mut();

        // Pre-size to avoid rehashing during bulk insertion.
        if mappings.len() > s2d.len() {
            let additional = mappings.len() - s2d.len();
            s2d.reserve(additional);
            d2s.reserve(additional);
        }

        for &(raw_sym_id, def_id) in mappings.iter() {
            let sym_id = tsz_binder::SymbolId(raw_sym_id);

            // Skip if already in local cache (e.g., from a prior warm pass).
            if s2d.contains_key(&sym_id) {
                continue;
            }

            // Skip if the current file's binder owns a symbol at this SymbolId.
            // The shared store's `symbol_only_index` is file-agnostic: the same
            // raw `SymbolId(u32)` from different files maps to whichever DefId
            // was registered first.  Installing such a mapping would pollute this
            // file's local caches with a DefId that belongs to a *different*
            // file's symbol — for example, File A's `Set` (SymbolId 5) would
            // shadow File B's `AtTop` (also SymbolId 5) during instanceof
            // narrowing.  Skipping here is safe: when the local symbol is
            // actually referenced, `get_or_create_def_id` will resolve it
            // correctly through the file-aware `symbol_def_index`.
            if symbols.get(sym_id).is_some() {
                continue;
            }

            s2d.insert(sym_id, def_id);
            d2s.insert(def_id, sym_id);

            // NOTE: DefKind registration is intentionally skipped here.
            // The TypeEnvironment is rebuilt from scratch in build_type_environment()
            // (called later in check_source_file), and ensure_both_envs_have_definition_store()
            // installs the DefinitionStore reference into both TypeEnvironments for lazy DefKind fallback.
            // Eagerly registering DefKinds here would be overwritten and wastes
            // N DashMap lookups per symbol (for .get() and .get_constructor_def()).

            count += 1;
        }
        drop(s2d);
        drop(d2s);

        trace!(
            count,
            total_mappings = mappings.len(),
            "Warmed local caches from shared DefinitionStore"
        );

        self.local_caches_warmed.set(true);
        count
    }

    /// Returns `true` if the shared `DefinitionStore` has been pre-populated
    /// (i.e., it contains definitions registered at merge time, not just an
    /// empty store created by the default constructor).
    ///
    /// When true, `warm_local_caches_from_shared_store()` can replace the
    /// more expensive `pre_populate_def_ids_from_binder()` +
    /// `pre_populate_def_ids_from_lib_binders()` calls.
    pub fn has_shared_store(&self) -> bool {
        !self.definition_store.is_empty()
    }

    /// Resolve heritage for definitions whose extends/implements targets were
    /// not found during their batch's pass 2 (cross-batch heritage).
    ///
    /// This handles the common case where a user class extends a lib type
    /// (e.g., `class MyError extends Error`): when `pre_populate_def_ids_from_binder`
    /// processes the user file, the lib type's `DefId` hasn't been registered yet
    /// (lib binders are pre-populated separately). After ALL pre-population batches
    /// complete, this method resolves the remaining heritage using the
    /// `DefinitionStore`'s name index, which now contains entries from all batches.
    ///
    /// Called once during checker construction after all `pre_populate_*` methods.
    /// Returns the number of heritage links resolved.
    pub fn resolve_cross_batch_heritage(&self) -> usize {
        use tsz_solver::def::DefKind;

        let mut resolved_count = 0;

        // Collect all semantic_defs from all sources (primary binder + all_binders).
        // The shared DefinitionStore's name_to_defs index is already populated from
        // all pre-population batches, so name-based lookups will find targets from
        // any batch (user files, lib files, cross-file binders).
        let sources: Vec<
            &rustc_hash::FxHashMap<tsz_binder::SymbolId, tsz_binder::SemanticDefEntry>,
        > = {
            // `&*x.semantic_defs` dereferences the `Arc<FxHashMap<...>>` so the
            // resulting reference targets the underlying map (the type the Vec
            // expects), not the Arc wrapper.
            let mut v = vec![&*self.binder.semantic_defs];
            for lib_ctx in self.lib_contexts.iter() {
                v.push(&*lib_ctx.binder.semantic_defs);
            }
            if let Some(ref binders) = self.all_binders {
                for binder in binders.iter() {
                    v.push(&*binder.semantic_defs);
                }
            }
            v
        };

        for source in &sources {
            for (&sym_id, entry) in *source {
                let def_id = match self.definition_store.find_def_by_symbol(sym_id.0) {
                    Some(id) => id,
                    None => continue,
                };

                // Skip if extends is already wired (from pre-populate Pass 3)
                if let Some(info) = self.definition_store.get(def_id)
                    && info.extends.is_some()
                {
                    continue;
                }

                // Resolve extends_names → extends
                for name_str in &entry.extends_names {
                    if name_str.contains('.') {
                        continue;
                    }
                    let name_atom = self.types.intern_string(name_str);
                    if let Some(candidates) = self.definition_store.find_defs_by_name(name_atom) {
                        for &candidate_id in &candidates {
                            if candidate_id == def_id {
                                continue;
                            }
                            if let Some(info) = self.definition_store.get(candidate_id)
                                && matches!(info.kind, DefKind::Class | DefKind::Interface)
                            {
                                self.definition_store.set_extends(def_id, candidate_id);
                                resolved_count += 1;
                                break;
                            }
                        }
                    }
                    break; // only first extends name
                }

                // Resolve implements_names → implements
                if !entry.implements_names.is_empty() {
                    let mut resolved = Vec::new();
                    for name_str in &entry.implements_names {
                        if name_str.contains('.') {
                            continue;
                        }
                        let name_atom = self.types.intern_string(name_str);
                        if let Some(candidates) = self.definition_store.find_defs_by_name(name_atom)
                        {
                            for &candidate_id in &candidates {
                                if candidate_id == def_id {
                                    continue;
                                }
                                if let Some(info) = self.definition_store.get(candidate_id)
                                    && matches!(info.kind, DefKind::Interface | DefKind::Class)
                                {
                                    resolved.push(candidate_id);
                                    break;
                                }
                            }
                        }
                    }
                    if !resolved.is_empty() {
                        self.definition_store
                            .set_implements(def_id, resolved.clone());
                        resolved_count += resolved.len();
                    }
                }
            }
        }

        resolved_count
    }
}

#[cfg(test)]
#[path = "def_mapping_tests.rs"]
mod tests;
