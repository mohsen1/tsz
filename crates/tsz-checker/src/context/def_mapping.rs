//! DefId migration helpers for `CheckerContext`.
//!
//! Handles bidirectional mapping between `SymbolId` and `DefId`, lazy type
//! references, type parameter registration, and resolved-type registration
//! in the `TypeEnvironment`.

mod body_publication;
mod env_registration;
mod prepopulation;

use std::sync::Arc;

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
            && symbol_name
                .as_ref()
                .is_some_and(|name| self.def_name_matches(def_id, name).unwrap_or(false))
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
        // across all binders — except under the `u32::MAX` declaration-file
        // sentinel that every lib-binder symbol shares. There a raw-id
        // collision across binders (e.g. a lib-local id inside a delegated
        // child checker vs the program binder's merged id space) can hit an
        // unrelated def, so validate by name exactly like Step 2; a mismatch
        // falls through to Step 4, whose decl-site guard resolves collisions
        // (issue #15687).
        if let Some(def_id) = self.definition_store.lookup_by_symbol(sym_id.0, file_idx)
            && (file_idx != u32::MAX
                || self
                    .def_name_matches(def_id, &symbol.escaped_name)
                    .unwrap_or(false))
        {
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

        // A checker whose current arena IS a builtin lib file (the lib-baseline
        // diagnostics passes) re-binds that lib's own declarations under its
        // LOCAL file index, so the `is_current_file_local_symbol` attribution
        // above yields a per-pass file id for a lib type that already has a
        // pre-populated `u32::MAX`-sentinel def in the shared store. Minting
        // there creates a SECOND identity for the lib type; member references
        // baked into shared bases (e.g. `Array.prototype.flatMap`'s
        // `ReadonlyArray<U>`) then bind to whichever copy an election sees,
        // and the split breaks Application evaluation downstream (witness:
        // `[1].flatMap(x => [[x]])` inferring `number[][][]` under a user
        // lib-global augmentation because `ReadonlyArray<U>` stopped
        // matching). Adopt the pre-populated sentinel def instead.
        if crate::state_type_analysis::cross_file_direct::is_builtin_lib_declaration_arena(
            self.arena,
        ) {
            let atom = self.types.intern_string(expected_name);
            if let Some(defs) = self.definition_store.find_defs_by_name(atom)
                && let Some(&sentinel_def) = defs.iter().find(|&&candidate| {
                    self.definition_store.get(candidate).is_some_and(|info| {
                        info.file_id == Some(u32::MAX) && info.symbol_id == Some(sym_id.0)
                    })
                })
            {
                self.symbol_to_def.borrow_mut().insert(sym_id, sentinel_def);
                self.def_to_symbol.borrow_mut().insert(sentinel_def, sym_id);
                return sentinel_def;
            }
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
    ///
    /// Resolution is keyed on `name` through the collision-free
    /// `DefinitionStore` name index, NOT on the binder-relative
    /// `canonical_sym`. Every lib-binder symbol shares the `u32::MAX`
    /// declaration-file sentinel, so a raw `SymbolId -> DefId` lookup
    /// ([`get_lib_def_id`]) can answer with a def a *different* lib binder
    /// registered for an unrelated name whose raw index collides. That misfires
    /// when a canonical symbol is chosen from a merged/global index — e.g. after
    /// a user `interface Error {}` merges into the lib `Error`, hoisting lib
    /// globals into the primary binder's `file_locals` so
    /// [`canonical_lib_sym_id`] returns a merged identity that differs from the
    /// per-lib input — producing `FlatArray -> eval`, `ReadonlyArray -> isNaN`
    /// inside re-lowered lib member signatures. The name index is keyed on the
    /// interned name, so it resolves the intended lib def regardless of which
    /// binder the canonical `SymbolId` is relative to. The raw
    /// `get_lib_def_id(canonical_sym)` remains the fallback for names not yet
    /// registered under a lib def in the store.
    pub fn get_canonical_lib_def_id(&self, name: &str, per_lib_sym_id: SymbolId) -> DefId {
        let canonical_sym = self.canonical_lib_sym_id(name, per_lib_sym_id);
        // When the canonical symbol DIFFERS from the per-lib input, the merged
        // identity is authoritative and `SymbolId`-keyed: resolve through it
        // directly. Electing from the name index here instead can return a
        // sibling same-named def that the `SymbolId`-keyed paths
        // (`get_or_create_def_id`, semantic-def prepopulation, env
        // publication) do not use, splitting one lib type into two identities
        // (witness: `Parameters`/`ReturnType` utilities diverging until
        // `Plugin[]` failed against `Plugin[]` in
        // thislessFunctionsNotContextSensitive3). The interface-merge
        // collision family a name-index election here once compensated for
        // (`FlatArray -> alert`/`eval`) is owned upstream: lib member refs
        // lower name-first unconditionally and identity-verified def writes
        // keep collided ids from publishing (see `queries/lib*.rs` and
        // `insert_type_env_symbol`).
        if canonical_sym != per_lib_sym_id {
            let via_canonical = self.get_lib_def_id(canonical_sym);
            // The canonical symbol can come from the largest-`SymbolId`
            // HEURISTIC in `canonical_lib_sym_id` (ROBUSTNESS_AUDIT item #4):
            // a raw id picked by name in ONE binder's numbering and resolved
            // by `get_lib_def_id` against ANOTHER's. When the mapped def's
            // recorded name CONTRADICTS the requested name, the identity is a
            // collision artifact — a lib alias body ref to
            // `XMLHttpRequestBodyInit` elected `AudioSampleFormat`'s def and
            // baked its literal union into `BodyInit`, the ofetch canary
            // false TS2322 (#15778). Fall through to the name-verified
            // election below instead of committing the collided identity.
            // A def with no recorded info keeps the fast path (fresh sentinel
            // mints have no name to contradict).
            let recorded_name_contradicts = self
                .definition_store
                .get(via_canonical)
                .is_some_and(|info| &*self.types.resolve_atom_ref(info.name) != name);
            if !recorded_name_contradicts {
                return via_canonical;
            }
        }
        let atom = self.types.intern_string(name);
        self.definition_store
            .find_defs_by_name(atom)
            .and_then(|defs| {
                defs.into_iter()
                    .filter(|def_id| {
                        self.definition_store.get(*def_id).is_some_and(|info| {
                            // Lib provenance is recognized two ways: the def's
                            // symbol resolves as a lib symbol in this binder
                            // context, OR the def carries the `u32::MAX`
                            // lib declaration-file sentinel — the form the
                            // parallel driver's composed semantic-def store
                            // produces (its symbol_id is a merged-binder id
                            // that `symbol_is_from_actual_or_cloned_lib`
                            // cannot recognize here; rejecting it sent lib
                            // type refs like `FlatArray` through the raw
                            // fallback, which collides under an interface
                            // merge into a lib global: FlatArray -> alert).
                            (info.symbol_id.is_some_and(|sym_id| {
                                self.symbol_is_from_actual_or_cloned_lib(SymbolId(sym_id))
                            }) || info.file_id == Some(u32::MAX))
                                && matches!(
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
                        // Election among same-named lib defs, in precedence order:
                        //
                        // 1. `canonical_match`: the def whose recorded `symbol_id`
                        //    IS the canonical merged symbol. That is the def every
                        //    `SymbolId`-keyed path (`get_or_create_def_id`,
                        //    semantic-def prepopulation, env publication) uses;
                        //    electing any sibling here splits one lib type into
                        //    two identities (witness: `Parameters`/`ReturnType`
                        //    utilities diverging until `Plugin[]` failed against
                        //    `Plugin[]` in thislessFunctionsNotContextSensitive3).
                        //
                        // 2. `name_verified`: a user `interface` merging into a
                        //    lib global can leave a lib-named def whose recorded
                        //    `symbol_id` is a merged-global id aliasing an
                        //    UNRELATED lib symbol (a `FlatArray` def colliding
                        //    with `alert`/`eval`). Preferring the raw-highest
                        //    `symbol_id` let that duplicate outbid the genuine
                        //    def; require the `symbol_id` to resolve to a symbol
                        //    actually NAMED `name`.
                        //
                        // 3. Historical highest-`symbol_id` tiebreak.
                        //
                        // 4. Deterministic authoritative-identity tiebreak
                        //    (#16309 evidence #1/#2). Under parallel fresh
                        //    checking a per-file checker can mint an
                        //    intermediate, pre-heritage-merge def for the SAME
                        //    lib symbol (same `symbol_id`) as the pre-populated
                        //    `u32::MAX`-sentinel def (the materialized
                        //    heritage-merged identity). Those defs tie on keys
                        //    1-3, so `max_by_key` fell through to the
                        //    `find_defs_by_name` push order, i.e. whichever
                        //    thread registered last: the resolved
                        //    `DecoratorContext` body one run, an unresolved
                        //    placeholder (`["kind"]` collapsing to `undefined`)
                        //    the next. Prefer the sentinel — a FIXED structural
                        //    witness (`file_id`), unlike body-materialization
                        //    state, which is itself thread-timing dependent and
                        //    so cannot key a deterministic order. Fires only
                        //    when keys 1-3 already tie (same-symbol duplicates),
                        //    which are otherwise content-equivalent, so
                        //    single-threaded / non-duplicated election is
                        //    unchanged.
                        let sym_id = self.definition_store.get_symbol_id(*def_id).map(SymbolId);
                        let is_sentinel = self.definition_store.def_is_non_program(*def_id);
                        let canonical_match = sym_id == Some(canonical_sym);
                        let name_verified = sym_id.is_some_and(|sym| {
                            self.binder
                                .get_symbol(sym)
                                .is_some_and(|s| s.escaped_name.as_str() == name)
                                || self.lib_contexts.iter().any(|lib_ctx| {
                                    lib_ctx
                                        .binder
                                        .get_symbol(sym)
                                        .is_some_and(|s| s.escaped_name.as_str() == name)
                                })
                        });
                        (
                            canonical_match,
                            name_verified,
                            sym_id.map(|s| s.0).unwrap_or_default(),
                            is_sentinel,
                        )
                    })
            })
            .unwrap_or_else(|| self.get_lib_def_id(canonical_sym))
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

    /// Wire the shared `DefinitionStore` fallback into both `TypeEnvironment`
    /// instances through the same race-safe deferred-write authority as ordinary
    /// env registrations.
    pub fn ensure_both_envs_have_definition_store(&self) {
        self.register_in_envs(DeferredFlowEnvWrite::SetDefinitionStore(Arc::clone(
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
    /// boundary, before the flow-analyzer env is checked against `type_env`, so
    /// the authoritative env holds every class-instance / def entry before any
    /// consumer (method-body checking, flow-env assertions) reads it. A no-op
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
            return Some(
                crate::query_boundaries::definition_identity::symbol_ref_to_symbol_id(sym_ref),
            );
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
            self.publish_definition_body(def_id, type_id);
        }
    }
}

#[cfg(test)]
#[path = "def_mapping_tests.rs"]
mod tests;
