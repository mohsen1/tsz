impl<'a> ContextualTypeContext<'a> {
    fn is_function_boxed_or_intrinsic(&self, type_id: TypeId) -> bool {
        if matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Function))
        ) {
            return true;
        }
        if self
            .interner
            .get_boxed_type(IntrinsicKind::Function)
            .is_some_and(|boxed| boxed == type_id)
        {
            return true;
        }
        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(type_id)
            && self
                .interner
                .is_boxed_def_id(def_id, IntrinsicKind::Function)
        {
            return true;
        }
        false
    }

    fn apply_conditional_true_branch_param_substitution(
        &self,
        ty: TypeId,
        cond: &crate::types::ConditionalType,
    ) -> TypeId {
        use crate::types::TypeData;
        if ty.is_intrinsic() {
            return ty;
        }
        match self.interner.lookup(ty) {
            Some(TypeData::Function(func_id)) => {
                let mut shape = (*self.interner.function_shape(func_id)).clone();
                for p in &mut shape.params {
                    p.type_id = self.substitute_conditional_param_type(p.type_id, cond);
                }
                self.interner.function(shape)
            }
            Some(TypeData::Callable(callable_id)) => {
                let mut shape = (*self.interner.callable_shape(callable_id)).clone();
                for sig in &mut shape.call_signatures {
                    for p in &mut sig.params {
                        p.type_id = self.substitute_conditional_param_type(p.type_id, cond);
                    }
                }
                for sig in &mut shape.construct_signatures {
                    for p in &mut sig.params {
                        p.type_id = self.substitute_conditional_param_type(p.type_id, cond);
                    }
                }
                self.interner.callable(shape)
            }
            _ => ty,
        }
    }

    fn substitute_conditional_param_type(
        &self,
        param_type: TypeId,
        cond: &crate::types::ConditionalType,
    ) -> TypeId {
        if param_type == cond.check_type {
            self.interner.intersection2(param_type, cond.extends_type)
        } else {
            param_type
        }
    }
}
