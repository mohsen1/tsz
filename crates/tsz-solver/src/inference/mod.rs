pub(crate) mod infer;
pub(crate) mod infer_bct;
pub(crate) mod infer_candidate_kinds;
pub(crate) mod infer_matching;
pub(crate) mod infer_matching_tuples;
pub(crate) mod infer_resolve;
pub(crate) mod infer_variance;
mod partially_inferable;
mod template_anchor;
mod template_segment_prefix;

pub(crate) use infer::InferenceContext;

use crate::caches::db::QueryDatabase;
use crate::types::{InferencePriority, TypeId, TypeParamInfo};
use tsz_common::interner::Atom;

/// Infer concrete bindings for a generic signature's type parameters by
/// structurally matching each `(declared parameter type, concrete argument
/// type)` pair, reusing the same inference engine the solver uses for call
/// resolution.
///
/// Only type parameters the engine can resolve from the supplied pairs are
/// returned; parameters with no inference candidate are omitted so the caller
/// can decide how to handle them (e.g. leave a predicate generic or fall back
/// to a default). This is the shared primitive behind type-predicate
/// instantiation when a type parameter appears nested inside a parameter type
/// — a generic alias or wrapper such as `Box<T>` or
/// `MaybeAsync<T> = T | AsyncIterable<T>` — where a direct
/// parameter/type-parameter identity check cannot recover the binding.
pub fn infer_type_arguments_from_param_args(
    db: &dyn QueryDatabase,
    type_params: &[TypeParamInfo],
    param_arg_pairs: &[(TypeId, TypeId)],
) -> Vec<(Atom, TypeId)> {
    if type_params.is_empty() || param_arg_pairs.is_empty() {
        return Vec::new();
    }

    let mut ctx = InferenceContext::with_query_db(db);
    let mut vars = Vec::with_capacity(type_params.len());
    for tp in type_params {
        let var = ctx.fresh_type_param(tp.name, tp.is_const);
        if let Some(constraint) = tp.constraint {
            ctx.set_declared_constraint(var, constraint);
        }
        vars.push((tp.name, var));
    }

    for &(param_ty, arg_ty) in param_arg_pairs {
        // `infer_from_types(source, target)` reads bindings off `target`'s type
        // parameters from the concrete `source`, so the declared parameter type
        // is the target and the argument type is the source.
        let _ = ctx.infer_from_types(arg_ty, param_ty, InferencePriority::NakedTypeVariable);
    }

    // Resolve accumulated inference candidates into concrete bindings before
    // reading them; `probe` only reports a value once the variable is fixed.
    let _ = ctx.fix_current_variables();

    let mut bindings = Vec::new();
    for (name, var) in vars {
        if let Some(ty) = ctx.probe(var) {
            bindings.push((name, ty));
        }
    }
    bindings
}
