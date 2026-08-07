use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn written_keyof_any_constraint_display(
        &self,
        constraint: TypeId,
    ) -> Option<String> {
        let keyof_inner = query_common::keyof_inner_type(self.ctx.types, constraint)?;
        (keyof_inner == TypeId::ANY).then(|| "string | number | symbol".to_string())
    }

    /// Reference (display) form of a type argument for rendering a constraint
    /// in a diagnostic. A non-generic named type used as a type argument is
    /// interned as its inlined body, so substituting it into a `keyof T`
    /// constraint makes the formatter expand `keyof` to its literal key union.
    /// Rebuilding a `Lazy(DefId)` reference keeps the operator anchored to the
    /// name (`keyof T`), matching tsc.
    ///
    /// The recovery is gated on the *written* type argument being a type
    /// reference (`arg_node` is a `TYPE_REFERENCE`). Object/alias bodies are
    /// interned structurally, so an inline anonymous argument such as
    /// `MyPick<{ foo: 1 }, K>` shares a `TypeId` with any sibling alias of the
    /// same shape; recovering a def name purely from that shared `TypeId` would
    /// repaint the anonymous argument as an alias the user never wrote. Only
    /// when the source argument is itself a named reference is it correct to
    /// preserve that name. Returns the argument unchanged otherwise.
    pub(super) fn type_arg_reference_form(
        &self,
        type_arg: TypeId,
        arg_node: Option<NodeIndex>,
    ) -> TypeId {
        let db = self.ctx.types.as_type_database();
        if query_common::lazy_def_id(db, type_arg).is_some() {
            return type_arg;
        }

        // Only recover an alias/interface name when the user actually wrote a
        // type reference; an inline anonymous type must not borrow a structural
        // twin's name.
        let written_as_reference = arg_node
            .and_then(|idx| self.ctx.arena.get(idx))
            .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_REFERENCE);
        if !written_as_reference {
            return type_arg;
        }

        let store = &self.ctx.definition_store;
        let def_id = store
            .find_def_for_type(type_arg)
            .or_else(|| store.find_def_for_type(db.get_display_alias(type_arg)?));
        match def_id {
            Some(def_id)
                if store
                    .get(def_id)
                    .is_some_and(|def| def.type_params.is_empty()) =>
            {
                self.ctx.types.factory().lazy(def_id)
            }
            _ => type_arg,
        }
    }

    /// Display form for a constraint written as a non-generic alias whose body
    /// is the canonical primitive key union (`string | number | symbol`) — e.g.
    /// the lib `PropertyKey`, or a user `type Zed = string | number | symbol`.
    ///
    /// `tsc` renders such a constraint as the alias name written at the site
    /// (`PropertyKey`, `Zed`), like every other constraint surface: the spelling
    /// written at the site decides. tsz's generic-constraint validator resolves
    /// the constraint's `Lazy` wrapper to the shared canonical key union before
    /// the diagnostic is built (the assignability check needs the concrete
    /// union), and the key-union display path then force-expands that union
    /// structurally — dropping the alias name. Recover the written name from the
    /// *unresolved* constraint here, before that resolution happens.
    ///
    /// A constraint written longhand (`K extends string | number | symbol`)
    /// arrives without a `Lazy` wrapper, so this returns `None` and the
    /// structural rendering is preserved, matching `tsc`. The body is required
    /// to be the key union so that non-key-union aliases (which already keep
    /// their name through the ordinary display path) and primitive aliases like
    /// `type S = string` (which `tsc` renders as `string`, not `S`) are left
    /// untouched.
    pub(super) fn written_primitive_key_union_alias_display(
        &self,
        constraint: TypeId,
    ) -> Option<String> {
        use tsz_solver::def::DefKind;
        let db = self.ctx.types.as_type_database();
        let head_def_id = query_common::lazy_def_id(db, constraint)?;
        // tsc keeps a type's `aliasSymbol` on the alias whose declaration body is
        // *directly* the structural type. A pure alias-to-alias indirection
        // (`type B = A`) never mints its own alias, so `getDeclaredTypeOfSymbol`
        // returns `A`'s type object and `K extends B` renders the underlying `A`.
        // tsz inlines every non-generic alias body to one shared union `TypeId`,
        // so the indirection is invisible semantically; recover it by walking the
        // *source* declaration chain to the alias that directly owns the union.
        let owner_def_id = self.underlying_alias_owner_def(head_def_id);
        let def = self.ctx.definition_store.get(owner_def_id)?;
        if def.kind != DefKind::TypeAlias || !def.type_params.is_empty() {
            return None;
        }
        // Only the canonical key union reaches the force-expand display path this
        // recovery counters; every other alias body already renders its own name
        // through the ordinary `Lazy` display path, and a primitive-bodied alias
        // (`type S = string`) is stripped to its primitive by tsc. Gating on the
        // key-union shape keeps those untouched.
        if self.is_primitive_key_union_type(def.body?) {
            return Some(db.resolve_atom_ref(def.name).to_string());
        }
        None
    }

    /// Walk the pure non-generic alias-to-alias chain rooted at `def_id` through
    /// the *source* declarations, returning the `DefId` of the alias whose
    /// declaration body is directly a structural type rather than a bare
    /// reference to another type alias. tsc resolves `type B = A` to `A`'s type
    /// object, so the underlying alias — not the head written at the site — owns
    /// the displayed name.
    ///
    /// The chain is followed only while each hop's declaration is reachable in
    /// the current file's AST and its body is a bare non-generic alias
    /// reference; a hop whose target declaration lives in another file (e.g. the
    /// lib `PropertyKey`) terminates the walk at that target, which is treated as
    /// the owning alias. The bound guards a mutually-recursive alias cycle
    /// (`type A = B; type B = A`).
    fn underlying_alias_owner_def(&self, def_id: tsz_solver::DefId) -> tsz_solver::DefId {
        let mut current = def_id;
        for _ in 0..8 {
            match self.bare_alias_reference_target_def(current) {
                Some(next) => current = next,
                None => return current,
            }
        }
        current
    }

    /// When the non-generic type-alias `def_id`'s source declaration body is a
    /// bare reference (no type arguments) to another non-generic type alias,
    /// return that alias's `DefId`. Returns `None` for any other body shape (a
    /// union/object/operator written directly, a generic reference, or a
    /// reference to a non-alias) and when the declaration is not reachable in the
    /// current file's AST.
    fn bare_alias_reference_target_def(
        &self,
        def_id: tsz_solver::DefId,
    ) -> Option<tsz_solver::DefId> {
        use tsz_solver::def::DefKind;
        let sym_raw = self.ctx.definition_store.get(def_id)?.symbol_id?;
        let symbol = self.ctx.binder.get_symbol(tsz_binder::SymbolId(sym_raw))?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
            || symbol.declarations.len() != 1
        {
            return None;
        }
        let decl_node = self.ctx.arena.get(symbol.declarations[0])?;
        let type_alias = self.ctx.arena.get_type_alias(decl_node)?;
        if type_alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty())
        {
            return None;
        }
        // Unwrap a parenthesized body (`type B = (A)`), then require a bare type
        // reference carrying no type arguments — anything else means this alias
        // owns its structural body directly, so no hop is taken.
        let body_idx = self.unwrap_parenthesized_type(type_alias.type_node)?;
        let body_node = self.ctx.arena.get(body_idx)?;
        if body_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.ctx.arena.get_type_ref(body_node)?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }
        let target_raw = self.resolve_type_symbol_for_lowering(type_ref.type_name)?;
        let target_sym = tsz_binder::SymbolId(target_raw);
        let target_symbol = self
            .ctx
            .binder
            .get_symbol(target_sym)
            .or_else(|| self.get_cross_file_symbol(target_sym))?;
        if !target_symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return None;
        }
        // Read-only: a display path must not mint a fresh `DefId`. Any alias
        // reachable while emitting TS2344 already has one from checking; a miss
        // is the correct chain terminus.
        let target_def_id = self.ctx.get_existing_def_id(target_sym)?;
        let target_def = self.ctx.definition_store.get(target_def_id)?;
        if target_def.kind != DefKind::TypeAlias || !target_def.type_params.is_empty() {
            return None;
        }
        Some(target_def_id)
    }

    pub(super) fn written_keyof_constraint_display(
        &self,
        constraint: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args_list: &NodeList,
    ) -> Option<String> {
        let keyof_inner = query_common::keyof_inner_type(self.ctx.types, constraint)?;
        let param_info =
            query_common::type_param_info(self.ctx.types.as_type_database(), keyof_inner)?;
        let param_index = type_params
            .iter()
            .position(|param| param.name == param_info.name)?;
        let arg_idx = *type_args_list.nodes.get(param_index)?;
        let arg_node = self.ctx.arena.get(arg_idx)?;
        if arg_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let arg_ref = self.ctx.arena.get_type_ref(arg_node)?;
        if arg_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }
        let arg_name_node = self.ctx.arena.get(arg_ref.type_name)?;
        let arg_ident = self.ctx.arena.get_identifier(arg_name_node)?;
        Some(format!("keyof {}", arg_ident.escaped_text))
    }
}
