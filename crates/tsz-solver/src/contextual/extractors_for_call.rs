//! [`ParameterForCallExtractor`]: the call-site variant of the parameter-type
//! extractors in [`super::extractors`], split into its own file so that file
//! stays under the architecture size ratchet.

use super::extractors::{
    collect_single_or_union_no_reduce, extract_param_type_at_for_call, type_parameters_identical,
};
use crate::construction::TypeDatabase;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::{
    CallSignature, CallableShapeId, FunctionShapeId, IntrinsicKind, LiteralValue, ParamInfo,
    TypeId, TypeListId, TypeParamInfo,
};
use crate::visitor::TypeVisitor;

/// When two or more overloaded call signatures are all generic and share the
/// same type-parameter arity, `tsc` compares/merges them as if the later
/// signatures' type parameters were the same binders as the first signature's
/// (its `createTypeMapper`/`combineIntersectionParameters` machinery). Without
/// this, contextually typing a parameter shared across such overloads unions
/// each overload's own, distinct type-parameter identity together (`T | T`
/// display) instead of collapsing to the single shared `T`, which then fails
/// downstream relation checks that a real bare `T` would pass.
///
/// Returns `sig`'s params unchanged when its type-parameter list doesn't match
/// `canonical` in length (mismatched arity isn't this shared-identity case) or
/// is empty (nothing to map).
fn signature_params_mapped_to_canonical(
    db: &dyn TypeDatabase,
    canonical: &[TypeParamInfo],
    sig: &CallSignature,
) -> Vec<ParamInfo> {
    if sig.type_params.is_empty() || sig.type_params.len() != canonical.len() {
        return sig.params.clone();
    }
    let mut sub = TypeSubstitution::for_signature_domain(&sig.type_params);
    for (tp, canon) in sig.type_params.iter().zip(canonical.iter()) {
        sub.insert(tp.name, db.type_param(*canon));
    }
    sig.params
        .iter()
        .map(|p| ParamInfo {
            type_id: instantiate_type(db, p.type_id, &sub),
            ..*p
        })
        .collect()
}

/// Visitor to extract parameter type from callable types for a call site.
/// Filters signatures by arity (`arg_count`) to handle overloaded functions.
pub(crate) struct ParameterForCallExtractor<'a> {
    db: &'a dyn TypeDatabase,
    index: usize,
    arg_count: usize,
    no_implicit_any: bool,
}

impl<'a> ParameterForCallExtractor<'a> {
    pub(crate) fn new(
        db: &'a dyn TypeDatabase,
        index: usize,
        arg_count: usize,
        no_implicit_any: bool,
    ) -> Self {
        Self {
            db,
            index,
            arg_count,
            no_implicit_any,
        }
    }

    pub(crate) fn extract(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.visit_type(self.db, type_id)
    }

    fn signature_accepts_arg_count(&self, params: &[ParamInfo], arg_count: usize) -> bool {
        // Count required parameters. A rest parameter (`...a: T[]`) requires zero
        // arguments, so it must never count toward the minimum arity — otherwise a
        // call that omits the rest (e.g. `g(5)` for `g(e: 5, ...a: any[])`) is
        // wrongly rejected, dropping contextual typing for the fixed params and
        // widening fresh literal arguments.
        let required_count = params.iter().filter(|p| !p.optional && !p.rest).count();

        // Check if there's a rest parameter
        let has_rest = params.iter().any(|p| p.rest);

        if has_rest {
            // With rest parameter: arity must be >= required_count
            arg_count >= required_count
        } else {
            // Without rest parameter: arity must be within [required_count, total_count]
            arg_count >= required_count && arg_count <= params.len()
        }
    }
}

impl TypeVisitor for ParameterForCallExtractor<'_> {
    type Output = Option<TypeId>;

    fn visit_intrinsic(&mut self, _kind: IntrinsicKind) -> Self::Output {
        None
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        None
    }

    fn visit_function(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.function_shape(FunctionShapeId(shape_id));

        if !self.signature_accepts_arg_count(&shape.params, self.arg_count) {
            return None;
        }

        extract_param_type_at_for_call(self.db, &shape.params, self.index, self.arg_count)
    }

    fn visit_callable(&mut self, shape_id: u32) -> Self::Output {
        let shape = self.db.callable_shape(CallableShapeId(shape_id));

        let mut matched = false;
        let mut param_types: Vec<TypeId> = Vec::new();

        let mut matching_call_signatures: Vec<_> = shape
            .call_signatures
            .iter()
            .filter(|sig| self.signature_accepts_arg_count(&sig.params, self.arg_count))
            .collect();
        if matching_call_signatures
            .iter()
            .any(|sig| !sig.params.last().is_some_and(|param| param.rest))
        {
            matching_call_signatures
                .retain(|sig| !sig.params.last().is_some_and(|param| param.rest));
        }

        // Same `noImplicitAny` gate as `ParameterExtractor::visit_callable`
        // (oracle-verified against `typescript@7.0.2`): an overloaded
        // callable never contextually types a parameter under non-strict.
        if matching_call_signatures.len() > 1 && !self.no_implicit_any {
            return None;
        }

        // tsc's `getIntersectedSignatures` (checker.ts) combines two or more
        // arity-applicable signatures only when their type parameters are
        // *identical* (`compareTypeParametersIdentical`): equal arity and, after
        // positional remapping, identical constraints. A differing arity — which
        // includes any mix of generic and non-generic overloads — or a differing
        // constraint yields no combined signature, so the callable contextually
        // types nothing here (the parameter falls back to implicit `any`).
        let canonical_type_params = matching_call_signatures
            .first()
            .map(|sig| sig.type_params.clone())
            .unwrap_or_default();
        if matching_call_signatures.len() > 1
            && !matching_call_signatures[1..].iter().all(|sig| {
                type_parameters_identical(self.db, &canonical_type_params, &sig.type_params)
            })
        {
            return None;
        }

        // When the signatures ARE combined, tsc maps each later signature's own
        // type parameters onto the first signature's before unioning parameter
        // positions (`createTypeMapper` + `combineIntersectionParameters`). Two
        // overloads that each declare their own `<T>` therefore collapse to a
        // single `T` at the shared position instead of an undeduped `T | T`.
        for sig in matching_call_signatures {
            matched = true;
            let mapped_params =
                signature_params_mapped_to_canonical(self.db, &canonical_type_params, sig);
            if let Some(param_type) =
                extract_param_type_at_for_call(self.db, &mapped_params, self.index, self.arg_count)
            {
                param_types.push(param_type);
            }
        }

        if param_types.is_empty() && !matched {
            param_types = shape
                .call_signatures
                .iter()
                .filter_map(|sig| {
                    extract_param_type_at_for_call(self.db, &sig.params, self.index, self.arg_count)
                })
                .collect();
        }

        // If no call signatures matched, check construct signatures.
        // This handles super() calls and new expressions where the callee
        // is a Callable with construct signatures (not call signatures).
        // NOTE: Generic construct signatures still provide useful contextual
        // types for callback arguments (possibly involving type parameters),
        // and suppressing them causes false TS7006 in constructor calls.
        if param_types.is_empty() {
            matched = false;
            let mut matching_construct_signatures: Vec<_> = shape
                .construct_signatures
                .iter()
                .filter(|sig| self.signature_accepts_arg_count(&sig.params, self.arg_count))
                .collect();
            if matching_construct_signatures
                .iter()
                .any(|sig| !sig.params.last().is_some_and(|param| param.rest))
            {
                matching_construct_signatures
                    .retain(|sig| !sig.params.last().is_some_and(|param| param.rest));
            }
            for sig in matching_construct_signatures {
                matched = true;
                if let Some(param_type) =
                    extract_param_type_at_for_call(self.db, &sig.params, self.index, self.arg_count)
                {
                    param_types.push(param_type);
                }
            }
            if param_types.is_empty() && !matched {
                param_types = shape
                    .construct_signatures
                    .iter()
                    .filter_map(|sig| {
                        extract_param_type_at_for_call(
                            self.db,
                            &sig.params,
                            self.index,
                            self.arg_count,
                        )
                    })
                    .collect();
            }
        }

        // Avoid contextual-type poisoning from catch-all `any` signatures
        // (e.g. implementation signatures like `(...args: any[])` on overloaded
        // constructors). If at least one non-`any` contextual type exists, prefer
        // those and drop `any` contributors.
        if param_types.len() > 1 {
            let has_non_any = param_types.iter().any(|&ty| ty != TypeId::ANY);
            if has_non_any {
                param_types.retain(|&ty| ty != TypeId::ANY);
            }
        }

        collect_single_or_union_no_reduce(self.db, param_types)
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        // For unions, extract parameter types from each member and combine.
        // Use no-reduce union to preserve all callback type variants — see
        // collect_single_or_union_no_reduce doc comment for rationale.
        let members = self.db.type_list(TypeListId(list_id));
        let types: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let mut extractor = ParameterForCallExtractor::new(
                    self.db,
                    self.index,
                    self.arg_count,
                    self.no_implicit_any,
                );
                extractor.extract(member)
            })
            .collect();
        collect_single_or_union_no_reduce(self.db, types)
    }

    fn default_output() -> Self::Output {
        None
    }
}
