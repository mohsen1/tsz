//! Fallback construction for a type-reference name that resolves to no
//! declaration (missing lib type), split out of `core.rs` to keep that file
//! under the architecture guard's line cap.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    pub(crate) fn handle_missing_global_type_with_args(
        &mut self,
        name: &str,
        type_ref: &tsz_parser::parser::node::TypeRefData,
        type_name_idx: NodeIndex,
    ) -> TypeId {
        if self.is_mapped_type_utility(name) {
            if self.ctx.compiler_options.no_lib {
                self.report_missing_lib_type_name(name, type_name_idx);
                return TypeId::ANY;
            }

            if let Some(args) = &type_ref.type_arguments {
                let type_args: Vec<TypeId> = args
                    .nodes
                    .iter()
                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                    .collect();

                if name == "Pick" && type_args.len() == 2 {
                    let factory = self.ctx.types.factory();
                    let key_param = tsz_solver::TypeParamInfo {
                        name: self.ctx.types.intern_string("__pick_key"),
                        constraint: None,
                        default: None,
                        is_const: false,
                        origin: tsz_solver::TypeParamOrigin::User,
                    };
                    let key_type = self.ctx.types.type_param(key_param);
                    return factory.mapped(tsz_solver::MappedType {
                        type_param: key_param,
                        constraint: type_args[1],
                        name_type: None,
                        template: factory.index_access(type_args[0], key_type),
                        readonly_modifier: None,
                        optional_modifier: None,
                    });
                }

                let (base_type, _) = self.resolve_lib_type_with_params(name);
                if let Some(base_type) = base_type {
                    return self.ctx.types.factory().application(base_type, type_args);
                }
            }
            return TypeId::ANY;
        }

        self.report_missing_lib_type_name(name, type_name_idx);

        if !self.ctx.compiler_options.no_lib
            && matches!(name, "Promise" | "PromiseLike")
            && let Some(args) = &type_ref.type_arguments
        {
            let type_args: Vec<TypeId> = args
                .nodes
                .iter()
                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                .collect();
            if !type_args.is_empty() {
                let promise_base = {
                    let lib_binders = self.get_lib_binders();
                    crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
                        name,
                        self.ctx.binder,
                        self.ctx.global_file_locals_index.as_deref(),
                        self.ctx
                            .all_binders
                            .as_ref()
                            .map(|binders| binders.as_ref().as_slice()),
                        &self.ctx.lib_contexts,
                    )
                    .or_else(|| {
                        lib_binders
                            .iter()
                            .find_map(|binder| binder.file_locals.get(name))
                    })
                    .map(|sym_id| {
                        let _ = self.resolve_lib_type_by_name(name);
                        let def_id = self.ctx.get_canonical_lib_def_id(name, sym_id);
                        self.ctx.types.factory().lazy(def_id)
                    })
                    .unwrap_or(TypeId::PROMISE_BASE)
                };
                return self
                    .ctx
                    .types
                    .factory()
                    .application(promise_base, type_args);
            }
        }

        if let Some(args) = &type_ref.type_arguments {
            for &arg_idx in &args.nodes {
                let _ = self.get_type_from_type_node(arg_idx);
            }
        }
        TypeId::ERROR
    }
}
