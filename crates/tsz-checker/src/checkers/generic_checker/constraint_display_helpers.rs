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
        let def_id = query_common::lazy_def_id(db, constraint)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != DefKind::TypeAlias || !def.type_params.is_empty() {
            return None;
        }
        // The written spelling at the site is this head alias; its name is what
        // renders, regardless of any intermediate aliases the body chains
        // through (`type B = A; type A = string | number | symbol` still renders
        // `B`).
        let name = db.resolve_atom_ref(def.name).to_string();
        // Follow the non-generic alias chain to its underlying body, one hop at
        // a time via the shared single-hop resolver, and accept only the
        // canonical key-union shape. The bound is a cycle guard against a
        // mutually-recursive alias (`type A = B; type B = A`); a genuine chain
        // terminates earlier when a hop stops making progress.
        let mut body = def.body?;
        for _ in 0..8 {
            if self.is_primitive_key_union_type(body) {
                return Some(name);
            }
            let next = self.resolve_non_generic_alias_body_for_display(body);
            if next == body {
                break;
            }
            body = next;
        }
        None
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
