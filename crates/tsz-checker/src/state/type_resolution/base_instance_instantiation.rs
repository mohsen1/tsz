//! Instantiation of a base class instance type with a derived class's heritage
//! type arguments, including the mint-prevention deferral that keeps an
//! in-progress base reference lazy instead of baking the `error` cycle sentinel
//! into inherited members (#13044/#13484).

use crate::query_boundaries::common::TypeSubstitution;
use crate::state::CheckerState;
use tsz_parser::parser::NodeList;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn instantiate_base_instance_type_with_args(
        &mut self,
        base_instance_type: TypeId,
        base_type_params: &[tsz_solver::TypeParamInfo],
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        self.instantiate_base_instance_type_with_args_deferrable(
            base_instance_type,
            base_type_params,
            type_arguments,
            None,
        )
    }

    /// As [`Self::instantiate_base_instance_type_with_args`], but with an
    /// optional `deferral_def_id` identifying the base class definition so the
    /// instantiation can be kept deferred as `Application(Lazy(base_def), args)`
    /// when the base instance is the in-progress class-instance cycle sentinel.
    pub(super) fn instantiate_base_instance_type_with_args_deferrable(
        &mut self,
        base_instance_type: TypeId,
        base_type_params: &[tsz_solver::TypeParamInfo],
        type_arguments: Option<&NodeList>,
        deferral_def_id: Option<tsz_solver::DefId>,
    ) -> TypeId {
        if type_arguments.is_none() || base_type_params.is_empty() {
            return self.resolve_lazy_type(base_instance_type);
        }

        let mut type_args = Vec::with_capacity(type_arguments.map_or(0, |args| args.nodes.len()));
        if let Some(args) = type_arguments {
            for &arg_idx in &args.nodes {
                type_args.push(self.get_type_from_type_node(arg_idx));
            }
        }

        if type_args.is_empty() {
            return self.resolve_lazy_type(base_instance_type);
        }

        // Mint-prevention for the cross-arena base-class poison cycle (#13044/#13484).
        //
        // When a generic base class is resolved transitively while a derived
        // subclass chain is mid-resolution (e.g. `ControlledTransaction extends
        // Transaction extends Kysely extends QueryCreator<DB>`), the base's
        // instance type can resolve to the in-progress class-instance cycle
        // sentinel `TypeId::ERROR`. Substituting the derived class's type
        // arguments into that sentinel collapses every free base type parameter
        // (`DB`) into `error` and bakes `error` into the inherited member types
        // (`selectFrom(): SelectFrom<error, ...>`). That poisoned build wins the
        // last-write-wins type environment, so derived classes inherit the
        // spurious `error` even though the base ultimately builds clean.
        //
        // `tsc` binds base->derived type parameters order-independently and never
        // collapses a free type parameter to an error sentinel. Mirror that: when
        // the base reference resolves to the cycle sentinel, keep the application
        // deferred as `Application(Lazy(base_def), args)`. The base body is not
        // resolved against the in-progress class; it resolves later once the base
        // class instance is complete, binding `DB` to the real type argument. The
        // base definition is recovered from a deferrable `Lazy(DefId)` reference
        // or from the caller-supplied `deferral_def_id` (which covers the case
        // where the base instance is already the raw `TypeId::ERROR` sentinel,
        // carrying no `Lazy` to recover the definition from).
        let resolved_base = self.resolve_lazy_type(base_instance_type);
        if resolved_base == TypeId::ERROR
            && let Some(base_def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, base_instance_type)
                    .or(deferral_def_id)
        {
            let lazy_base = self.ctx.types.lazy(base_def_id);
            return self.ctx.types.application(lazy_base, type_args);
        }

        let base_instance_type = resolved_base;
        if type_args.len() < base_type_params.len() {
            for (param_index, param) in base_type_params.iter().enumerate().skip(type_args.len()) {
                let fallback = param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN);
                let substitution = TypeSubstitution::from_args(
                    self.ctx.types,
                    &base_type_params[..param_index],
                    &type_args,
                );
                type_args.push(
                    crate::query_boundaries::common::instantiate_type_preserving_meta(
                        self.ctx.types,
                        fallback,
                        &substitution,
                    ),
                );
            }
        }
        if type_args.len() > base_type_params.len() {
            type_args.truncate(base_type_params.len());
        }

        let substitution =
            TypeSubstitution::from_args(self.ctx.types, base_type_params, &type_args);
        crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            base_instance_type,
            &substitution,
        )
    }
}
