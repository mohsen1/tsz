use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Push a mapped type's iteration variable (`K` in `[K in keyof T]`) into
    /// `type_parameter_scope` as a provisional carrying the resolved constraint.
    ///
    /// The constraint is required, not optional: downstream indexed-access
    /// well-formedness walks the constraint chain (`P -> K -> keyof T`) to
    /// suppress false TS2536 in nested mapped types like `{ [P in K]: T[P] }`.
    /// A bare placeholder drops the chain mid-walk and surfaces the false
    /// positive. Mirror what `check_type_node`'s `MAPPED_TYPE` arm writes so
    /// both checking passes observe a consistent binding.
    pub(crate) fn push_mapped_type_param_provisional(
        &mut self,
        type_parameter: NodeIndex,
    ) -> Option<(String, Option<TypeId>)> {
        let param_node = self.ctx.arena.get(type_parameter)?;
        let param = self.ctx.arena.get_type_parameter(param_node)?;
        let name_node = self.ctx.arena.get(param.name)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        let atom = self.ctx.types.intern_string(&ident.escaped_text);
        let name = ident.escaped_text.clone();
        let constraint_type = if param.constraint != NodeIndex::NONE {
            match self.get_type_from_type_node(param.constraint) {
                TypeId::ERROR => TypeId::UNKNOWN,
                resolved => resolved,
            }
        } else {
            TypeId::UNKNOWN
        };
        let type_id = self
            .ctx
            .types
            .factory()
            .type_param(tsz_solver::TypeParamInfo {
                name: atom,
                constraint: Some(constraint_type),
                default: None,
                is_const: false,
            });
        let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
        Some((name, previous))
    }

    /// Restore the `type_parameter_scope` entry previously captured by
    /// `push_mapped_type_param_provisional`.
    pub(crate) fn pop_mapped_type_param_provisional(
        &mut self,
        pushed: Option<(String, Option<TypeId>)>,
    ) {
        if let Some((name, previous)) = pushed {
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }
    }
}
