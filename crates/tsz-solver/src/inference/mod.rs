pub(crate) mod infer;
pub(crate) mod infer_bct;
mod infer_bct_guard_state;
pub(crate) mod infer_candidate_kinds;
mod infer_candidate_queries;
mod infer_guard_state;
pub(crate) mod infer_matching;
mod infer_matching_guard_state;
mod infer_matching_helpers;
mod infer_matching_structure;
pub(crate) mod infer_matching_tuples;
pub(crate) mod infer_resolve;
mod infer_resolve_fixing;
pub(crate) mod infer_variance;
mod partially_inferable;
pub(crate) mod spread_rest_literals;
mod template_anchor;
mod template_capture_coercion;
mod template_segment_prefix;
pub(crate) mod xarena_base;

pub(crate) use infer::InferenceContext;

use crate::caches::db::QueryDatabase;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::{InferencePriority, TypeId, TypeParamInfo, TypeParamOrigin};
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

    // Alpha-rename the declared type parameters to opaque inference
    // placeholders before matching. The inference engine resolves a
    // `TypeParameter` occurrence to one of our variables purely by *name*
    // (`find_type_param`). If an argument type carries a type parameter that
    // happens to share a declared name — which happens when a contextually
    // typed argument or an inner generic call leaks a synthesized/sibling
    // parameter (e.g. fp-ts `filter`/`partition` rendering `S`/`B` where the
    // declaration uses `A`) — the source-side name aliases our inference
    // variable. The foreign parameter is then recorded as the binding and its
    // wrong name leaks into the resolved predicate target, driving spurious
    // `TS2322`/`TS2345`. Renaming our parameters into the reserved
    // `__infer_pred_*` namespace, which neither user source nor the main
    // inference path ever produces, makes the source side structurally unable
    // to collide with our variables, exactly as the call-resolution inference
    // path disambiguates with `__infer_*` placeholders.
    let interner = db.as_type_database();
    let mut ctx = InferenceContext::with_query_db(db);

    // `forward`: original declared name -> opaque placeholder `TypeParameter`,
    // applied to the declared parameter types so their type-parameter
    // references match our renamed inference variables.
    let mut forward = TypeSubstitution::new();
    // `reverse`: placeholder name -> original declared `TypeParameter`, applied
    // to each resolved binding so any residual placeholder (a higher-order
    // result that mentions a sibling parameter) renders with the real name.
    let mut reverse = TypeSubstitution::new();
    let mut vars = Vec::with_capacity(type_params.len());
    for (i, tp) in type_params.iter().enumerate() {
        // Distinct from the main path's `__infer_{digit}` so the two
        // placeholder namespaces can never alias even if a main-path
        // placeholder were to surface in an argument type.
        let placeholder_atom = interner.intern_string(&format!("__infer_pred_{i}"));
        let placeholder_id = interner.type_param(TypeParamInfo {
            is_const: tp.is_const,
            name: placeholder_atom,
            // The placeholder's constraint is unrenamed, matching the
            // call-resolution path: the constraint participates only as a
            // resolution fallback, not as a structural match target.
            constraint: tp.constraint,
            default: None,
            origin: TypeParamOrigin::InferPlaceholder { id: i as u64 },
        });

        let var = ctx.fresh_type_param(placeholder_atom, tp.is_const);
        if let Some(constraint) = tp.constraint {
            ctx.set_declared_constraint(var, constraint);
        }

        forward.insert(tp.name, placeholder_id);
        reverse.insert(placeholder_atom, interner.type_param(*tp));
        vars.push((tp.name, var));
    }

    for &(param_ty, arg_ty) in param_arg_pairs {
        // `infer_from_types(source, target)` reads bindings off `target`'s type
        // parameters from the concrete `source`, so the declared parameter type
        // (renamed into the placeholder namespace) is the target and the
        // argument type is the source.
        let renamed_param = instantiate_type(interner, param_ty, &forward);
        let _ = ctx.infer_from_types(arg_ty, renamed_param, InferencePriority::NakedTypeVariable);
    }

    // Resolve accumulated inference candidates into concrete bindings before
    // reading them; `probe` only reports a value once the variable is fixed.
    let _ = ctx.fix_current_variables();

    let mut bindings = Vec::new();
    for (name, var) in vars {
        if let Some(ty) = ctx.probe(var) {
            // Map any residual placeholder in the resolved binding back to its
            // original declared name so the predicate target renders correctly.
            bindings.push((name, instantiate_type(interner, ty, &reverse)));
        }
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::infer_type_arguments_from_param_args;
    use crate::intern::TypeInterner;
    use crate::types::{TypeId, TypeParamInfo, TypeParamOrigin};
    use tsz_common::interner::Atom;

    /// A user-written type parameter named `name`, distinguished by `constraint`
    /// so two declarations that share a name still intern to distinct `TypeId`s
    /// (exactly the situation that makes name-based inference matching unsafe).
    fn user_param(name: Atom, constraint: Option<TypeId>) -> TypeParamInfo {
        TypeParamInfo {
            is_const: false,
            name,
            constraint,
            default: None,
            origin: TypeParamOrigin::User,
        }
    }

    /// Baseline: a type parameter nested inside an array parameter is still
    /// recovered after the alpha-rename. `(p: A[]) ~ number[]` infers `A=number`.
    #[test]
    fn infers_nested_parameter_after_alpha_rename() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("A");
        let tp_a = interner.type_param(user_param(a, None));

        let param_ty = interner.array(tp_a);
        let arg_ty = interner.array(TypeId::NUMBER);

        let bindings = infer_type_arguments_from_param_args(
            &interner,
            &[user_param(a, None)],
            &[(param_ty, arg_ty)],
        );

        assert_eq!(bindings, vec![(a, TypeId::NUMBER)]);
    }

    /// A type parameter that appears only in the *argument* (source) position,
    /// sharing a declared parameter's name, must not be treated as that
    /// inference variable. Matching the concrete target `string` against a
    /// source `A` previously wired the foreign `A` to our variable as a spurious
    /// upper bound (the fp-ts name leak); now it contributes nothing.
    #[test]
    fn same_named_source_parameter_does_not_drive_inference() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("A");
        // A distinct declaration of `A` (constrained) standing in for the
        // leaked argument-side parameter.
        let foreign_a = interner.type_param(user_param(a, Some(TypeId::BOOLEAN)));

        let bindings = infer_type_arguments_from_param_args(
            &interner,
            &[user_param(a, None)],
            &[(TypeId::STRING, foreign_a)],
        );

        assert!(
            bindings.is_empty(),
            "a same-named source-only parameter must not let us infer the declared parameter, got {bindings:?}"
        );
    }

    /// A legitimate inference must survive a same-named source parameter in
    /// another pair: `(p: A) ~ number` binds `A=number`, and a second pair whose
    /// source is a foreign `A` must not perturb that binding. Under the
    /// name-collision bug the foreign `A` injected a `string` upper bound that
    /// corrupted the result.
    #[test]
    fn legit_inference_unperturbed_by_same_named_source_parameter() {
        let interner = TypeInterner::new();
        let a = interner.intern_string("A");
        let tp_a = interner.type_param(user_param(a, None));
        let foreign_a = interner.type_param(user_param(a, Some(TypeId::BOOLEAN)));

        let bindings = infer_type_arguments_from_param_args(
            &interner,
            &[user_param(a, None)],
            &[(tp_a, TypeId::NUMBER), (TypeId::STRING, foreign_a)],
        );

        assert_eq!(bindings, vec![(a, TypeId::NUMBER)]);
    }
}
