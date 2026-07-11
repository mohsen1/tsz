use crate::state::CheckerState;
use crate::types_domain::queries::core::{
    get_literal_or_well_known_property_name, get_literal_property_name,
};
use crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol;
use tsz_binder::BinderState;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// A resolved computed-property key together with its provenance.
///
/// `is_symbol` is `true` only when the key denotes a genuine binding identity
/// (`__unique_<id>` / `__symbol_<file>_<id>` derived from a `unique symbol` or
/// plain-`symbol` binding). It is `false` for string/number literal keys — even
/// when the literal value happens to spell the synthetic `__unique_`/`__symbol_`
/// prefix — and for well-known `[Symbol.X]` keys, which carry their own string
/// key on the interface lowering path. This distinction is what
/// `precompute_symbol_named_computed_property_names_in_arenas` needs to flag
/// late-bound members without a string-prefix heuristic that cannot separate a
/// real symbol from a `"__unique_1"`-valued string const.
struct ResolvedComputedName {
    name: String,
    is_symbol: bool,
}

impl ResolvedComputedName {
    const fn string(name: String) -> Self {
        Self {
            name,
            is_symbol: false,
        }
    }

    const fn symbol(name: String) -> Self {
        Self {
            name,
            is_symbol: true,
        }
    }
}

impl<'a> CheckerState<'a> {
    fn with_current_arena(&self, declarations: &[NodeIndex]) -> Vec<(NodeIndex, &'a NodeArena)> {
        declarations
            .iter()
            .map(|&decl_idx| (decl_idx, self.ctx.arena))
            .collect()
    }

    fn computed_property_owner_binder(&self, arena: &NodeArena) -> Option<&BinderState> {
        if std::ptr::eq(arena, self.ctx.arena) {
            return Some(self.ctx.binder);
        }
        if let Some(lib) = self
            .ctx
            .lib_contexts
            .iter()
            .find(|lib| std::ptr::eq(lib.arena.as_ref(), arena))
        {
            return Some(lib.binder.as_ref());
        }
        let arenas = self.ctx.all_arenas.as_ref()?;
        let file_idx = arenas
            .iter()
            .position(|candidate| std::ptr::eq(candidate.as_ref(), arena))?;
        self.ctx
            .all_binders
            .as_ref()?
            .get(file_idx)
            .map(std::convert::AsRef::as_ref)
    }

    pub(crate) fn prewarm_member_type_reference_params(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<tsz_solver::def::DefId, Vec<tsz_solver::TypeParamInfo>> {
        let decls = self.with_current_arena(declarations);
        self.prewarm_member_type_reference_params_in_arenas(&decls)
    }

    pub(crate) fn prewarm_member_type_reference_params_in_arenas(
        &mut self,
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> rustc_hash::FxHashMap<tsz_solver::def::DefId, Vec<tsz_solver::TypeParamInfo>> {
        // PERF: declaration files like react16.d.ts contain extremely large interface
        // graphs. Walking every descendant of every interface just to prewarm an
        // optional cache can dominate checker time. The lowering path already falls
        // back to `ctx.get_def_type_params(def_id)` on demand, so skipping the eager
        // prewarm here preserves correctness while avoiding repeated full-tree scans.
        if self.ctx.is_declaration_file() {
            return rustc_hash::FxHashMap::default();
        }

        let mut stack = Vec::new();
        let mut params_by_def = rustc_hash::FxHashMap::default();

        for &(decl_idx, decl_arena) in declarations {
            stack.push(decl_idx);

            while let Some(node_idx) = stack.pop() {
                let Some(node) = decl_arena.get(node_idx) else {
                    continue;
                };

                if node.kind == syntax_kind_ext::TYPE_REFERENCE
                    && let Some(type_ref) = decl_arena.get_type_ref(node)
                {
                    let has_type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .is_some_and(|args| !args.nodes.is_empty());
                    if !has_type_args
                        && let Some(sym_id) = self
                            .resolve_type_reference_symbol_in_arena(decl_arena, type_ref.type_name)
                    {
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        let params = self.get_type_params_for_symbol(sym_id);
                        if !params.is_empty() {
                            params_by_def.insert(def_id, params);
                        }
                    }
                }

                stack.extend(decl_arena.get_children(node_idx));
            }
        }

        params_by_def
    }

    /// Pre-compute property names for computed property name expressions in interface members.
    /// Iterates over all members of all declarations, finds `COMPUTED_PROPERTY_NAME` nodes,
    /// evaluates the expression type, and builds a map from expression `NodeIndex` to Atom.
    pub(crate) fn precompute_computed_property_names(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashMap<(NodeIndex, usize), tsz_common::Atom> {
        let decls = self.with_current_arena(declarations);
        self.precompute_computed_property_names_in_arenas(&decls)
    }

    pub(crate) fn precompute_computed_property_names_in_arenas(
        &mut self,
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> rustc_hash::FxHashMap<(NodeIndex, usize), tsz_common::Atom> {
        let mut map = rustc_hash::FxHashMap::default();
        for &(decl_idx, decl_arena) in declarations {
            let arena_key = decl_arena as *const NodeArena as usize;
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member) = decl_arena.get(member_idx) else {
                    continue;
                };
                // Get the name node from signature or accessor
                let name_idx = if let Some(sig) = decl_arena.get_signature(member) {
                    sig.name
                } else if let Some(acc) = decl_arena.get_accessor(member) {
                    acc.name
                } else {
                    continue;
                };
                let Some(name_node) = decl_arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = decl_arena.get_computed_property(name_node) else {
                    continue;
                };
                if let Some(resolved) =
                    self.resolve_computed_property_name_in_arena(decl_arena, name_idx)
                {
                    map.insert(
                        (computed.expression, arena_key),
                        self.ctx.types.intern_string(&resolved.name),
                    );
                }
            }
        }
        map
    }

    pub(crate) fn precompute_symbol_named_computed_property_names(
        &mut self,
        declarations: &[NodeIndex],
    ) -> rustc_hash::FxHashSet<(NodeIndex, usize)> {
        let decls = self.with_current_arena(declarations);
        self.precompute_symbol_named_computed_property_names_in_arenas(&decls)
    }

    pub(crate) fn precompute_symbol_named_computed_property_names_in_arenas(
        &mut self,
        declarations: &[(NodeIndex, &NodeArena)],
    ) -> rustc_hash::FxHashSet<(NodeIndex, usize)> {
        let mut set = rustc_hash::FxHashSet::default();
        for &(decl_idx, decl_arena) in declarations {
            let arena_key = decl_arena as *const NodeArena as usize;
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = decl_arena.get_interface(node) else {
                continue;
            };
            for &member_idx in &interface.members.nodes {
                let Some(member) = decl_arena.get(member_idx) else {
                    continue;
                };
                let name_idx = if let Some(sig) = decl_arena.get_signature(member) {
                    sig.name
                } else if let Some(acc) = decl_arena.get_accessor(member) {
                    acc.name
                } else {
                    continue;
                };
                let Some(name_node) = decl_arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = decl_arena.get_computed_property(name_node) else {
                    continue;
                };
                // A member is late-bound (symbol-named) exactly when its key
                // resolves to a genuine symbol binding identity. Reusing the same
                // resolution the name map runs — and reading its provenance flag —
                // keeps well-known `[Symbol.X]` keys and string/number literal keys
                // (including a `"__unique_1"`-valued const) out of the set, which a
                // string-prefix test over the resolved name cannot do.
                if self
                    .resolve_computed_property_name_in_arena(decl_arena, name_idx)
                    .is_some_and(|resolved| resolved.is_symbol)
                {
                    set.insert((computed.expression, arena_key));
                }
            }
        }
        set
    }

    fn resolve_type_reference_symbol_in_arena(
        &self,
        arena: &NodeArena,
        type_name: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        if std::ptr::eq(arena, self.ctx.arena) {
            return self
                .resolve_type_symbol_for_lowering(type_name)
                .map(tsz_binder::SymbolId);
        }

        let name = arena.get_identifier_text(type_name)?;
        resolve_name_to_lib_symbol(
            name,
            self.ctx.binder,
            self.ctx.global_file_locals_index.as_deref(),
            self.ctx
                .all_binders
                .as_ref()
                .map(|binders| binders.as_ref().as_slice()),
            &self.ctx.lib_contexts,
        )
    }

    fn resolve_computed_property_name_in_arena(
        &mut self,
        arena: &NodeArena,
        name_idx: NodeIndex,
    ) -> Option<ResolvedComputedName> {
        if std::ptr::eq(arena, self.ctx.arena) {
            return self.resolve_local_computed_property_name(name_idx);
        }

        // A literal key or a well-known `[Symbol.X]` key carries its own string
        // key; neither is a binding-identity symbol member.
        if let Some(name) = get_literal_or_well_known_property_name(arena, name_idx) {
            return Some(ResolvedComputedName::string(name));
        }

        let name_node = arena.get(name_idx)?;
        let computed = arena.get_computed_property(name_node)?;
        if let Some(name) =
            self.computed_expression_literal_name_in_arena(arena, computed.expression)
        {
            return Some(ResolvedComputedName::string(name));
        }
        // A computed key `[K]` whose `K` is a string/number `const` — `const K =
        // '$_TSR'` or `declare const K: '$R'` — keys the member under the literal
        // value, exactly as the current-file value-position path
        // (`resolve_local_computed_property_name`) does through type evaluation.
        // The cross-arena path cannot run `get_type_of_node` against a foreign
        // arena, so resolve the binding in the AUGMENTATION arena's own binder
        // (where `[K]` is written) and read the literal syntactically from the
        // declaration, following an import-type alias to its declaring file.
        // Without this leg a `declare global { interface Window { [K]?: T } }`
        // augmentation declared in one file drops its computed-const member when
        // the member is accessed from a DIFFERENT file (false TS2339 on
        // `self.$_TSR` / `window.$_TSR`).
        if let Some(literal_name) =
            self.cross_arena_const_literal_key_name(arena, computed.expression)
        {
            return Some(ResolvedComputedName::string(literal_name));
        }
        let sym_id = self.resolve_computed_property_symbol_in_arena(arena, computed.expression)?;
        // Canonicalize to the declaring binder's symbol id so a cross-file
        // interface member keyed here agrees with the same `const`'s key reached
        // through a different import path (e.g. a fresh object literal's
        // directly-imported `[matcher]`). Without this the member atom embeds
        // whichever per-file alias copy resolved here, producing a spurious
        // TS2353/TS2561 excess-property mismatch.
        let sym_id = crate::types_domain::computed_names::follow_import_aliases(&self.ctx, sym_id);
        Some(ResolvedComputedName::symbol(format!(
            "__unique_{}",
            sym_id.0
        )))
    }

    fn resolve_local_computed_property_name(
        &mut self,
        name_idx: NodeIndex,
    ) -> Option<ResolvedComputedName> {
        if let Some(name) = get_literal_property_name(self.ctx.arena, name_idx) {
            return Some(ResolvedComputedName::string(name));
        }

        let name_node = self.ctx.arena.get(name_idx)?;
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        // A well-known `[Symbol.X]` key carries its own string key; it is not a
        // binding-identity symbol member on the interface lowering path.
        if let Some(name) = self.local_well_known_symbol_property_name(computed.expression) {
            return Some(ResolvedComputedName::string(name));
        }

        if let Some(name) =
            self.computed_expression_literal_name_in_arena(self.ctx.arena, computed.expression)
        {
            return Some(ResolvedComputedName::string(name));
        }
        if let Some(literal_type) = self.const_object_member_literal_type_query(computed.expression)
            && let Some(name) =
                crate::query_boundaries::type_computation::access::literal_property_name(
                    self.ctx.types,
                    literal_type,
                )
        {
            return Some(ResolvedComputedName::string(
                self.ctx.types.resolve_atom_ref(name).to_string(),
            ));
        }
        // A computed name `[base.s]` whose qualified expression resolves to a
        // binding with unique-symbol identity (e.g. a namespace-import-qualified
        // `Symbol.for(...)` const reached through `import * as base`) keys the
        // member under the canonical `__unique_<id>` binding-identity atom. The
        // shared `computed_identifier_unique_symbol_property_ref` resolver
        // (identifier OR qualified entity name) runs the result through
        // `follow_import_aliases`, so the declaration-side member key here agrees
        // with the SAME atom the index-side element access derives from the
        // `unique symbol` index type. Without this leg the value-position
        // evaluation below cannot type a cross-module namespace member during
        // interface-member precomputation (it widens to `unknown`), the member is
        // keyed under its syntactic fallback name, and the canonical
        // `__unique_<id>` lookup misses -> false TS7053.
        if let Some(sym_ref) =
            self.computed_identifier_unique_symbol_property_ref(computed.expression)
        {
            return Some(ResolvedComputedName::symbol(format!(
                "__unique_{}",
                sym_ref.0
            )));
        }
        let prev = self.ctx.checking_computed_property_name;
        self.ctx.checking_computed_property_name = Some(name_idx);
        let prev_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let mut expr_type = self.get_type_of_node(computed.expression);
        // A `[K]` whose `K` is a type-only import (`import type { K }`) of a value
        // `const K = 'x'` has no value meaning, so the value-position evaluation
        // above yields no literal key. tsc still resolves the property name from
        // the binding's declared literal type; resolve that directly as a fallback
        // so the member is not dropped (false TS2339 on `[K]`-keyed members).
        if crate::query_boundaries::type_computation::access::literal_property_name(
            self.ctx.types,
            expr_type,
        )
        .is_none()
            && let Some(binding_type) = self.computed_name_binding_type(computed.expression)
        {
            expr_type = binding_type;
        }
        self.ctx.preserve_literal_types = prev_preserve;
        self.ctx.checking_computed_property_name = prev;
        if let Some(name) = crate::query_boundaries::type_computation::access::literal_property_name(
            self.ctx.types,
            expr_type,
        ) {
            // A `unique symbol` value type surfaces here as its binding-identity
            // key (`__unique_<id>`), which `literal_property_name` reports
            // alongside genuine string/number literal keys. That is a symbol-named
            // (late-bound) member, not a string-literal key — the distinction a
            // string const whose *value* spells `"__unique_1"` must NOT share.
            // Read the value type's unique-symbol identity to classify: a unique
            // symbol is `is_symbol`, a literal-typed const is not.
            if crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, expr_type)
                .is_some()
            {
                Some(ResolvedComputedName::symbol(
                    self.ctx.types.resolve_atom_ref(name).to_string(),
                ))
            } else {
                Some(ResolvedComputedName::string(
                    self.ctx.types.resolve_atom_ref(name).to_string(),
                ))
            }
        } else if let Some(name) =
            self.symbol_valued_binding_property_name(computed.expression, expr_type)
        {
            Some(ResolvedComputedName::symbol(name))
        } else if let Some(sym_ref) =
            crate::query_boundaries::common::unique_symbol_ref(self.ctx.types, expr_type)
        {
            Some(ResolvedComputedName::symbol(format!(
                "__unique_{}",
                sym_ref.0
            )))
        } else {
            // Value-position evaluation produced no key. The expression may
            // still denote a binding with unique-symbol / plain-`symbol`
            // identity that has no value meaning at this position — notably a
            // `unique symbol` reached through a type-only namespace import
            // (`import type * as s; [s.member]`), whose value-position type is
            // ERROR. Delegate to the canonical computed-name policy, which keys
            // purely from the resolved binding's identity rather than from its
            // value-position type, so the member is not dropped. Well-known
            // `[Symbol.X]` keys were already returned above, so a name reached
            // here denotes a binding-identity symbol member.
            self.computed_property_expression_name_atom(computed.expression)
                .map(|atom| ResolvedComputedName::symbol(self.ctx.types.resolve_atom(atom)))
        }
    }

    /// The declared type of the binding a bare-identifier computed-name
    /// expression refers to (e.g. `K` in `[K]`), following the binder's symbol
    /// resolution — which transparently resolves type-only-import aliases to the
    /// aliased `const`. Used only as a fallback when value-position evaluation
    /// produced no literal key, so it never overrides a working value resolution.
    fn computed_name_binding_type(&mut self, expr_idx: NodeIndex) -> Option<tsz_solver::TypeId> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, expr_idx)?;
        let ty = self.get_type_of_symbol(sym_id);
        if ty == tsz_solver::TypeId::ERROR || ty == tsz_solver::TypeId::UNKNOWN {
            None
        } else {
            Some(ty)
        }
    }

    fn local_well_known_symbol_property_name(&self, expr_idx: NodeIndex) -> Option<String> {
        use crate::types_domain::computed_names::{
            WellKnownSymbolName, well_known_symbol_property_name,
        };
        match well_known_symbol_property_name(&self.ctx, self.ctx.arena, self.ctx.binder, expr_idx)
        {
            Some(WellKnownSymbolName::Global(name)) => Some(name),
            Some(WellKnownSymbolName::Shadowed) | None => None,
        }
    }

    fn computed_expression_literal_name_in_arena(
        &self,
        arena: &NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(expr_idx)?;
        if matches!(
            node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        ) {
            return crate::types_domain::queries::core::get_literal_property_name(arena, expr_idx);
        }
        None
    }

    /// Resolve a bare-identifier computed-key expression `[K]` written in
    /// `arena` (the augmentation's own, possibly cross-file, arena) to the
    /// string/number literal name of the `const` it binds. The identifier is
    /// resolved in `arena`'s own binder — not the checker's current-file binder —
    /// then an import-type alias is followed to the declaring file, and the
    /// literal is read from THAT file's own arena. Handles both an initializer
    /// literal (`const K = '$_TSR'`) and a declared literal-type annotation
    /// (`declare const K: '$R'`). Returns `None` for non-`const` bindings,
    /// non-identifier expressions, or keys that do not denote a string/number
    /// literal — those fall through to the symbol-identity (`__unique_<id>`)
    /// path, matching tsc, which keys such members by literal name only when the
    /// key type is a string/number literal.
    fn cross_arena_const_literal_key_name(
        &self,
        arena: &NodeArena,
        mut expr_idx: NodeIndex,
    ) -> Option<String> {
        while let Some(node) = arena.get(expr_idx)
            && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
        {
            expr_idx = arena.get_parenthesized(node)?.expression;
        }
        let ident_name = arena.get_identifier_text(expr_idx)?.to_string();
        let aug_binder = self.ctx.get_binder_for_arena(arena)?;
        let local_sym_id = aug_binder
            .resolve_identifier(arena, expr_idx)
            .or_else(|| aug_binder.file_locals.get(&ident_name))?;
        // Follow an import alias (including a type-only `import type { K }`) to
        // the `const` in its declaring file. `resolve_import_alias_and_register`
        // follows the module specifier cross-file; the in-binder
        // `follow_import_aliases` covers same-binder re-export hops.
        let sym_id = self
            .ctx
            .resolve_import_alias_and_register(local_sym_id)
            .map(|target| {
                crate::types_domain::computed_names::follow_import_aliases(&self.ctx, target)
            })
            .unwrap_or_else(|| {
                crate::types_domain::computed_names::follow_import_aliases(&self.ctx, local_sym_id)
            });
        let symbol =
            crate::types_domain::computed_names::symbol_from_any_context(&self.ctx, sym_id)?;
        let decl = symbol.value_declaration;
        if decl.is_none() {
            return None;
        }
        let owner_arena = if symbol.decl_file_idx == u32::MAX {
            arena
        } else {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        };
        if !owner_arena.is_const_variable_declaration(decl) {
            return None;
        }
        let var_decl = owner_arena
            .get(decl)
            .and_then(|node| owner_arena.get_variable_declaration(node))?;

        // Initializer literal: `const K = '$_TSR'` / `const K = 1`.
        if let Some(name) = owner_arena
            .get(var_decl.initializer)
            .and_then(|init| owner_arena.get_literal(init))
            .map(|lit| lit.text.clone())
        {
            return Some(name);
        }

        // Declared literal-type annotation: `declare const K: '$R'`.
        let mut ann_idx = var_decl.type_annotation;
        if let Some(ann) = owner_arena.get(ann_idx)
            && ann.kind == syntax_kind_ext::LITERAL_TYPE
            && let Some(lit_type) = owner_arena.get_literal_type(ann)
        {
            ann_idx = lit_type.literal;
        }
        owner_arena
            .get(ann_idx)
            .filter(|n| {
                n.kind == SyntaxKind::StringLiteral as u16
                    || n.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    || n.kind == SyntaxKind::NumericLiteral as u16
            })
            .and_then(|_| get_literal_property_name(owner_arena, ann_idx))
    }

    fn resolve_computed_property_symbol_in_arena(
        &self,
        arena: &NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        let binder = self.computed_property_owner_binder(arena)?;
        self.resolve_computed_property_symbol_with_binder_in_arena(arena, binder, expr_idx)
    }

    fn resolve_computed_property_symbol_with_binder_in_arena(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        mut expr_idx: NodeIndex,
    ) -> Option<tsz_binder::SymbolId> {
        while let Some(node) = arena.get(expr_idx)
            && node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
        {
            expr_idx = arena.get_parenthesized(node)?.expression;
        }

        if let Some(sym_id) = binder.resolve_identifier(arena, expr_idx) {
            return Some(sym_id);
        }

        let name = if arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
        {
            arena.get_identifier_text(expr_idx)?.to_string()
        } else {
            crate::symbols_domain::name_text::expression_name_text_in_arena(arena, expr_idx)?
        };

        resolve_name_to_lib_symbol(
            &name,
            binder,
            self.ctx.global_file_locals_index.as_deref(),
            self.ctx
                .all_binders
                .as_ref()
                .map(|binders| binders.as_ref().as_slice()),
            &self.ctx.lib_contexts,
        )
    }
}
