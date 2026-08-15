//! Cross-file symbol resolution: resolving symbols across multiple files,
//! delegating type resolution to child checkers, tracking cross-file targets,
//! and cross-file interface declaration merging.
use crate::state::CheckerState;
use crate::state_type_analysis::cross_file_direct::is_direct_lowering_source_file_arena;
use crate::symbols_domain::name_text::expression_name_text_in_arena;
use crate::types_domain::queries::lib_resolution::keyword_syntax_to_type_id;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::perf_counters::{CrossArenaSymbolMissKind, CrossArenaSymbolMissSource};
use tsz_parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

mod miss_kind;

pub(crate) use super::cross_file_query_types::CrossFileQueryKind;

impl CheckerState<'_> {
    fn resolve_cross_file_heritage_type_arg(
        &mut self,
        arena: &tsz_parser::NodeArena,
        node_idx: NodeIndex,
    ) -> TypeId {
        let Some(node) = arena.get(node_idx) else {
            return TypeId::UNKNOWN;
        };

        if let Some(builtin) = keyword_syntax_to_type_id(node.kind) {
            return builtin;
        }

        let name = if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            arena
                .get_type_ref(node)
                .and_then(|type_ref| expression_name_text_in_arena(arena, type_ref.type_name))
        } else {
            expression_name_text_in_arena(arena, node_idx)
        };

        let Some(name) = name else {
            return TypeId::UNKNOWN;
        };
        if name == "BuiltinIteratorReturn" {
            return self.builtin_iterator_return_intrinsic_type();
        }
        if let Some(&type_id) = self.ctx.type_parameter_scope.get(&name) {
            return type_id;
        }
        if let Some(sym_id) = self.resolve_cross_file_global_type_symbol(&name) {
            return self.get_type_of_symbol(sym_id);
        }

        let atom = self.ctx.types.intern_string(&name);
        self.ctx.types.type_param(tsz_solver::TypeParamInfo {
            name: atom,
            constraint: None,
            default: None,
            is_const: false,
            origin: tsz_solver::TypeParamOrigin::User,
        })
    }

    /// Get a symbol from the current binder, lib binders, or other file binders.
    /// This ensures we can resolve symbols from lib.d.ts and other files.
    ///
    /// Import aliases are pinned locally (see `cross_file_import_alias_pin`)
    /// before the cross-file overlay is consulted; raw `SymbolId`s are
    /// file-local and the overlay would otherwise substitute an unrelated
    /// same-id decl from the source module.
    pub(crate) fn get_symbol_globally(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        if let Some(alias) = self.local_import_alias(sym_id) {
            return Some(alias);
        }
        // NOTE: this read deliberately keeps the dynamic-overlay-first resolver.
        // For a re-exported import-alias `SymbolId`, the followed chain endpoint
        // is recorded only in the dynamic `cross_file_symbol_targets` overlay
        // (`resolve_import_alias_chain_and_register`); the immutable
        // `global_symbol_file_index` declaring index points at the alias's own
        // binding file, not the terminal target. Preferring the declaring index
        // here breaks re-exported `unique symbol` / aliased member resolution
        // (regression witnessed by `reexported_symbol_keyed_member_tests`). The
        // #13255 stabilization is applied at the delegation DECISION + cache-KEY
        // sites below, not at the symbol-flags read.
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && file_idx != self.ctx.current_file_idx
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }

        // 1. Check current file
        if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
            return Some(sym);
        }
        // 2. Check lib files (lib.d.ts, etc.)
        for lib in self.ctx.lib_contexts.iter() {
            if let Some(sym) = lib.binder.get_symbol(sym_id) {
                return Some(sym);
            }
        }
        // 3. O(1) fast-path: if this SymbolId was already resolved to a specific
        //    file via the resolver, go directly to that binder. (Dynamic-first:
        //    see the note above on re-exported alias chains.)
        {
            let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
            if let Some(file_idx) = file_idx
                && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                && let Some(sym) = binder.get_symbol(sym_id)
            {
                return Some(sym);
            }
        }
        // 4. Fallback: O(N) scan over all binders
        if let Some(binders) = &self.ctx.all_binders {
            for binder in binders.iter() {
                if let Some(sym) = binder.get_symbol(sym_id) {
                    return Some(sym);
                }
            }
        }
        None
    }

    /// Fetch the declaration data of a *resolved import target* `sym_id` from
    /// the file that actually declares it, bypassing the local-import-alias pin.
    ///
    /// `get_symbol_globally` / `get_cross_file_symbol` deliberately pin a raw
    /// `SymbolId` to a local `import ... from "./m"` alias when one exists,
    /// because per-file binders mint colliding raw ids (no `base_offset` in
    /// production) and a blind cross-file lookup could pick up an unrelated
    /// same-id decl. That pin is correct while resolving the *alias itself*, but
    /// wrong once the alias has already been followed to its target via
    /// `resolve_import_alias_and_register` and the target's raw id happens to
    /// collide with the local alias: there we must read the *target's* real
    /// flags/declarations from its owning binder, not the alias's. Falls back to
    /// `get_symbol_globally` when the target has no distinct owning file
    /// (same-file targets, libs).
    pub(crate) fn resolved_import_target_symbol(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        // Dynamic-overlay-first: this reads a *resolved import target*, whose
        // owner is recorded in the overlay by the chain resolver; see the note
        // in `get_symbol_globally`.
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && file_idx != self.ctx.current_file_idx
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }
        self.get_symbol_globally(sym_id)
    }

    /// Get a symbol, preferring the cross-file binder for known cross-file `SymbolIds`.
    /// Resolves through `cross_file_symbol_targets` first to avoid same-raw-id
    /// collisions with the local binder; import aliases are pinned local (see
    /// `cross_file_import_alias_pin`).
    pub(crate) fn get_cross_file_symbol(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        if let Some(alias) = self.local_import_alias(sym_id) {
            return Some(alias);
        }
        // Dynamic-overlay-first: see the note in `get_symbol_globally` on
        // re-exported alias chains.
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }
        self.get_symbol_globally(sym_id)
    }

    /// Order-independent hash of the in-progress resolution sets a delegated
    /// computation's cycle detection can observe. Used as the context
    /// component of the cross-arena sentinel memo key: a completed sentinel
    /// is replayable only under the identical in-progress context.
    fn cross_arena_context_fingerprint(&self) -> u64 {
        #[inline]
        const fn mix(value: u64) -> u64 {
            // splitmix64 finalizer
            let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        let mut acc: u64 = 0;
        for s in &self.ctx.symbol_resolution_set {
            acc ^= mix(u64::from(s.0));
        }
        for s in &self.ctx.class_instance_resolution_set {
            acc ^= mix(u64::from(s.0) | (1 << 40));
        }
        for s in &self.ctx.class_constructor_resolution_set {
            acc ^= mix(u64::from(s.0) | (1 << 41));
        }
        acc
    }

    /// `true` when `symbol` has at least one `interface` declaration that
    /// resolves in the *current* arena. Used to keep a locally-declared
    /// interface on the local declaration-merging path instead of delegating its
    /// type computation to a foreign arena.
    fn symbol_has_local_interface_declaration(&self, symbol: &tsz_binder::Symbol) -> bool {
        symbol.declarations.iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .and_then(|node| self.ctx.arena.get_interface(node))
                .is_some()
        })
    }

    /// Delegate symbol resolution to a checker using the correct arena.
    ///
    /// When a symbol's arena differs from the current arena (cross-file symbol),
    /// we create a child checker with the correct arena and delegate the resolution.
    /// This ensures symbols are resolved in their original context.
    pub(crate) fn delegate_cross_arena_symbol_resolution(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        // TYPE_ALIAS + value merge fix: When a user-defined type alias (e.g., `type Proxy<T>`)
        // has the same name as a global value (`declare var Proxy: ProxyConstructor`), the
        // merged symbol has both TYPE_ALIAS and value flags, and symbol_arenas may point to
        // the lib arena. Delegating to the lib arena loses the type alias declaration (which
        // lives in the user arena), causing property access on the instantiated type to fail.
        // If the type alias declaration exists in the current arena, handle it locally.
        //
        // Resolved once and reused across the read-only guard blocks below (the first
        // guard already required it unconditionally): avoids re-running the alias-pin +
        // `resolve_symbol_file_index` + binder lookups (and the fallback O(N) binder
        // scan) several times per delegation. Byte-identical — no guard mutates state.
        let cross_file_symbol = self.get_cross_file_symbol(sym_id);
        let requester_default_import_cache_file_idx = self
            .local_import_alias(sym_id)
            .filter(|symbol| symbol.import_name() == Some("default"))
            .map(|_| self.ctx.current_file_idx);
        {
            let sym_found = cross_file_symbol;
            let has_type_alias =
                sym_found.is_some_and(|s| s.has_any_flags(symbol_flags::TYPE_ALIAS));
            if has_type_alias {
                let symbol = sym_found.expect("has_type_alias guard ensures sym_found is Some");
                // A cross-file alias can collide with an identically-shaped local
                // alias on raw SymbolId/NodeIndex; the helper confirms genuine
                // current-file ownership before we resolve it locally.
                let has_type_alias_in_current_arena =
                    self.symbol_has_local_type_alias_declaration(symbol, sym_id);
                tracing::debug!(
                    sym_id = sym_id.0,
                    name = %symbol.escaped_name,
                    has_type_alias_in_current_arena,
                    "delegate_cross_arena: TYPE_ALIAS check result"
                );
                if has_type_alias_in_current_arena {
                    return None; // Handle locally, don't delegate to lib arena
                }
            }
        }
        // CLASS + cross-file merge fix: When a class declaration exists in the current
        // arena but the merged symbol also has declarations in another file (e.g., a JS
        // constructor function `var Foo = function(){}` in file1.js merged with
        // `class Foo {}` in file2.js), delegating to the other file's arena would cause
        // compute_class_symbol_type to fail to find the class node and return UNKNOWN,
        // triggering false TS18046 errors. Handle the class locally instead.
        //
        // A declaration `NodeIndex` is arena-relative. A foreign class can have the
        // same raw index as an unrelated class in the requesting file, so seeing class
        // syntax at that index does not prove this symbol has a local declaration. As
        // with the FUNCTION guard below, require the current binder's node-to-symbol
        // map to round-trip to `sym_id`. This keeps the local merge fast path O(1) while
        // allowing genuine foreign classes to delegate to their owning arena.
        {
            let sym_found = cross_file_symbol;
            if let Some(symbol) = sym_found
                && symbol.has_any_flags(symbol_flags::CLASS)
            {
                let has_class_in_current_arena = symbol.declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| self.ctx.arena.get_class(n))
                        .is_some()
                        && self.ctx.binder.get_node_symbol(d) == Some(sym_id)
                });
                if has_class_in_current_arena {
                    return None; // Handle locally, don't delegate
                }
            }
        }

        // When the user re-declares a lib global function, keep the user's overloads in scope
        // (delegating to the lib arena would drop them and mis-resolve calls).
        //
        // The pin must confirm genuine current-arena OWNERSHIP, not just the
        // presence of *a* function node at the symbol's raw `NodeIndex`. A raw
        // `NodeIndex` is arena-relative: `cross_file_symbol` here is the FOREIGN
        // owner's symbol, so its declaration `NodeIndex`es are indices into the
        // OWNER's arena. Reusing one as an index into the CURRENT arena can land
        // on an unrelated function node that the current binder assigned to a
        // DIFFERENT symbol (e.g. mobx `die` in `errors.ts` and `runInAction` in
        // `api/action.ts` both declared at the same raw `NodeIndex(364)`).
        // Pinning that collision locally computes the wrong function's signature
        // for the foreign symbol and — because the result is then cached under
        // the owner's `(file_idx, SymbolId)` key first-writer-wins — poisons every
        // later reader of the real symbol. Require the current binder's
        // `get_node_symbol` round-trip to map the node back to exactly `sym_id`,
        // which holds for a genuine lib/local re-declaration (the binder assigned
        // the merged symbol to that node) but rejects the cross-arena collision.
        {
            let sym_found = cross_file_symbol;
            if let Some(symbol) = sym_found
                && symbol.has_any_flags(symbol_flags::FUNCTION)
                && !symbol.has_any_flags(
                    symbol_flags::CLASS | symbol_flags::INTERFACE | symbol_flags::ALIAS,
                )
            {
                let has_function_in_current_arena = symbol.declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| self.ctx.arena.get_function(n))
                        .is_some()
                        && self.ctx.binder.get_node_symbol(d) == Some(sym_id)
                });
                if has_function_in_current_arena {
                    return None; // Handle locally, don't delegate to lib arena
                }
            }
        }

        // A JS expando container variable (`var x = function(){}; x.a = …`) is
        // FUNCTION-flagged but its declaration is a `VariableDeclaration`, so
        // the FUNCTION guard above (which tests for a function *node*) does not
        // catch it. When the current file owns such a container declaration for
        // this symbol, resolve it locally rather than routing to a conflicting
        // cross-file sibling's arena — otherwise a `.ts` sibling's `var x =
        // <number>` becomes the type of the JS file's own `x`, mis-reporting
        // TS2339 on `x`'s own expando write (#17443). Only while the container
        // is the primary declaration; an earlier sibling's canonical type
        // governs instead (#17544). Scoped to checked JS.
        if self.is_js_file()
            && self.ctx.compiler_options.check_js
            && self.current_file_owns_authoritative_expando_container(sym_id)
        {
            return None; // Handle locally, don't delegate
        }

        let cross_file_symbol_is_class =
            cross_file_symbol.is_some_and(|symbol| symbol.has_any_flags(symbol_flags::CLASS));
        let is_known_cross_file = self.ctx.has_symbol_file_index(sym_id);

        if !is_known_cross_file
            && let Some(symbol) = cross_file_symbol
            && symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
        {
            return None;
        }

        let mut delegate_arena: Option<&tsz_parser::NodeArena> = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);
        let mut delegate_arena_source = if delegate_arena.is_some() {
            CrossArenaSymbolMissSource::SymbolArena
        } else {
            CrossArenaSymbolMissSource::Unknown
        };

        // For INTERFACE symbols that have local (user) interface declarations in the
        // current arena, do NOT delegate to the lib arena. The user's interface body
        // must be merged with the lib type, and delegating would lose the user's
        // members (e.g., `interface Node { forEachChild(...) }` augments lib Node).
        // The INTERFACE block in compute_type_of_symbol handles multi-arena merging.
        //
        // Also used below to prevent cross-file delegation fallback from overriding
        // this decision for merged interfaces across user files.
        let mut interface_has_local_decl = false;
        if delegate_arena.is_some_and(|arena| !std::ptr::eq(arena, self.ctx.arena))
            && let Some(symbol) = cross_file_symbol
            && symbol.has_any_flags(symbol_flags::INTERFACE)
            && self.symbol_has_local_interface_declaration(symbol)
        {
            delegate_arena = None; // Handle locally with merge
            interface_has_local_decl = true;
        }

        // The guard above only fires when `symbol_arenas` already points at a
        // foreign arena. A locally-declared interface that is re-exported
        // (`export * from "./x"`) and pulled in through a namespace import
        // (`import * as ns from "./re-exporter"`) instead has its canonical file
        // index point at the *re-exporting* module, while `symbol_arenas` is
        // unset or current — so `interface_has_local_decl` stays false and the
        // `resolve_symbol_file_index` path below delegates to the re-exporter's
        // arena. That arena has no body for the interface's `Lazy(DefId)`, so the
        // delegation returns the `error` sentinel, which then poisons every
        // derived-class member typed by the interface. Inspect the *local* binder
        // symbol (raw file-local `SymbolId`s are interpreted in the current
        // arena, mirroring the FUNCTION path below) so any genuine current-arena
        // interface declaration pins resolution locally regardless of how the
        // canonical file index resolved.
        if !interface_has_local_decl
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.has_any_flags(symbol_flags::INTERFACE)
            && self.symbol_has_local_interface_declaration(symbol)
        {
            delegate_arena = None; // Handle locally with merge
            interface_has_local_decl = true;
        }

        // Raw `SymbolId`s are file-local: `SymbolId(0)` may name `f` (FUNCTION) in
        // the current file and `x: string` (ALIAS) in another. Inspect the *local*
        // binder directly — `get_cross_file_symbol` would return the wrong symbol
        // when a cross-file overlay maps the same raw id to a foreign declaration —
        // so a current-file function declaration always pins resolution locally.
        // This covers both the lib-merge case (`declare function f` in current arena
        // overlapping a lib `declare function f`) and the multi-file collision case.
        //
        // As with the lib-redeclaration guard above, require the current binder's
        // `get_node_symbol` round-trip to confirm the candidate function node in
        // the current arena genuinely belongs to `sym_id`. A bare
        // `arena.get_function(d)` presence test fires on a raw-`NodeIndex`
        // collision — a function the current binder bound to a DIFFERENT symbol at
        // the same index — and would pin a foreign symbol to the wrong local
        // declaration.
        let mut function_has_local_decl = false;
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.has_any_flags(symbol_flags::FUNCTION)
            && !symbol
                .has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE | symbol_flags::ALIAS)
        {
            let has_local_function_decl = symbol.declarations.iter().any(|&d| {
                self.ctx
                    .arena
                    .get(d)
                    .and_then(|n| self.ctx.arena.get_function(n))
                    .is_some()
                    && self.ctx.binder.get_node_symbol(d) == Some(sym_id)
            });
            if has_local_function_decl {
                delegate_arena = None;
                function_has_local_decl = true;
            }
        }

        if delegate_arena.is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && let Some(symbol) = cross_file_symbol
        {
            // For INTERFACE symbols whose primary arena is already the current arena,
            // do NOT scan per-declaration arenas for delegation. Interfaces split across
            // multiple lib files (e.g., RegExp in es5 + es2015.symbol.wellknown) cause
            // ping-pong between arenas until the depth limit, resulting in ERROR.
            // The INTERFACE block in compute_type_of_symbol handles multi-arena merging
            // correctly via resolve_lib_type_by_name.
            // Skip for INTERFACE (merge path handles multi-arena via
            // resolve_lib_type_by_name) and for FUNCTION symbols that already
            // have a declaration in the current arena (we want the local
            // compute_type_of_symbol path to see every overload, including
            // the lib-arena ones, via declaration_arenas lookup).
            if !symbol.has_any_flags(symbol_flags::INTERFACE) && !function_has_local_decl {
                for decl_idx in symbol.all_declarations() {
                    if decl_idx.is_none() {
                        continue;
                    }
                    if let Some(arena) = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        && !std::ptr::eq(arena.as_ref(), self.ctx.arena)
                    {
                        delegate_arena = Some(arena.as_ref());
                        delegate_arena_source = CrossArenaSymbolMissSource::DeclarationArena;
                        break;
                    }
                }
            }
        }

        // Use recorded cross-file targets only when local merge handling is not required.
        //
        // The delegation DECISION and the resulting body/symbol-type cache KEY
        // (`cross_file_idx`) must be order-independent: the dynamic-overlay-first
        // `resolve_symbol_file_index` is schedule-dependent, so two parallel
        // arenas could disagree on whether to delegate (and under which owner
        // file_idx to key the shared bucket), splitting the symbol's type
        // identity (#13255 cross-arena body-cache residual). The stable resolver
        // prefers the immutable declaring-file index; it routes back to the
        // dynamic resolver when `TSZ_DISABLE_ORDER_INDEP_RESOLUTION=1`.
        let mut cross_file_idx: Option<usize> = None;
        let needs_cross_file_delegation = !interface_has_local_decl
            && !function_has_local_decl
            && delegate_arena.is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && self
                .ctx
                .resolve_symbol_file_index_stable(sym_id)
                .is_some_and(|file_idx| {
                    let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
                    !std::ptr::eq(target_arena, self.ctx.arena)
                });

        if needs_cross_file_delegation {
            let file_idx = self.ctx.resolve_symbol_file_index_stable(sym_id).expect(
                "needs_cross_file_delegation derived from resolve_symbol_file_index_stable returning Some",
            );
            cross_file_idx = Some(file_idx);
        }

        let should_delegate = needs_cross_file_delegation
            || delegate_arena.is_some_and(|arena| !std::ptr::eq(arena, self.ctx.arena));

        if should_delegate {
            // Session-memo key: the owner file of the delegated symbol. Uses
            // the same `(owner_file_idx, raw SymbolId)` key shape as the
            // canonical DefinitionStore cross-file buckets (the raw id is
            // interpreted in the owner binder), independent of the stable
            // shared-cache eligibility gate below.
            let memo_file_idx = requester_default_import_cache_file_idx
                .or(cross_file_idx)
                .or_else(|| delegate_arena.and_then(|arena| self.ctx.get_file_idx_for_arena(arena)))
                .map(|idx| idx as u32);

            let symbol_type_cache_file_idx =
                requester_default_import_cache_file_idx.or_else(|| {
                    self.symbol_arena_symbol_type_cache_file_idx(
                        needs_cross_file_delegation,
                        cross_file_idx,
                        delegate_arena_source,
                        delegate_arena,
                        sym_id,
                    )
                });
            let symbol_type_cache_from_symbol_arena = requester_default_import_cache_file_idx
                .is_none()
                && symbol_type_cache_file_idx.is_some()
                && !needs_cross_file_delegation;
            let symbol_type_cache_scope = symbol_type_cache_from_symbol_arena
                .then(|| self.ctx.source_file_symbol_type_cache_scope());
            let source_cache_scope = symbol_type_cache_scope.unwrap_or(0);

            let perf = if tsz_common::perf_counters::enabled_fast() {
                Some(tsz_common::perf_counters::counters())
            } else {
                None
            };
            let shared_actual_lib_delegation_name = self.shared_actual_lib_delegation_name(
                sym_id,
                delegate_arena,
                needs_cross_file_delegation,
            );
            if let Some(p) = perf {
                p.delegate_cross_arena_calls
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();

            // Delegation-tree memo of completed sentinel outcomes the shared
            // store refuses: replay the first completion instead of
            // re-running the full child checker per type-reference
            // occurrence inside this tree (issue #13041). At depth 0 this
            // call roots a new tree, so stale entries from the previous
            // tree's in-progress context are dropped first.
            if !Self::is_in_cross_arena_delegation() {
                self.ctx
                    .lib_delegation_cache
                    .session_memo()
                    .clear_for_new_delegation_tree();
            }
            let memo_context_fp = memo_file_idx
                .is_some()
                .then(|| self.cross_arena_context_fingerprint());
            if let Some(file_idx) = memo_file_idx
                && let Some(fp) = memo_context_fp
                && let Some(hit) = self
                    .ctx
                    .lib_delegation_cache
                    .session_memo()
                    .symbol
                    .get(&(file_idx, sym_id.0, fp))
            {
                let (cached_type, cached_params) = hit.clone();
                drop(hit);
                if cached_type != TypeId::ERROR && cached_type != TypeId::UNKNOWN {
                    self.ctx.symbol_types.insert(sym_id, cached_type);
                } else if needs_cross_file_delegation {
                    // Mirror the full-work path's child merge-back: for
                    // cross-file delegations the child's
                    // `symbol_types[sym] = ERROR` is merged into the
                    // requesting checker (`entry_or_insert`), and later
                    // `get_type_of_symbol` lookups short-circuit on it.
                    self.ctx.symbol_types.entry_or_insert(sym_id, cached_type);
                }
                if let Some(p) = perf {
                    p.delegate_cross_arena_cache_hits_cross_file
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                return Some((cached_type, cached_params));
            }

            if symbol_type_cache_file_idx.is_none()
                && !needs_cross_file_delegation
                && let Some(shared_name) = shared_actual_lib_delegation_name.as_deref()
                && let Some(cached) = self.cached_shared_actual_lib_delegation(sym_id, shared_name)
            {
                if let Some(p) = perf {
                    p.delegate_cross_arena_cache_hits_lib
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                return Some(cached);
            }

            if symbol_type_cache_file_idx.is_none()
                && !needs_cross_file_delegation
                && let Some((cached_type, cached_params)) =
                    self.ctx.lib_delegation_cache.symbol_type(sym_id)
            {
                if let Some(p) = perf {
                    p.delegate_cross_arena_cache_hits_lib
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                self.ctx.symbol_types.insert(sym_id, cached_type);
                let cached_params = if cached_params.is_empty()
                    || !self
                        .get_cross_file_symbol(sym_id)
                        .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_ALIAS))
                {
                    Vec::new()
                } else {
                    cached_params
                };
                return Some((cached_type, cached_params));
            }

            if let Some(cache_file_idx) = symbol_type_cache_file_idx
                && let Some((cached_type, cached_params)) = self
                    .cached_symbol_arena_or_cross_file_symbol_type(
                        sym_id,
                        cache_file_idx,
                        source_cache_scope,
                        symbol_type_cache_from_symbol_arena,
                    )
            {
                // Class symbols gate the SYMBOL-bucket shortcut on the
                // instance side being recoverable; see
                // `class_instance_recoverable` (#13185).
                if !cross_file_symbol_is_class
                    || self
                        .ctx
                        .class_instance_recoverable(sym_id, cache_file_idx as u32)
                {
                    if let Some(p) = perf {
                        p.delegate_cross_arena_cache_hits_cross_file
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    self.ctx.symbol_types.insert(sym_id, cached_type);
                    return Some((cached_type, cached_params));
                }
            }

            if let Some((result, params)) =
                self.try_resolve_cross_arena_named_alias_without_child(sym_id)
            {
                if let Some(file_idx) = symbol_type_cache_file_idx
                    && !symbol_type_cache_from_symbol_arena
                {
                    self.ctx.cache_cross_file_symbol_type(
                        sym_id,
                        file_idx as u32,
                        result,
                        params.clone(),
                    );
                }
                return Some((result, params));
            }

            if let Some(result) = self.direct_actual_lib_symbol_type(
                sym_id,
                delegate_arena_source,
                delegate_arena,
                needs_cross_file_delegation,
            ) {
                return Some(result);
            }
            let direct_target = if let Some(file_idx) = cross_file_idx {
                let arena = self.ctx.get_arena_for_file(file_idx as u32);
                let binder = self
                    .ctx
                    .get_binder_for_file(file_idx)
                    .unwrap_or(self.ctx.binder);
                Some((arena, binder, Some(file_idx)))
            } else {
                delegate_arena.map(|arena| {
                    let binder = if std::ptr::eq(arena, self.ctx.arena) {
                        self.ctx.binder
                    } else {
                        self.ctx
                            .get_binder_for_arena(arena)
                            .unwrap_or(self.ctx.binder)
                    };
                    let file_idx = if std::ptr::eq(arena, self.ctx.arena) {
                        Some(self.ctx.current_file_idx)
                    } else {
                        self.ctx.get_file_idx_for_arena(arena)
                    };
                    (arena, binder, file_idx)
                })
            };
            if let Some((symbol_arena, delegate_binder, _delegate_file_idx)) = direct_target
                && let Some((direct_type, direct_params)) = self
                    .direct_cross_file_interface_lowering(
                        sym_id,
                        delegate_binder,
                        symbol_arena,
                        false,
                        symbol_type_cache_from_symbol_arena
                            || is_direct_lowering_source_file_arena(symbol_arena),
                    )
            {
                self.ctx.symbol_types.insert(sym_id, direct_type);
                if let Some(file_idx) = symbol_type_cache_file_idx {
                    self.cache_symbol_arena_or_cross_file_symbol_type(
                        sym_id,
                        file_idx,
                        source_cache_scope,
                        symbol_type_cache_from_symbol_arena,
                        direct_type,
                        direct_params.clone(),
                    );
                }
                if symbol_type_cache_file_idx.is_none() && !needs_cross_file_delegation {
                    self.ctx
                        .lib_delegation_cache
                        .insert_symbol_type(sym_id, (direct_type, direct_params.clone()));
                }
                return Some((direct_type, direct_params));
            }

            if let Some(direct_type) = self
                .direct_source_file_variable_or_function_annotation_result(
                    sym_id,
                    direct_target,
                    true,
                )
            {
                self.ctx.symbol_types.insert(sym_id, direct_type);
                if let Some(file_idx) = symbol_type_cache_file_idx {
                    self.ctx.cache_stable_source_file_symbol_arena_type(
                        sym_id,
                        file_idx as u32,
                        source_cache_scope,
                        direct_type,
                        Vec::new(),
                    );
                }
                return Some((direct_type, Vec::new()));
            }

            if let Some(direct_type) =
                self.direct_source_file_function_declaration_result(sym_id, direct_target)
            {
                self.ctx.symbol_types.insert(sym_id, direct_type);
                if let Some(file_idx) = symbol_type_cache_file_idx {
                    self.cache_symbol_arena_or_cross_file_symbol_type(
                        sym_id,
                        file_idx,
                        source_cache_scope,
                        symbol_type_cache_from_symbol_arena,
                        direct_type,
                        Vec::new(),
                    );
                }
                return Some((direct_type, Vec::new()));
            }

            if let Some(result) = self.direct_declaration_file_type_alias_delegation_result(
                sym_id,
                cross_file_idx,
                symbol_type_cache_file_idx,
                source_cache_scope,
                symbol_type_cache_from_symbol_arena,
            ) {
                return Some(result);
            }

            let direct_target_file_idx =
                if symbol_type_cache_from_symbol_arena || needs_cross_file_delegation {
                    symbol_type_cache_file_idx
                } else {
                    None
                };
            let allow_direct_source_alias =
                symbol_type_cache_from_symbol_arena || needs_cross_file_delegation;
            if let Some((direct_type, direct_params)) = self.direct_source_file_type_alias_result(
                sym_id,
                direct_target_file_idx,
                allow_direct_source_alias,
            ) {
                self.ctx.symbol_types.insert(sym_id, direct_type);
                if let Some(file_idx) = symbol_type_cache_file_idx {
                    self.cache_symbol_arena_or_cross_file_symbol_type(
                        sym_id,
                        file_idx,
                        source_cache_scope,
                        symbol_type_cache_from_symbol_arena,
                        direct_type,
                        direct_params.clone(),
                    );
                }
                return Some((direct_type, direct_params));
            }

            // Lib-merged ids exist in no per-file binder: when every direct
            // shortcut above missed, the generic child path below would
            // interpret the merged id in the wrong id space, so delegate into
            // the owning lib context instead (issue #15687). Runs after the
            // shortcuts so actual-lib aliases keep their canonical name-keyed
            // resolution (and its materialization ratchets).
            if let Some(result) = self.delegate_lib_merged_symbol_type(sym_id) {
                return Some(result);
            }

            if let Some(p) = perf {
                p.delegate_cross_arena_misses
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let miss_source = if needs_cross_file_delegation {
                CrossArenaSymbolMissSource::SymbolFileTarget
            } else {
                delegate_arena_source
            };
            let miss_target_arena = if needs_cross_file_delegation {
                cross_file_idx.map(|file_idx| self.ctx.get_arena_for_file(file_idx as u32))
            } else {
                delegate_arena
            };
            let miss_target_source_file =
                miss_target_arena.and_then(|arena| arena.source_files.first());
            let miss_kind = self.cross_arena_symbol_miss_kind(sym_id);
            self.record_cross_arena_symbol_miss_residue(
                sym_id,
                miss_source,
                miss_kind,
                miss_target_source_file.is_some_and(|source_file| source_file.is_declaration_file),
                miss_target_source_file.map(|source_file| source_file.file_name.as_str()),
            );

            // Cross-file circular type-alias detection (TS2456): if resolving
            // this alias re-enters an alias already on the delegation path, mark
            // every member of the cycle circular so each file's
            // `check_cross_file_circular_type_aliases` post-pass emits its own
            // TS2456 (see `cross_file_alias_cycle`). We deliberately do NOT
            // short-circuit resolution here: the cross-arena depth guard below
            // terminates the recursion exactly as it does without this marking,
            // so a legitimately self-referential (non-circular per tsc, e.g. a
            // recursive lib alias resolved through deferral) type keeps
            // resolving to the same type main produces instead of collapsing
            // early to ERROR and cascading spurious assignability errors.
            let alias_cycle_def_id = self.cross_arena_alias_def_id(sym_id);
            if let Some(def_id) = alias_cycle_def_id {
                self.mark_cross_arena_alias_cycle(def_id);
            }
            // A class/interface delegated here is a deferral boundary for the
            // cross-arena alias cycle: an alias re-entered through this symbol's
            // member signatures defers like tsc and is not a TS2456 cycle (see
            // `mark_cross_arena_alias_cycle`).
            let delegated_symbol_is_class_or_interface = alias_cycle_def_id.is_none()
                && self.get_cross_file_symbol(sym_id).is_some_and(|symbol| {
                    symbol.has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE)
                });

            // Guard against deep cross-arena recursion to prevent stack overflow.
            // Uses shared thread-local counter across all delegation points.
            let Some(cross_arena_guard) = Self::enter_cross_arena_delegation() else {
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
                return Some((TypeId::ERROR, Vec::new()));
            };

            // Also check the per-checker recursion guard
            if !self.ctx.enter_recursion() {
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
                return Some((TypeId::ERROR, Vec::new()));
            }

            // Remove the in-progress ERROR marker before delegating to child checker.
            // The parent pre-caches ERROR as a cycle-detection marker and we don't
            // want the child checker to observe that placeholder.
            self.ctx.symbol_types.remove(&sym_id);

            // Re-fetch the arena reference after mutable operations above.
            // For cross-file symbols, use the target file's arena and binder.
            let (symbol_arena, delegate_binder, delegate_file_idx) =
                if let Some(file_idx) = cross_file_idx {
                    let arena = self.ctx.get_arena_for_file(file_idx as u32);
                    let binder = self
                        .ctx
                        .get_binder_for_file(file_idx)
                        .unwrap_or(self.ctx.binder);
                    (arena, binder, Some(file_idx))
                } else {
                    // Non-cross-file delegation: use the already-computed arena.
                    let arena = delegate_arena.unwrap_or(self.ctx.arena);
                    let binder = if std::ptr::eq(arena, self.ctx.arena) {
                        self.ctx.binder
                    } else {
                        self.ctx
                            .get_binder_for_arena(arena)
                            .unwrap_or(self.ctx.binder)
                    };
                    let file_idx = if std::ptr::eq(arena, self.ctx.arena) {
                        Some(self.ctx.current_file_idx)
                    } else {
                        self.ctx.get_file_idx_for_arena(arena)
                    };
                    (arena, binder, file_idx)
                };

            // Use the target file's name so that file-type-sensitive checks
            // (e.g. is_js_file() for optional JS parameters) use the declaring
            // file's context rather than the calling file's context.
            let delegate_file_name = symbol_arena
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
                .unwrap_or_else(|| self.ctx.file_name.clone());

            // Box the child checker to keep it on the heap — nested delegations for
            // interdependent lib types (Array → ReadonlyArray → Iterator → ...) can
            // create deep call stacks, and CheckerState is too large to stack-allocate
            // at every level without risking stack overflow.
            let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
                symbol_arena,
                delegate_binder,
                self.ctx.types,
                delegate_file_name,
                self.ctx.compiler_options.clone(),
                self, // Share parent's cache to fix Cache Isolation Bug
                tsz_common::perf_counters::CheckerCreationReason::DelegateCrossArenaSymbol,
            ));
            // Copy lib contexts for global symbol resolution (Array, Promise, etc.)
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            // Copy all cross-file state: arenas, binders, all 6 global indices,
            // resolved_module_paths, and module_specifiers.
            checker.ctx.copy_cross_file_state_from(&self.ctx);
            if super::cross_file_overlay_gate::symbol_delegation_needs_parent_targets(
                delegate_arena_source,
                symbol_arena,
                needs_cross_file_delegation,
            ) {
                // Copy cross-file symbol targets (local overlay only; global index
                // is already shared via copy_cross_file_state_from).
                self.ctx.copy_symbol_file_targets_to_attributed(
                    &mut checker.ctx,
                    tsz_common::perf_counters::CheckerCreationReason::DelegateCrossArenaSymbol,
                );
            }
            checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
            // The parent cache is cloned into the child for performance, but raw
            // SymbolIds can still collide across binders in direct multi-file tests.
            // Clear the delegated symbol's local cache entry so the child resolves it
            // against the authoritative binder instead of reusing a colliding parent
            // entry from the caller's file.
            checker.ctx.symbol_types.remove(&sym_id);
            checker.ctx.symbol_instance_types.remove(&sym_id);
            // Copy symbol resolution state to detect cross-file cycles, but exclude
            // the current symbol (which the parent added) since this checker will
            // add it again during get_type_of_symbol
            for &id in &self.ctx.symbol_resolution_set {
                if id != sym_id {
                    checker.ctx.symbol_resolution_set.insert(id);
                }
            }
            // DefId ↔ SymbolId mappings are no longer copied from parent to child.
            // The child's `def_to_symbol_id()` and `get_existing_def_id()` methods
            // fall back to the shared `DefinitionStore` on local cache miss, which
            // contains all mappings registered by any checker context. This enables
            // cross-file circular reference detection (e.g., `is_direct_circular_reference`)
            // without the O(N) copy overhead.

            // Copy class_instance_resolution_set to detect circular class inheritance
            for &id in &self.ctx.class_instance_resolution_set {
                checker.ctx.class_instance_resolution_set.insert(id);
            }
            // Copy class_constructor_resolution_set to detect circular constructor resolution
            for &id in &self.ctx.class_constructor_resolution_set {
                checker.ctx.class_constructor_resolution_set.insert(id);
            }

            // Wire up the shared DefinitionStore in both of the child's TypeEnvironments
            // so inner DefId→TypeId mappings survive child-checker teardown.
            checker.ctx.ensure_both_envs_have_definition_store();

            // Track this alias on the cross-arena resolution stack so a nested
            // delegation that comes back to it is recognized as a cross-file
            // cycle (see the detection above). The guard pops on drop —
            // including on panic unwind — so a reused worker thread never
            // inherits a stale entry.
            // Capture the cross-arena bailout epoch before resolving. If a
            // deeper delegation is refused by the depth cap while resolving this
            // symbol, the result is a transiently-incomplete artifact (a
            // provisional `any`/`error` minted under the cap) and must not be
            // persisted/promoted as authoritative — a later shallower pass
            // recomputes it. Mirrors the solver's `unresolved_def_seen` gate.
            let bailout_epoch_before = Self::cross_arena_bailout_epoch();
            let result = {
                let _alias_guard = alias_cycle_def_id.map(Self::enter_cross_arena_alias);
                let _class_boundary = delegated_symbol_is_class_or_interface
                    .then(Self::enter_cross_arena_class_boundary);
                // Use get_type_of_symbol to ensure proper cycle detection.
                checker.get_type_of_symbol(sym_id)
            };
            let resolved_under_bailout = Self::cross_arena_bailout_epoch() != bailout_epoch_before;
            // A provisional `any` minted under the cap must not be frozen as
            // this symbol's authoritative type (#13846). Concrete results — and
            // the deliberate `ERROR`/`UNKNOWN` cross-file cycle markers — are
            // fine to persist.
            let result_is_bailout_artifact = resolved_under_bailout && result == TypeId::ANY;
            let result_params = checker
                .ctx
                .get_existing_def_id(sym_id)
                .and_then(|def_id| checker.ctx.get_def_type_params(def_id))
                .unwrap_or_default();

            // Collect child data before dropping (child borrows from self.ctx.types).

            // Merge child's symbol_types back to parent to avoid re-resolving the
            // same types across delegations.  Without this, multi-file tests with
            // complex type libraries (react.d.ts) hang due to O(K×N) rework.
            //
            // For cross-file delegations (correct binder+arena pairing), ALL entries
            // are safe to merge.  For lib delegations, the child uses the parent's
            // binder with a lib arena, so entries for SymbolIds that belong to the
            // parent's binder may be corrupt (node index collision).  We filter those
            // out by only merging SymbolIds that the parent's binder doesn't own.
            let child_symbol_types: Vec<(SymbolId, TypeId)> = if needs_cross_file_delegation {
                // Cross-file: safe to merge everything
                checker.ctx.symbol_types.iter().collect()
            } else {
                // Lib delegation: only merge entries for MERGED lib SymbolIds.
                // During lib merge, symbols get new IDs tracked in
                // `lib_symbol_reverse_remap`. Entries for SymbolIds NOT in that
                // map belong to the parent binder's own symbols — they collide
                // with lib arena indices and may carry wrong types.
                checker
                    .ctx
                    .symbol_types
                    .iter()
                    .filter(|(k, _)| self.ctx.binder.lib_symbol_reverse_remap.contains_key(k))
                    .collect()
            };

            // def_to_symbol and def_type_params are no longer collected from the
            // child for merge-back. The child's `get_or_create_def_id()` and
            // `insert_def_type_params()` write through to the shared
            // `DefinitionStore`, so the parent can read them on next access via
            // the fallback path in `def_to_symbol_id()` and `get_def_type_params()`.

            // Merge the child's DefId→TypeId mappings into the parent's type_env.
            // The DefinitionStore write-through (set_body) only works for DefIds
            // that were created via register(), but get_or_create_def_id() does not
            // call register(). Copy the child's local def_types cache to ensure the
            // parent can resolve Lazy(DefId) references for types nested inside
            // cross-file interfaces (e.g., IServer inside IConfig's properties).
            if let Ok(child_env) = checker.ctx.type_env.try_borrow() {
                self.merge_child_type_env_snapshots(
                    &child_env,
                    "delegate_cross_arena_symbol_resolution",
                );
            }

            let child_namespace_names: rustc_hash::FxHashMap<TypeId, String> =
                std::mem::take(&mut checker.ctx.namespace_module_names);

            let child_lib_delegation_cache = std::mem::take(&mut checker.ctx.lib_delegation_cache);

            // Propagate lib type resolution cache from child to parent.
            // Without this, child contexts that resolve lib types (Array, Promise, etc.)
            // lose those cached results, forcing the parent to re-resolve them.
            let child_lib_type_cache: Vec<(String, Option<TypeId>)> =
                std::mem::take(&mut checker.ctx.lib_type_resolution_caches.types)
                    .into_iter()
                    .collect();

            // Collect circular type alias markers so the parent can detect
            // cross-file cycles.  When the child resolves `type B = A` and
            // finds A in the resolution set (from the parent), it marks A as
            // circular.  Propagating this back lets the parent's TS2456 check
            // for A fire correctly.
            let child_circular_aliases: Vec<SymbolId> =
                checker.ctx.circular_type_aliases.iter().copied().collect();

            // Propagate class instance types so that type-position references
            // (e.g., `foo(): Cls`) can resolve the instance type without
            // re-computing it from the class declaration (which lives in a
            // different arena and would fail).
            let child_instance_types: Vec<(SymbolId, TypeId)> =
                checker.ctx.symbol_instance_types.iter().collect();

            // Drop child checker to release borrow on self.ctx.types.
            drop(checker);

            // Merge collected data into the parent.
            // Note: def_to_symbol, def_type_params, and type_env DefId->TypeId
            // mappings are NOT merged back here. The child already wrote through
            // to the shared DefinitionStore, and the parent reads from
            // DefinitionStore on local cache miss.
            // Merge the child's resolved symbol types back into the parent, but
            // drop a provisional `any` minted under a depth-cap bailout while
            // resolving this symbol. Such an entry is a registration-window
            // artifact: merging it back propagates the poison first-writer-wins
            // into the parent and the program-global bucket, where it later
            // mis-routes identical patterns in other files (the immer
            // `[WRITABLE]` computed-key poison, #13846). Gating on bailout
            // *provenance* (not on the value being `any`) keeps genuine
            // cross-file `any` results cached, and dropping only the provisional
            // `any` lets a later shallower pass recompute and persist the
            // authoritative answer without a recompute storm over the concrete
            // siblings. `ERROR`/`UNKNOWN` cross-file cycle markers are preserved.
            for (sym_id, type_id) in child_symbol_types {
                if resolved_under_bailout && type_id == TypeId::ANY {
                    continue;
                }
                self.ctx.symbol_types.entry_or_insert(sym_id, type_id);
            }
            self.ctx
                .namespace_module_names
                .extend(child_namespace_names);
            for (name, cache_value) in child_lib_delegation_cache.symbol_types() {
                self.ctx
                    .lib_delegation_cache
                    .entry_or_insert_symbol_type(name, cache_value);
            }
            for (name, type_id) in child_lib_type_cache {
                self.ctx
                    .lib_type_resolution_caches
                    .types
                    .entry(name)
                    .or_insert(type_id);
            }
            for sym in child_circular_aliases {
                self.ctx.circular_type_aliases.insert(sym);
            }
            for (sym_id, inst_type) in child_instance_types {
                if resolved_under_bailout && inst_type == TypeId::ANY {
                    continue;
                }
                self.ctx
                    .symbol_instance_types
                    .entry_or_insert(sym_id, inst_type);
            }

            // Cache the result for lib delegations by SymbolId.
            // This prevents redundant child checker creation for the same lib symbol.
            // Skipped under a depth-cap bailout: the result is a transiently
            // incomplete artifact that must not be frozen (#13846).
            if symbol_type_cache_file_idx.is_none()
                && !needs_cross_file_delegation
                && !result_is_bailout_artifact
            {
                self.ctx
                    .lib_delegation_cache
                    .insert_symbol_type(sym_id, (result, result_params.clone()));
                if let Some(shared_name) = shared_actual_lib_delegation_name.as_deref() {
                    self.cache_shared_actual_lib_delegation(shared_name, result);
                }
            }

            // Write through to the canonical cross-file symbol-type cache so
            // other parallel checkers can reuse this result without rebuilding
            // a child checker. Skipped under a depth-cap bailout so a provisional
            // result is not promoted first-writer-wins (#13846).
            if let Some(target_file_idx) =
                symbol_type_cache_file_idx.filter(|_| !result_is_bailout_artifact)
            {
                self.cache_symbol_arena_or_cross_file_symbol_type(
                    sym_id,
                    target_file_idx,
                    source_cache_scope,
                    symbol_type_cache_from_symbol_arena,
                    result,
                    result_params.clone(),
                );
                // Publish the class INSTANCE type next to the SYMBOL (value)
                // entry; without it the SYMBOL entry cannot satisfy class
                // reads (see `class_instance_recoverable`, #13185).
                if !symbol_type_cache_from_symbol_arena
                    && cross_file_symbol_is_class
                    && let Some(inst) = self.ctx.symbol_instance_types.get(&sym_id)
                {
                    self.ctx.cache_cross_file_class_instance_type(
                        sym_id,
                        target_file_idx as u32,
                        inst,
                        result_params.clone(),
                    );
                }
            }

            // Record completed *sentinel* results in the session memo so
            // repeats within this file-check session replay them instead of
            // re-running the child checker (issue #13041's livelock was
            // exclusively repeated identical ERROR completions). Non-sentinel
            // results stay on the gated shared-store caches above, which
            // already model requester stability; memoizing them here changed
            // elaboration output on the valibot/kysely canaries. In-progress
            // guard returns above never reach this write.
            if matches!(result, TypeId::ERROR | TypeId::UNKNOWN) {
                tsz_common::perf_counters::record_delegate_cross_arena_full_work_sentinel_result();
            }
            if let Some(file_idx) = memo_file_idx
                && let Some(fp) = memo_context_fp
                && matches!(result, TypeId::ERROR | TypeId::UNKNOWN)
            {
                let memo = self.ctx.lib_delegation_cache.session_memo();
                memo.symbol
                    .insert((file_idx, sym_id.0, fp), (result, result_params.clone()));
                memo.mark_dirty();
            }

            self.ctx.leave_recursion();
            drop(cross_arena_guard);
            return Some((result, result_params));
        }

        None
    }

    /// Delegate class instance type resolution to a child checker with the correct arena.
    ///
    /// When a class symbol's declaration is not in the current file's arena (cross-file case),
    /// this creates a child checker using the symbol's home arena and computes the instance
    /// type there, where the class declaration node is accessible.
    pub(crate) fn delegate_cross_arena_class_instance_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        if !self
            .get_symbol_from_registered_file_target(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))?
            .has_any_flags(symbol_flags::CLASS)
        {
            return None;
        }

        // Find the symbol's home arena
        let mut delegate_arena: Option<&tsz_parser::NodeArena> = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);
        let mut delegate_file_idx = None;

        // Order-independent delegation decision + cache key (#13255): see the
        // comment in `delegate_cross_arena_symbol_resolution`. The stable
        // resolver keys the cross-file class-instance bucket on the immutable
        // declaring-file index so parallel arenas converge; it falls back to the
        // dynamic resolver under `TSZ_DISABLE_ORDER_INDEP_RESOLUTION=1`.
        let needs_cross_file_delegation = delegate_arena
            .is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && self
                .ctx
                .resolve_symbol_file_index_stable(sym_id)
                .is_some_and(|file_idx| {
                    let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
                    !std::ptr::eq(target_arena, self.ctx.arena)
                });

        // Lib arenas are absent from `global_arena_index`, so the DefinitionStore
        // cache below never fires for them; use the shared name-keyed lib class cache.
        let shared_lib_class_name =
            self.lib_class_shared_cache_name(sym_id, needs_cross_file_delegation);
        if let Some(shared_name) = shared_lib_class_name.as_deref()
            && let Some(cached) =
                self.cached_shared_actual_lib_class_delegation(sym_id, shared_name)
        {
            tsz_common::perf_counters::record_delegate_cross_arena_cache_hit_lib();
            return Some(cached);
        }

        if needs_cross_file_delegation {
            let file_idx = self.ctx.resolve_symbol_file_index_stable(sym_id).expect(
                "needs_cross_file_delegation derived from resolve_symbol_file_index_stable returning Some",
            );
            delegate_arena = Some(self.ctx.get_arena_for_file(file_idx as u32));
            delegate_file_idx = Some(file_idx);
        }

        let symbol_arena = delegate_arena.filter(|arena| !std::ptr::eq(*arena, self.ctx.arena))?;
        let query_file_idx =
            delegate_file_idx.or_else(|| self.ctx.get_file_idx_for_arena(symbol_arena));
        // Lib-merged symbols (issue #15687): a class merged from a lib context
        // exists only in the program binder under a remapped `SymbolId`, so the
        // declaration-file bailout below and the raw-id binder lookup both fail
        // for it. Resolve the originating lib binder and lib-local id through
        // `lib_symbol_reverse_remap` and delegate into that self-consistent
        // (arena, binder, id) triple instead.
        let lib_merged_origin = self
            .lib_merged_symbol_origin(sym_id)
            .filter(|(lib_ctx, _)| std::ptr::eq(lib_ctx.arena.as_ref(), symbol_arena))
            .map(|(lib_ctx, local_id)| (std::sync::Arc::clone(&lib_ctx.binder), local_id));
        if lib_merged_origin.is_none() && self.query_file_is_declaration_file(query_file_idx) {
            return None;
        }
        if let Some(file_idx) = query_file_idx
            && let Some((cached_type, cached_params)) = self
                .ctx
                .cached_cross_file_class_instance_type(sym_id, file_idx as u32)
        {
            tsz_common::perf_counters::record_delegate_cross_arena_cache_hit_cross_file();
            return Some((cached_type, cached_params.as_ref().clone()));
        }

        // Delegation-tree memo of completed `None`/sentinel outcomes the
        // shared bucket refuses to store (issue #13041: repeated full
        // child-checker recomputation). Non-sentinel results stay on the
        // shared bucket above. At depth 0 this call roots a new tree, so
        // stale entries from the previous tree's context are dropped first.
        if !Self::is_in_cross_arena_delegation() {
            self.ctx
                .lib_delegation_cache
                .session_memo()
                .clear_for_new_delegation_tree();
        }
        let memo_context_fp = query_file_idx
            .is_some()
            .then(|| self.cross_arena_context_fingerprint());
        if let Some(file_idx) = query_file_idx
            && let Some(fp) = memo_context_fp
            && let Some(hit) = self
                .ctx
                .lib_delegation_cache
                .session_memo()
                .class_instance
                .get(&(file_idx as u32, sym_id.0, fp))
        {
            tsz_common::perf_counters::record_delegate_cross_arena_cache_hit_cross_file();
            return hit.clone();
        }

        // Cross-arena class-instance cycle: this class is already being built
        // higher on the resolution path, so its member types (mutually,
        // cross-file) reference it. Re-delegating would recurse until the
        // cross-arena depth cap truncates the chain and drops members (a false
        // `TS2339` on the surviving type). Defer to a `Lazy(DefId)`
        // self-reference, mirroring the single-file `class_instance_resolution_set`
        // fallback; it resolves to the full instance type once the in-flight build
        // completes. Keyed by the stable `(owner file, declaration node)` — raw
        // `SymbolId`s / `DefId`s of the class differ per import alias, so they do
        // not identify "the same class" across the child checkers a delegation
        // creates.
        //
        // Restricted to pure classes: a class merged with a namespace/value module
        // (`class C {} namespace C {}`) carries static/namespace members on its
        // value side, and deferring its whole resolution to a lazy instance ref
        // drops those from `typeof C` while the class is still in flight (the
        // separate cross-file-statics-in-import-cycle gap; a `static make`/`create`
        // then reads as missing). Those merges keep the prior delegation behavior.
        if let Some(owner_file_idx) = query_file_idx
            && let Ok(owner_file_idx_u32) = u32::try_from(owner_file_idx)
            && let Some(symbol) = self
                .get_symbol_from_registered_file_target(sym_id)
                .or_else(|| self.get_cross_file_symbol(sym_id))
            && !symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
            && let Some(decl) = symbol.primary_declaration()
            && Self::cross_arena_class_instance_in_progress(owner_file_idx_u32, decl)
        {
            let params = self
                .ctx
                .get_def_type_params(self.ctx.get_or_create_def_id(sym_id))
                .unwrap_or_default();
            return Some((self.ctx.create_lazy_type_ref(sym_id), params));
        }

        let cross_arena_guard = Self::enter_cross_arena_delegation()?;

        if !self.ctx.enter_recursion() {
            return None;
        }

        // Use the target arena's file name for correct is_js_file() detection.
        let delegate_file_name = symbol_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        // Use the target file's binder when available so that node→symbol
        // lookups (e.g. `get_node_symbol` for private member `parent_id`)
        // resolve correctly instead of returning `None`.
        let delegate_binder = if let Some((lib_binder, _)) = lib_merged_origin.as_ref() {
            lib_binder.as_ref()
        } else if let Some(file_idx) = delegate_file_idx {
            self.ctx
                .get_binder_for_file(file_idx)
                .unwrap_or(self.ctx.binder)
        } else {
            self.ctx
                .get_binder_for_arena(symbol_arena)
                .unwrap_or(self.ctx.binder)
        };
        // The id the delegate binder knows the class by: the lib-local id for
        // lib-merged symbols, the shared raw id otherwise. Results are cached
        // under the caller-visible `sym_id` either way.
        let delegate_sym_id = lib_merged_origin
            .as_ref()
            .map_or(sym_id, |&(_, local_id)| local_id);
        // Cache check above returned None → about to do real work, so this
        // entry is a miss. Counts toward the `misses` denominator for
        // cache-hit-rate metrics.
        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();

        let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
            symbol_arena,
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
            tsz_common::perf_counters::CheckerCreationReason::DelegateCrossArenaClass,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.current_file_idx = query_file_idx
            .or(delegate_file_idx)
            .unwrap_or(self.ctx.current_file_idx);
        let delegated_class_is_ambient = delegate_binder
            .get_symbol(delegate_sym_id)
            .and_then(tsz_binder::Symbol::primary_declaration)
            .is_some_and(|decl_idx| checker.is_ambient_class_declaration(decl_idx));
        // The cross-file resolution state (all arenas/binders, resolved modules,
        // and the global name indices) is what lets the delegated child follow
        // import references that appear in the class's members — e.g. a generic
        // method whose type-parameter constraint references a type alias imported
        // into the declaring module (`bareC<TE extends AnyTable>` where `AnyTable`
        // is `import`ed). Ambient (`declare class`) user classes need this just as
        // much as concrete ones: without it the constraint alias resolves to
        // `Error`, which downstream widens inferred literal arguments (a spurious
        // `TS2322`/`TS2345` cascade) and drops the constraint's own enforcement
        // (#15256). Declaration/lib files bail out earlier (see the
        // `query_file_is_declaration_file` guard above), so this only broadens
        // resolution for ambient classes in real modules. `current_file_idx` is
        // already set above and `copy_cross_file_state_from` does not touch it;
        // the symbol-cache invalidation stays gated on the concrete path to
        // preserve ambient classes' cached-type reuse.
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        if !delegated_class_is_ambient {
            checker.ctx.symbol_types.remove(&delegate_sym_id);
            checker.ctx.symbol_instance_types.remove(&delegate_sym_id);
            checker
                .ctx
                .symbol_to_def
                .borrow_mut()
                .remove(&delegate_sym_id);
        }
        checker.propagate_class_delegation_setup(self, delegate_sym_id);
        if let Some((lib_binder, _)) = lib_merged_origin.as_ref() {
            // Lib-merged delegation always prepares the child, ambient or
            // not: the parent's copied state is keyed in the MERGED id space
            // (issue #15687).
            self.prepare_lib_merged_delegation_child(&mut checker, lib_binder, delegate_sym_id);
        } else if !delegated_class_is_ambient {
            self.clear_delegated_symbol_cache_collisions(
                &mut checker,
                delegate_binder,
                delegate_sym_id,
            );
        }

        // Record a class instance-type boundary on the cross-arena alias stack:
        // an alias re-entered through this class's members defers like tsc and is
        // not a TS2456 cycle (see `mark_cross_arena_alias_cycle`). The
        // class-instance in-progress key is pushed by the build itself
        // (`get_class_instance_type_inner`), which also covers a class first
        // built locally in its declaring file before any delegation.
        let result = {
            let _class_boundary = Self::enter_cross_arena_class_boundary();
            checker.class_instance_type_with_params_from_symbol(delegate_sym_id)
        };
        // Lib-merged symbols have no registered file target, so no other
        // publish site runs for them; publish under the caller-visible id so
        // repeated queries reuse the instance type instead of re-delegating.
        if lib_merged_origin.is_some()
            && let Some((instance_type, params)) = result.as_ref()
        {
            self.publish_delegated_class_instance_type(sym_id, *instance_type, params);
        }
        if self.ctx.share_owner_symbol_type_results
            && let (Some(file_idx), Some((type_id, params))) = (query_file_idx, result.as_ref())
            && *type_id != TypeId::UNKNOWN
            && *type_id != TypeId::ERROR
        {
            self.ctx.definition_store.cache_resolved_cross_file_query(
                CrossFileQueryKind::ClassInstance.as_storage_kind(),
                file_idx as u32,
                sym_id.0,
                0,
                0,
                *type_id,
                params.clone(),
            );
        }

        if let (Some(shared_name), Some((type_id, _))) =
            (shared_lib_class_name.as_deref(), result.as_ref())
        {
            self.cache_shared_actual_lib_class_delegation(shared_name, *type_id);
        }

        // Record only completed `None`/sentinel outcomes in the session
        // memo; non-sentinel results already write through to the shared
        // class-instance bucket above, which models requester stability.
        // In-progress guard returns above never reach here.
        let completed_negative = result
            .as_ref()
            .is_none_or(|(type_id, _)| matches!(*type_id, TypeId::ERROR | TypeId::UNKNOWN));
        if completed_negative
            && let Some(file_idx) = query_file_idx
            && let Some(fp) = memo_context_fp
        {
            let memo = self.ctx.lib_delegation_cache.session_memo();
            memo.class_instance
                .insert((file_idx as u32, sym_id.0, fp), result.clone());
            memo.mark_dirty();
        }

        self.ctx.leave_recursion();
        drop(cross_arena_guard);

        result
    }

    /// Delegate interface type resolution to a child checker with the symbol's home arena.
    ///
    /// When `type_reference_symbol_type` encounters a cross-file INTERFACE symbol
    /// whose declarations are in a different arena, `get_type_of_symbol` returns UNKNOWN.
    /// This function creates a child checker with the correct arena and resolves the
    /// interface type there.
    pub(crate) fn delegate_cross_arena_interface_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        // Prefer the symbol's declared arena, but fall back to explicit
        // cross-file ownership when the current binder does not know it.
        let mut delegate_arena: Option<&tsz_parser::NodeArena> = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);
        let mut delegate_file_idx = None;

        // Order-independent delegation decision + cache key (#13255): see the
        // comment in `delegate_cross_arena_symbol_resolution`. The interface
        // body bucket is keyed on the stable declaring-file index so parallel
        // arenas converge; falls back to the dynamic resolver under
        // `TSZ_DISABLE_ORDER_INDEP_RESOLUTION=1`.
        let needs_cross_file_delegation = delegate_arena
            .is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && self
                .ctx
                .resolve_symbol_file_index_stable(sym_id)
                .is_some_and(|file_idx| {
                    let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
                    !std::ptr::eq(target_arena, self.ctx.arena)
                });

        if needs_cross_file_delegation {
            let file_idx = self.ctx.resolve_symbol_file_index_stable(sym_id).expect(
                "needs_cross_file_delegation derived from resolve_symbol_file_index_stable returning Some",
            );
            delegate_arena = Some(self.ctx.get_arena_for_file(file_idx as u32));
            delegate_file_idx = Some(file_idx);
        }

        let symbol_arena = delegate_arena.filter(|arena| !std::ptr::eq(*arena, self.ctx.arena))?;
        let query_file_idx =
            delegate_file_idx.or_else(|| self.ctx.get_file_idx_for_arena(symbol_arena));
        if let Some(file_idx) = query_file_idx
            && let Some(cached_type) = self
                .ctx
                .cached_cross_file_interface_type(sym_id, file_idx as u32)
        {
            tsz_common::perf_counters::record_delegate_cross_arena_cache_hit_cross_file();
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx
                .definition_store
                .register_type_to_def(cached_type, def_id);
            return Some(cached_type);
        }

        // Delegation-tree memo of completed `None` (UNKNOWN/ERROR)
        // child-checker outcomes the shared bucket refuses to store (issue
        // #13041). Successful results stay on the shared interface bucket
        // above. At depth 0 this call roots a new tree, so stale entries
        // from the previous tree's context are dropped first.
        if !Self::is_in_cross_arena_delegation() {
            self.ctx
                .lib_delegation_cache
                .session_memo()
                .clear_for_new_delegation_tree();
        }
        let memo_context_fp = query_file_idx
            .is_some()
            .then(|| self.cross_arena_context_fingerprint());
        if let Some(file_idx) = query_file_idx
            && let Some(fp) = memo_context_fp
            && let Some(hit) = self
                .ctx
                .lib_delegation_cache
                .session_memo()
                .interface
                .get(&(file_idx as u32, sym_id.0, fp))
        {
            let cached = *hit;
            drop(hit);
            tsz_common::perf_counters::record_delegate_cross_arena_cache_hit_cross_file();
            return cached;
        }
        let delegate_binder = if let Some(file_idx) = delegate_file_idx {
            self.ctx
                .get_binder_for_file(file_idx)
                .unwrap_or(self.ctx.binder)
        } else {
            // Use the target arena's binder so that node→symbol lookups
            // (e.g. `get_node_symbol` for private member `parent_id`)
            // resolve correctly instead of returning `None`.
            self.ctx
                .get_binder_for_arena(symbol_arena)
                .unwrap_or(self.ctx.binder)
        };

        if let Some((direct_type, direct_params)) = self.direct_cross_file_interface_lowering(
            sym_id,
            delegate_binder,
            symbol_arena,
            false,
            false,
        ) {
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            if !direct_params.is_empty() {
                self.ctx.insert_def_type_params(def_id, direct_params);
            }
            if let Some(file_idx) = query_file_idx {
                self.ctx
                    .cache_cross_file_interface_type(sym_id, file_idx as u32, direct_type);
            }
            return Some(direct_type);
        }

        // Guard against deep cross-arena recursion
        let cross_arena_guard = Self::enter_cross_arena_delegation()?;

        if !self.ctx.enter_recursion() {
            return None;
        }

        let delegate_file_name = symbol_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        // PERF: see the matching block in `delegate_cross_arena_class_instance_type`.
        // Cache check above returned None → about to do real work.
        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();

        let mut checker = CheckerState::delegate_for_arena(
            symbol_arena,
            delegate_binder,
            delegate_file_name,
            self,
            tsz_common::perf_counters::CheckerCreationReason::DelegateCrossArenaInterface,
        );
        // `symbol_arenas` can identify the owner arena before the symbol has an
        // explicit file target. In that case `delegate_file_idx` is `None`, but
        // `query_file_idx` still records the arena's declaring file. Keep the
        // child checker aligned with that owner: retaining the requester's file
        // index makes names local to the delegated declaration look cross-file
        // while resolving its heritage (for example `Omit<LocalBase, K>`).
        checker.ctx.current_file_idx = query_file_idx.unwrap_or(self.ctx.current_file_idx);
        // Parent caches are cloned into the child for performance, but raw SymbolIds
        // can collide across binders. Clear the delegated symbol's entries so the
        // child recomputes the interface in its home binder instead of reusing a
        // colliding cache entry from the caller's file.
        checker.ctx.symbol_types.remove(&sym_id);
        checker.ctx.symbol_instance_types.remove(&sym_id);
        checker.ctx.symbol_to_def.borrow_mut().clear();
        checker.ctx.def_to_symbol.borrow_mut().clear();
        for &id in &self.ctx.symbol_resolution_set {
            if id != sym_id {
                checker.ctx.symbol_resolution_set.insert(id);
            }
        }
        // DefId ↔ SymbolId mappings are resolved via DefinitionStore fallback
        // on cache miss — no parent-to-child copy needed.

        // Wire up the shared DefinitionStore in both of the child's TypeEnvironments
        // so that DefId→TypeId mappings for inner types (e.g., IServer inside
        // IConfig's properties) are written through to the shared store. Without
        // this, the parent checker cannot resolve Lazy(DefId) references for
        // types nested inside the cross-file interface after the child is dropped.
        checker.ctx.ensure_both_envs_have_definition_store();

        // Try compute_interface_type_from_declarations first (more direct),
        // fall back to get_type_of_symbol for non-pure-interface symbols.
        Self::enter_cross_arena_interface_delegation();
        let mut result = checker.compute_interface_type_from_declarations(sym_id);
        Self::leave_cross_arena_interface_delegation();
        if result == TypeId::ERROR {
            result = checker.get_type_of_symbol(sym_id);
        }

        // Merge the child's DefId→TypeId mappings into the parent's type_env.
        // The child may have resolved inner types (e.g., IServer inside IConfig)
        // and registered their DefId→body mappings in its local type_env cache.
        // Without this merge, the parent cannot resolve Lazy(DefId) references
        // for those inner types after the child checker is dropped.
        if let Ok(child_env) = checker.ctx.type_env.try_borrow() {
            self.merge_child_type_env_snapshots(&child_env, "delegate_cross_arena_interface_type");
        } else {
            tracing::warn!(
                "delegate_cross_arena_interface_type: could not borrow child type_env for snapshot"
            );
        }

        // Merge the child's cross_file_symbol_targets back into the parent.
        // The child may have discovered new symbol → file mappings (e.g., when
        // resolving qualified names like `server.IWorkspace` where IWorkspace
        // belongs to server.ts). Without this merge, the parent cannot look up
        // these symbols in the correct binder, causing SymbolId collisions.
        self.ctx
            .merge_missing_symbol_file_targets_from(&checker.ctx);

        self.ctx.leave_recursion();
        drop(cross_arena_guard);

        let outcome = if result != TypeId::UNKNOWN && result != TypeId::ERROR {
            // Register instance type → DefId so the TypeFormatter can display
            // the interface name (e.g., "Date") instead of the structural form.
            // This mirrors the class registration in symbol_types.rs.
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx
                .definition_store
                .register_type_to_def(result, def_id);
            if let Some(file_idx) = query_file_idx {
                self.ctx
                    .cache_cross_file_interface_type(sym_id, file_idx as u32, result);
            }
            Some(result)
        } else {
            None
        };

        // Record only the completed-`None` (UNKNOWN/ERROR) child-checker
        // outcome in the session memo; successful results were written to
        // the shared interface bucket above. In-progress guard returns
        // never reach here.
        if outcome.is_none()
            && let Some(file_idx) = query_file_idx
            && let Some(fp) = memo_context_fp
        {
            let memo = self.ctx.lib_delegation_cache.session_memo();
            memo.interface
                .insert((file_idx as u32, sym_id.0, fp), outcome);
            memo.mark_dirty();
        }

        outcome
    }

    pub(crate) fn delegate_cross_arena_interface_member_simple_type(
        &mut self,
        interface_idx: NodeIndex,
        member_idx: NodeIndex,
        interface_arena: &tsz_parser::NodeArena,
        type_args: Option<&[TypeId]>,
    ) -> Option<TypeId> {
        self.delegate_cross_arena_interface_member_simple_types(
            interface_idx,
            std::slice::from_ref(&member_idx),
            interface_arena,
            type_args,
            false,
        )
        .and_then(|mut results| results.remove(&member_idx))
    }

    /// Resolve multiple members from the same remote interface with one child checker.
    ///
    /// Interface compatibility and module augmentation checks often walk every
    /// property/method in a remote declaration. Batching keeps the same target
    /// arena/binder semantics as the single-member path without constructing a
    /// child checker per member.
    pub(crate) fn delegate_cross_arena_interface_member_simple_types(
        &mut self,
        interface_idx: NodeIndex,
        member_indices: &[NodeIndex],
        interface_arena: &tsz_parser::NodeArena,
        type_args: Option<&[TypeId]>,
        allow_source_file_arena: bool,
    ) -> Option<rustc_hash::FxHashMap<NodeIndex, TypeId>> {
        if std::ptr::eq(interface_arena, self.ctx.arena) {
            return None;
        }
        if member_indices.is_empty() {
            return Some(rustc_hash::FxHashMap::default());
        }

        // O(1) via global_arena_index (replaces O(N) position scan)
        let delegate_file_idx = self.ctx.get_file_idx_for_arena(interface_arena);
        let delegate_binder_arc = delegate_file_idx
            .and_then(|file_idx| self.ctx.all_binders.as_ref()?.get(file_idx).cloned());
        let delegate_binder = delegate_binder_arc.as_deref().unwrap_or(self.ctx.binder);

        let mut results = rustc_hash::FxHashMap::default();
        let mut misses = Vec::new();
        if type_args.is_none()
            && let Some(file_idx) = delegate_file_idx
        {
            for &member_idx in member_indices {
                if let Some(cached_type) = self.ctx.cached_cross_file_interface_member_simple_type(
                    interface_idx,
                    member_idx,
                    file_idx as u32,
                ) {
                    tsz_common::perf_counters::record_delegate_cross_arena_cache_hit_cross_file();
                    results.insert(member_idx, cached_type);
                } else {
                    misses.push(member_idx);
                }
            }
        } else {
            misses.extend_from_slice(member_indices);
        }

        if misses.is_empty() {
            return Some(results);
        }

        if let Some(direct_results) = self.direct_cross_file_interface_member_simple_types(
            interface_idx,
            &misses,
            interface_arena,
            delegate_binder,
            type_args,
            allow_source_file_arena,
        ) {
            if type_args.is_none()
                && let Some(file_idx) = delegate_file_idx
            {
                for (&member_idx, &member_type) in direct_results.iter() {
                    self.ctx.cache_cross_file_interface_member_simple_type(
                        interface_idx,
                        member_idx,
                        file_idx as u32,
                        member_type,
                    );
                }
            }
            results.extend(direct_results);
            misses.retain(|member_idx| !results.contains_key(member_idx));
            if misses.is_empty() {
                return Some(results);
            }
        }

        let Some(cross_arena_guard) = Self::enter_cross_arena_delegation() else {
            return if results.is_empty() {
                None
            } else {
                Some(results)
            };
        };
        if !self.ctx.enter_recursion() {
            return if results.is_empty() {
                None
            } else {
                Some(results)
            };
        }

        let delegate_file_name = interface_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        // PERF: see the matching block in `delegate_cross_arena_class_instance_type`.
        // Cache check above returned None → about to do real work.
        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();

        let mut checker = CheckerState::delegate_for_arena(
            interface_arena,
            delegate_binder,
            delegate_file_name,
            self,
            tsz_common::perf_counters::CheckerCreationReason::DelegateCrossArenaOther,
        );
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        let parent_is_declaration_file = self.ctx.file_name.ends_with(".d.ts")
            || self.ctx.file_name.ends_with(".d.cts")
            || self.ctx.file_name.ends_with(".d.mts");
        let delegate_is_declaration_file = interface_arena
            .source_files
            .first()
            .is_some_and(|source_file| source_file.is_declaration_file);
        if parent_is_declaration_file && !delegate_is_declaration_file {
            checker
                .ctx
                .type_resolution_fuel
                .set(crate::state::MAX_TYPE_RESOLUTION_OPS);
            self.ctx.eval_session.reset_lazy_resolution_fuel();
            self.ctx.eval_session.reset_lazy_readiness_guards();
        }
        // DefId ↔ SymbolId mappings are resolved via DefinitionStore fallback
        // on cache miss — no parent-to-child copy needed.

        let interface_type_params = checker
            .ctx
            .arena
            .get(interface_idx)
            .and_then(|node| checker.ctx.arena.get_interface(node))
            .and_then(|iface| iface.type_parameters.clone());
        let (interface_params, interface_updates) = interface_type_params
            .as_ref()
            .map(|type_parameters| checker.push_type_parameters(&Some(type_parameters.clone())))
            .unwrap_or_default();

        let substitution = type_args
            .filter(|type_args| {
                !interface_params.is_empty() && type_args.len() <= interface_params.len()
            })
            .and_then(|type_args| {
                crate::query_boundaries::type_defaults::fill_application_defaults(
                    checker.ctx.types,
                    type_args,
                    &interface_params,
                )
            })
            .map(|type_args| {
                crate::query_boundaries::common::TypeSubstitution::from_args(
                    checker.ctx.types,
                    &interface_params,
                    &type_args,
                )
            });

        for member_idx in misses {
            let mut result = checker.get_type_of_interface_member_simple(member_idx);
            if let Some(substitution) = substitution.as_ref() {
                result = crate::query_boundaries::common::instantiate_type(
                    checker.ctx.types,
                    result,
                    substitution,
                );
            }
            if result != TypeId::UNKNOWN && result != TypeId::ERROR {
                if type_args.is_none()
                    && let Some(file_idx) = delegate_file_idx
                {
                    self.ctx.cache_cross_file_interface_member_simple_type(
                        interface_idx,
                        member_idx,
                        file_idx as u32,
                        result,
                    );
                }
                results.insert(member_idx, result);
            }
        }
        checker.pop_type_parameters(interface_updates);

        self.ctx.leave_recursion();
        drop(cross_arena_guard);

        Some(results)
    }
}

#[path = "cross_file_lowering.rs"]
mod cross_file_lowering;

#[cfg(test)]
#[path = "cross_file_query_kind_tests.rs"]
mod cross_file_query_kind_tests;

#[cfg(test)]
#[path = "cross_file_cache_tests.rs"]
mod tests;
