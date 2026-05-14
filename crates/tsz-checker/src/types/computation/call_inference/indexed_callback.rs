use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_common::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_solver::{FunctionShape, TypeId};

impl<'a> CheckerState<'a> {
    pub(crate) fn direct_round1_literal_index_key_type_params(
        &mut self,
        shape: &FunctionShape,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        sensitive_args: &[bool],
    ) -> crate::query_boundaries::common::TypeSubstitution {
        let mut preserved = crate::query_boundaries::common::TypeSubstitution::new();

        for (i, &arg_type) in arg_types.iter().enumerate() {
            if sensitive_args.get(i).copied().unwrap_or(false) {
                continue;
            }

            let Some(param) = shape.params.get(i) else {
                continue;
            };
            if param.rest || arg_type.is_any_unknown_or_error() {
                continue;
            }
            let Some(tp_info) = common::type_param_info(self.ctx.types, param.type_id) else {
                continue;
            };
            if !shape.type_params.iter().any(|tp| tp.name == tp_info.name) {
                continue;
            }
            if !type_param_feeds_sensitive_indexed_callback_param(
                self.ctx.types,
                shape,
                sensitive_args,
                tp_info.name,
            ) {
                continue;
            }

            let literal_arg_type = args
                .get(i)
                .and_then(|&arg_idx| self.literal_type_from_initializer(arg_idx))
                .unwrap_or(arg_type);
            if common::widen_literal_type(self.ctx.types, literal_arg_type) == literal_arg_type {
                continue;
            }
            preserved.insert(tp_info.name, literal_arg_type);
        }

        preserved
    }
}

fn type_param_feeds_sensitive_indexed_callback_param(
    db: &dyn tsz_solver::TypeDatabase,
    shape: &FunctionShape,
    sensitive_args: &[bool],
    type_param_name: Atom,
) -> bool {
    shape.params.iter().enumerate().any(|(i, param)| {
        sensitive_args.get(i).copied().unwrap_or(false)
            && type_contains_index_access_indexed_by_param(db, param.type_id, type_param_name, 8)
    })
}

fn type_contains_index_access_indexed_by_param(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
    type_param_name: Atom,
    depth: usize,
) -> bool {
    if depth == 0 || type_id.is_any_unknown_or_error() {
        return false;
    }

    if let Some((object, index)) =
        crate::query_boundaries::checkers::generic::index_access_components(db, type_id)
    {
        return common::contains_type_parameter_named(db, index, type_param_name)
            || type_contains_index_access_indexed_by_param(db, object, type_param_name, depth - 1)
            || type_contains_index_access_indexed_by_param(db, index, type_param_name, depth - 1);
    }

    if let Some(shape) = common::function_shape_for_type(db, type_id) {
        return shape.params.iter().any(|param| {
            type_contains_index_access_indexed_by_param(
                db,
                param.type_id,
                type_param_name,
                depth - 1,
            )
        }) || type_contains_index_access_indexed_by_param(
            db,
            shape.return_type,
            type_param_name,
            depth - 1,
        );
    }

    match common::classify_for_traversal(db, type_id) {
        common::TypeTraversalKind::Application { base, args, .. } => {
            type_contains_index_access_indexed_by_param(db, base, type_param_name, depth - 1)
                || args.into_iter().any(|arg| {
                    type_contains_index_access_indexed_by_param(db, arg, type_param_name, depth - 1)
                })
        }
        common::TypeTraversalKind::TypeParameter {
            constraint,
            default,
        } => constraint.into_iter().chain(default).any(|ty| {
            type_contains_index_access_indexed_by_param(db, ty, type_param_name, depth - 1)
        }),
        common::TypeTraversalKind::Members(members)
        | common::TypeTraversalKind::TemplateLiteral(members) => {
            members.into_iter().any(|member| {
                type_contains_index_access_indexed_by_param(db, member, type_param_name, depth - 1)
            })
        }
        common::TypeTraversalKind::Array(inner)
        | common::TypeTraversalKind::Readonly(inner)
        | common::TypeTraversalKind::KeyOf(inner)
        | common::TypeTraversalKind::StringIntrinsic(inner) => {
            type_contains_index_access_indexed_by_param(db, inner, type_param_name, depth - 1)
        }
        common::TypeTraversalKind::Tuple(elements_id) => {
            db.tuple_list(elements_id).iter().any(|element| {
                type_contains_index_access_indexed_by_param(
                    db,
                    element.type_id,
                    type_param_name,
                    depth - 1,
                )
            })
        }
        _ => false,
    }
}
