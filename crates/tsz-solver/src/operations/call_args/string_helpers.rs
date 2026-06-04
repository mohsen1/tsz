use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{IntrinsicKind, LiteralValue, ParamInfo, TypeData, TypeId};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn is_string_like_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::String))
                | Some(TypeData::TemplateLiteral(_))
                | Some(TypeData::Literal(LiteralValue::String(_)))
        )
    }

    pub(crate) fn function_signature_is_contextually_sensitive(
        &self,
        params: &[ParamInfo],
    ) -> bool {
        params.iter().any(|param| {
            param.type_id == TypeId::ANY || self.type_uses_inference_placeholders(param.type_id)
        })
    }
}
