//! Cross-module interface-heritage publication/consumption helpers for
//! `type_reference_symbol_type_with_params` (split from `symbol_types.rs`
//! to keep that shard under the 2000-line cap).
//!
//! See the INTERFACE-branch publication/consumption gates in
//! `symbol_types.rs` for the structural rules; these helpers carry the
//! guard predicates (published-body member coverage, program-file heritage
//! provenance) and the import-alias `DefId` forwarding registration.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::{TypeId, TypeParamInfo};

impl<'a> CheckerState<'a> {
    /// Publish (declaration owner): when this checker owns every declaration
    /// (current-arena lowering) and the heritage merge contributed members,
    /// register the merged body in the shared `DefinitionStore` so importing
    /// files — whose local heritage merges cannot see this module's bases —
    /// resolve inherited members instead of re-deriving a memberless body.
    /// `register_def_auto_params_in_envs` invalidates def-dependent
    /// evaluation caches when the body changes, so results computed before
    /// this registration cannot shadow it.
    ///
    /// Scope: program-module files only (not ambient declaration files), and
    /// only interfaces whose direct heritage bases all resolve to
    /// program-file symbols. A lib base (`extends Response`,
    /// `extends Omit<RequestInit, ..>`) already merges through the lib-aware
    /// path on both sides; publishing those bodies additionally exposes a
    /// pre-existing union-normalization defect (a resolver-less evaluator
    /// mis-distributing still-generic conditionals, #13232) at relation
    /// sites.
    pub(crate) fn publish_heritage_merged_interface_body(
        &mut self,
        def_id: tsz_solver::DefId,
        merged: TypeId,
        interface_type: TypeId,
        needs_text_based_resolution: bool,
        declarations: &[NodeIndex],
        reg_params: Vec<TypeParamInfo>,
    ) {
        if !needs_text_based_resolution
            && !self.ctx.is_declaration_file()
            && merged != interface_type
            && merged != TypeId::ERROR
            && merged != TypeId::UNKNOWN
            && self.local_interface_heritage_bases_are_program_symbols(declarations)
            && self.ctx.definition_store.get_body(def_id) != Some(merged)
        {
            self.ctx
                .register_def_auto_params_in_envs(def_id, merged, reg_params);
        }
    }

    /// Consume (importing file): a foreign program-module interface with an
    /// `extends` clause whose heritage merge was a no-op here —
    /// `merge_interface_heritage_types` cannot read foreign declaration
    /// arenas and the lib-aware fallback resolves the bare name to the local
    /// import alias — silently drops every inherited member. Prefer the
    /// declaring checker's published heritage-merged body.
    ///
    /// Guards:
    /// - owner is another, non-ambient program file (declaration graphs keep
    ///   their dedicated lib/ambient resolution paths — the same boundary
    ///   `persist_env_eval_cache_entries` draws);
    /// - the published body covers every locally-derived member (a body
    ///   missing OWN members is a mid-resolution partial; consuming it would
    ///   trade missing inherited members for missing own members — the msw
    ///   `NetworkApi` family);
    /// - the published body is inference-inert (no callable/conditional
    ///   structure): signature/conditional-bearing bodies feed
    ///   contextual-inference paths where the pre-existing resolver-less
    ///   evaluation defect (#13232) produces false relation failures. Lift
    ///   once that defect is fixed.
    ///
    /// On consumption, late registration must invalidate: expressions in this
    /// file may already have evaluated applications of the def against the
    /// heritage-dropped local body registered into the env earlier in this
    /// same file (e.g. the first explicit-args reference), and those results
    /// sit in the def-keyed evaluation caches. The published body is
    /// registered in the envs and both cache layers are swept so every later
    /// evaluation observes the heritage-complete form. The returned params
    /// are the owner's registered ones: the published body's type-parameter
    /// occurrences carry the declaring checker's per-declaration identities
    /// (#13165), so instantiation must substitute those, not same-named
    /// locally re-derived parameters.
    pub(crate) fn try_consume_published_heritage_body(
        &mut self,
        sym_id: SymbolId,
        def_id: tsz_solver::DefId,
        merged: TypeId,
        local_merge_was_noop: bool,
        decls_with_arenas: &[(NodeIndex, &NodeArena)],
        params: &[TypeParamInfo],
    ) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        if !(local_merge_was_noop
            && !self.ctx.symbol_is_from_lib(sym_id)
            && self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .is_some_and(|file_idx| {
                    file_idx != self.ctx.current_file_idx
                        && !self.file_index_is_declaration_file(file_idx)
                })
            && decls_with_arenas.iter().any(|&(decl_idx, arena)| {
                arena
                    .get(decl_idx)
                    .and_then(|node| arena.get_interface(node))
                    .and_then(|iface| iface.heritage_clauses.as_ref())
                    .is_some_and(|clauses| !clauses.nodes.is_empty())
            }))
        {
            return None;
        }
        let published = self.ctx.definition_store.get_body(def_id)?;
        // The published body feeds contextual-inference paths only when it
        // carries a conditional that can surface still-generic during union
        // normalization (the #13232 resolver-less defect). A callable member
        // with no conditional in its signature — directly or behind an applied
        // alias — is inference-inert, so consuming it lets an importing file
        // resolve members inherited through a method-bearing generic interface
        // (`interface D<T> extends Base<T>` with a method on `Base`). Detect the
        // conditional *through* alias applications, since the standard content
        // walk treats an applied alias base as an opaque leaf.
        let published_feeds_conditional_inference = {
            let db = self.ctx.types.as_type_database();
            let def_store = &self.ctx.definition_store;
            let mut resolve_lazy = |d: tsz_solver::DefId| def_store.get_body(d);
            tsz_solver::type_queries::contains_conditional_through_aliases(
                db,
                published,
                &mut resolve_lazy,
            )
        };
        if published == TypeId::ERROR
            || published == TypeId::UNKNOWN
            || published == merged
            || crate::query_boundaries::common::lazy_def_id(self.ctx.types, published)
                == Some(def_id)
            || !self.published_body_covers_local_members(published, merged)
            || published_feeds_conditional_inference
        {
            return None;
        }
        let owner_params = self
            .ctx
            .definition_store
            .get_type_params(def_id)
            .filter(|owner| !owner.is_empty())
            .unwrap_or_else(|| params.to_vec());
        self.ctx
            .insert_def_type_params(def_id, owner_params.clone());
        self.ctx
            .register_def_auto_params_in_envs(def_id, published, owner_params.clone());
        self.ctx.clear_type_evaluation_caches_for_def(def_id);
        self.ctx
            .types
            .invalidate_application_eval_cache_for_def(def_id);
        tracing::debug!(
            target: "tsz::heritage_consume",
            sym = sym_id.0,
            published = published.0,
            "consumed published heritage body"
        );
        Some((published, owner_params))
    }

    /// Whether `published` (a candidate definition body from the shared
    /// `DefinitionStore`) carries at least every named property the local
    /// lowering `local` derived. Conservative on non-object shapes: returns
    /// `false` so the caller keeps the local form.
    pub(crate) fn published_body_covers_local_members(
        &self,
        published: TypeId,
        local: TypeId,
    ) -> bool {
        tsz_solver::type_queries::object_property_names_cover(
            self.ctx.types.as_type_database(),
            published,
            local,
        )
    }

    /// Whether every direct `extends` base of the given current-arena
    /// interface declarations resolves to a program-file (non-lib) type
    /// symbol. Unresolvable bases count as non-program (conservative: the
    /// publication caller skips rather than publishing a body whose heritage
    /// provenance is unknown).
    pub(crate) fn local_interface_heritage_bases_are_program_symbols(
        &self,
        declarations: &[NodeIndex],
    ) -> bool {
        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };
            let Some(clauses) = interface.heritage_clauses.as_ref() else {
                continue;
            };
            for &clause_idx in &clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    let expr_idx = if let Some(eta) = self.ctx.arena.get_expr_type_args(type_node) {
                        eta.expression
                    } else if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                        type_ref.type_name
                    } else {
                        type_idx
                    };
                    let Some(base_sym) = self
                        .resolve_type_symbol_for_lowering(expr_idx)
                        .map(tsz_binder::SymbolId)
                    else {
                        return false;
                    };
                    if self.ctx.symbol_is_from_lib(base_sym)
                        || self.ctx.symbol_is_from_actual_or_cloned_lib(base_sym)
                    {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Forward an import-alias symbol's `DefId` to its resolved target body.
    ///
    /// Type annotations lower the alias *name*, so `Application(Lazy(def),
    /// args)` and bare `Lazy(def)` in this file carry the alias's `DefId` —
    /// historically body-less. Registering the resolved target body (and the
    /// target's parameter list, whose identities the body's type-parameter
    /// occurrences carry) makes the alias def expandable wherever the target
    /// is, so a relation between an alias-keyed application and a
    /// target-keyed application of the same definition no longer degrades
    /// into "expanded shape vs. permanently opaque application".
    ///
    /// Skips placeholder results (`UNKNOWN`/`ERROR`) and self-lazy wrappers,
    /// and re-registers only when the body actually differs (avoiding
    /// generation churn on the hot resolution path).
    pub(crate) fn register_alias_def_forwarding(
        &mut self,
        alias_sym_id: SymbolId,
        target_sym_id: SymbolId,
        target_type: TypeId,
        target_params: &[tsz_solver::TypeParamInfo],
    ) {
        if target_type == TypeId::UNKNOWN || target_type == TypeId::ERROR {
            return;
        }
        // `get_or_create`: annotation lowering may not have minted the alias
        // `DefId` yet at first resolution; the symbol-keyed mapping is shared,
        // so the def created here is the one later lowerings reuse.
        let alias_def_id = self.ctx.get_or_create_def_id(alias_sym_id);
        // Canonical identity: alias-keyed `Lazy`/`Application` bases and the
        // declaring module's own key must compare as one definition in
        // relation logic (same-definition families, variance fast paths).
        let target_def_id = self.ctx.get_or_create_def_id(target_sym_id);
        self.ctx
            .definition_store
            .set_alias_forward(alias_def_id, target_def_id);
        // Intentionally no body registration here: the resolved target type
        // in an importing checker can be an incomplete (heritage-dropped)
        // local lowering, and registering it would shadow the richer
        // cross-arena delegation paths that body-less alias defs fall
        // through to today (measured: msw +20 / kysely +4 with the body
        // registered). The forward link alone is what relation logic needs
        // to keep alias-keyed and declaring-keyed applications in one
        // same-definition family.
        let _ = target_params;
        let _ = target_type;
    }
}
