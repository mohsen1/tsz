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
