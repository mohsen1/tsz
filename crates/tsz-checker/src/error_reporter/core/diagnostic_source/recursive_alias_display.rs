use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn recursive_alias_application_source_display(
        &mut self,
        expr_idx: NodeIndex,
        declared_type: TypeId,
    ) -> Option<String> {
        if !crate::query_boundaries::recursive_alias::is_recursive_type_alias_application(
            self.ctx.types,
            &self.ctx.definition_store,
            declared_type,
        ) {
            return None;
        }
        let annotation = self.declared_diagnostic_source_annotation_text(expr_idx)?;
        Some(self.format_annotation_like_type(&annotation))
    }

    /// Render a generic interface/class application source as its as-written
    /// reference (`O<T>`) instead of its evaluated structural shape.
    ///
    /// Returns `Some` only when `source` is an `Application` whose base resolves
    /// to a generic `interface`/`class` definition and at least one type
    /// argument still contains a free type parameter. In that case the type
    /// arguments are exactly the as-written ones, so the solver formatter
    /// renders the reference verbatim. Concrete instantiations (`O<number>`)
    /// return `None` so they keep flowing through the structural/widening
    /// display path that already matches tsc for fully-resolved arguments.
    ///
    /// The widening / structural-display fallbacks in the assignment-source
    /// formatter otherwise resolve the application to its instantiated member
    /// object and re-derive a parametric name from the member types, which
    /// over-instantiates the displayed argument (e.g. `O<U> { item: P<U> }`
    /// renders `O<T>` as `O<P<T>>`, and `OwnerList<U> extends List<List<U>>`
    /// renders `OwnerList<T>` as `OwnerList<List<T>>`). tsc keeps the as-written
    /// reference — its type arguments cannot be soundly recovered from the
    /// instantiated members.
    pub(in crate::error_reporter) fn generic_nominal_application_source_display(
        &mut self,
        source: TypeId,
    ) -> Option<String> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, source)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if !matches!(
            def.kind,
            tsz_solver::def::DefKind::Interface | tsz_solver::def::DefKind::Class
        ) {
            return None;
        }
        if def.type_params.is_empty() {
            return None;
        }
        let has_parametric_arg = app.args.iter().any(|&arg| {
            crate::query_boundaries::common::contains_type_parameters(self.ctx.types, arg)
        });
        if !has_parametric_arg {
            return None;
        }
        Some(self.format_type_diagnostic(source))
    }
}
