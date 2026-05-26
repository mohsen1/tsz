//! Resolution of generic JSDoc `@typedef` applications (`Name<Args...>`).
//!
//! Self-recursive generic typedefs (a `@typedef` whose body applies its own
//! name with type arguments) are handled here: the alias is registered as a
//! lazy `DefId` before its body is built, so the inner self-application defers
//! to `Application(Lazy(DefId), args)` and the solver resolves it
//! coinductively instead of re-expanding the body until the stack overflows.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::jsdoc::resolution) fn resolve_jsdoc_generic_typedef_type(
        &mut self,
        base_name: &str,
        type_args: &[TypeId],
    ) -> Option<TypeId> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        // Re-entrancy: if we are already expanding this generic typedef's body,
        // a self-recursive application like `Box<T>` inside `Box`'s own
        // definition must defer to the alias's lazy `DefId` rather than
        // re-expand the body (which would recurse until stack overflow). The
        // solver then resolves the application coinductively.
        let in_progress_def = self
            .ctx
            .jsdoc_generic_typedef_resolving
            .borrow()
            .get(base_name)
            .copied();
        if let Some(def_id) = in_progress_def {
            let base = self.ctx.types.factory().lazy(def_id);
            if type_args.is_empty() {
                return Some(base);
            }
            let app = self
                .ctx
                .types
                .factory()
                .application(base, type_args.to_vec());
            self.register_jsdoc_generic_display_name(base_name, type_args, app);
            return Some(app);
        }

        let source_file = self.ctx.arena.source_files.first()?;
        let mut best_def = None;
        for comment in &source_file.comments {
            if !is_jsdoc_comment(comment, &source_file.text) {
                continue;
            }
            let content = get_jsdoc_content(comment, &source_file.text);
            for (name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if name == base_name {
                    best_def = Some(typedef_info);
                }
            }
        }

        let (body_type, type_params) =
            self.type_from_jsdoc_typedef_inner(best_def?, Some(base_name))?;
        if type_args.is_empty() {
            return Some(body_type);
        }
        if type_params.is_empty() {
            return None;
        }

        use crate::query_boundaries::common::instantiate_generic;
        let instantiated = instantiate_generic(self.ctx.types, body_type, &type_params, type_args);
        self.register_jsdoc_generic_display_name(base_name, type_args, instantiated);
        Some(instantiated)
    }

    /// Find or register the lazy alias `DefId` for a generic JSDoc `@typedef`.
    ///
    /// Deduplicated by `(file, name, type parameters)` so repeated references
    /// to the same alias in a file share one stable `DefId` (preserving type
    /// identity), while a same-named generic typedef in a *different* file gets
    /// its own `DefId` and is never collapsed onto another file's alias. The
    /// body is filled in later via `set_body` once it has been constructed.
    pub(in crate::jsdoc::resolution) fn ensure_recursive_jsdoc_typedef_def(
        &mut self,
        name: &str,
        type_params: &[tsz_solver::TypeParamInfo],
    ) -> tsz_solver::def::DefId {
        use tsz_solver::def::{DefKind, DefinitionInfo};

        let file_id = self.ctx.current_file_idx as u32;
        let atom_name = self.ctx.types.intern_string(name);
        if let Some(candidates) = self.ctx.definition_store.find_defs_by_name(atom_name) {
            for def_id in candidates {
                if let Some(def) = self.ctx.definition_store.get(def_id)
                    && matches!(def.kind, DefKind::TypeAlias)
                    && def.file_id == Some(file_id)
                    && def.type_params.as_slice() == type_params
                {
                    return def_id;
                }
            }
        }
        let mut info = DefinitionInfo::type_alias(atom_name, type_params.to_vec(), TypeId::ERROR);
        info.file_id = Some(file_id);
        self.ctx.definition_store.register(info)
    }
}
