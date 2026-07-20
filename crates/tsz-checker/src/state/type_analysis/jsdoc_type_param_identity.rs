//! Stable declaration identity for JSDoc `@template` binders.

use crate::query_boundaries::signature_building as signature_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

type TypeParamPushResult = (
    Vec<tsz_solver::TypeParamInfo>,
    Vec<(String, Option<TypeId>, bool)>,
);

#[derive(Clone, Copy)]
enum JsdocTypeParamSite {
    Owner(u32),
    Comment(u32),
}

impl CheckerState<'_> {
    /// Push JSDoc `@template` binders owned by `owner_node` into the active
    /// type-parameter scope and return their declaration-stamped signature
    /// records.
    ///
    /// The owner node supplies stable identity because raw JSDoc template tags
    /// have no AST name nodes. Repeated construction of the same class or
    /// callable therefore reuses the same `TypeId`, while a nested owner that
    /// repeats the same spelling/constraint/default receives a distinct binder.
    pub(crate) fn push_jsdoc_template_type_parameters_for_owner(
        &mut self,
        owner_node: NodeIndex,
        jsdoc: &str,
    ) -> TypeParamPushResult {
        self.push_jsdoc_template_type_parameters_for_site(
            JsdocTypeParamSite::Owner(owner_node.0),
            jsdoc,
        )
    }

    /// Push the JSDoc template binders declared by one overload comment.
    /// The comment start is unique within its source file and lives in a
    /// declaration-site namespace disjoint from AST node indices.
    pub(crate) fn push_jsdoc_template_type_parameters_for_comment(
        &mut self,
        comment_pos: u32,
        jsdoc: &str,
    ) -> TypeParamPushResult {
        self.push_jsdoc_template_type_parameters_for_site(
            JsdocTypeParamSite::Comment(comment_pos),
            jsdoc,
        )
    }

    fn push_jsdoc_template_type_parameters_for_site(
        &mut self,
        declaration_site: JsdocTypeParamSite,
        jsdoc: &str,
    ) -> TypeParamPushResult {
        let template_names = Self::jsdoc_template_type_params(jsdoc);
        if template_names.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let constraint_strs = Self::jsdoc_template_constraint_strings(jsdoc);
        let mut params = Vec::with_capacity(template_names.len());
        let mut updates = Vec::with_capacity(template_names.len());

        for (name, is_const, default_str) in template_names {
            let atom = self.ctx.types.intern_string(&name);
            let default = default_str
                .as_deref()
                .and_then(|type_expr| self.resolve_jsdoc_reference(type_expr));
            let constraint = constraint_strs
                .get(&name)
                .and_then(|type_expr| self.resolve_jsdoc_reference(type_expr));
            let info = signature_query::user_type_param_info(atom, constraint, default, is_const);
            let (type_id, stamped_info) =
                self.intern_jsdoc_type_param_for_site_stamped(declaration_site, info);

            let mut shadowed_class_param = false;
            if let Some(ref mut class) = self.ctx.enclosing_class
                && let Some(position) = class.type_param_names.iter().position(|item| *item == name)
            {
                class.type_param_names.remove(position);
                shadowed_class_param = true;
            }

            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous, shadowed_class_param));
            params.push(stamped_info);
        }

        (params, updates)
    }

    /// Allocate (or reuse) the canonical type-parameter identity for a JSDoc
    /// `@template` owned by an AST declaration.
    ///
    /// JSDoc template parameters do not have their own parser node, so the
    /// containing class/function/method node is their stable declaration key.
    /// These parameters are stamped as
    /// [`tsz_solver::TypeParamOrigin::JsdocOwnerScoped`]: unlike syntax
    /// declarations, siblings have no individual name nodes and therefore use
    /// `(file, owner_node, name)` as their logical binder identity. Structural
    /// interning then makes repeated construction stable without the fresh-id
    /// declaration cache used by the syntax/lowering convergence path.
    pub(crate) fn intern_jsdoc_type_param_for_owner_stamped(
        &mut self,
        owner_node: NodeIndex,
        info: tsz_solver::TypeParamInfo,
    ) -> (tsz_solver::TypeId, tsz_solver::TypeParamInfo) {
        self.intern_jsdoc_type_param_for_site_stamped(JsdocTypeParamSite::Owner(owner_node.0), info)
    }

    /// Allocate (or reuse) a type parameter declared by one JSDoc comment,
    /// such as a generic `@typedef` or `@callback`.
    pub(crate) fn intern_jsdoc_type_param_for_comment_stamped(
        &mut self,
        comment_pos: u32,
        info: tsz_solver::TypeParamInfo,
    ) -> (tsz_solver::TypeId, tsz_solver::TypeParamInfo) {
        self.intern_jsdoc_type_param_for_site_stamped(
            JsdocTypeParamSite::Comment(comment_pos),
            info,
        )
    }

    /// Cross-arena counterpart of
    /// [`Self::intern_jsdoc_type_param_for_owner_stamped`].
    ///
    /// The declaration belongs to `file_name`, not the current checker arena,
    /// so its file atom is supplied explicitly.
    pub(crate) fn intern_cross_arena_jsdoc_type_param_for_owner_stamped(
        &self,
        file_name: &str,
        owner_node: NodeIndex,
        mut info: tsz_solver::TypeParamInfo,
    ) -> (tsz_solver::TypeId, tsz_solver::TypeParamInfo) {
        let file_atom = self.ctx.types.intern_string(file_name);
        info.origin = tsz_solver::TypeParamOrigin::JsdocOwnerScoped {
            file: file_atom,
            node: owner_node.0,
        };
        let type_id = signature_query::type_param(self.ctx.types, info);
        (type_id, info)
    }

    fn intern_jsdoc_type_param_for_site_stamped(
        &mut self,
        declaration_site: JsdocTypeParamSite,
        mut info: tsz_solver::TypeParamInfo,
    ) -> (tsz_solver::TypeId, tsz_solver::TypeParamInfo) {
        let file = self.ctx.types.intern_string(&self.ctx.file_name);
        info.origin = match declaration_site {
            JsdocTypeParamSite::Owner(node) => {
                tsz_solver::TypeParamOrigin::JsdocOwnerScoped { file, node }
            }
            JsdocTypeParamSite::Comment(pos) => {
                tsz_solver::TypeParamOrigin::JsdocCommentScoped { file, pos }
            }
        };
        let type_id = signature_query::type_param(self.ctx.types, info);
        (type_id, info)
    }
}
