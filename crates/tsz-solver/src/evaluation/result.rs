//! Typed result boundary for type evaluation.
//!
//! The evaluator still returns a `TypeId` today, but this wrapper names the
//! result stage so future cache/provenance metadata can be attached without
//! threading loose tuples through the evaluation engine.

use crate::types::TypeId;

/// The normalized output of an evaluation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationResult {
    type_id: TypeId,
}

impl EvaluationResult {
    pub const fn new(type_id: TypeId) -> Self {
        Self { type_id }
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    pub const fn into_type_id(self) -> TypeId {
        self.type_id
    }

    pub fn is_identity_for(self, input: TypeId) -> bool {
        self.type_id == input
    }
}

#[cfg(test)]
mod tests {
    use super::EvaluationResult;
    use crate::types::TypeId;

    #[test]
    fn result_wraps_evaluated_type_id() {
        let result = EvaluationResult::new(TypeId::STRING);

        assert_eq!(result.type_id(), TypeId::STRING);
        assert_eq!(result.into_type_id(), TypeId::STRING);
        assert!(result.is_identity_for(TypeId::STRING));
        assert!(!result.is_identity_for(TypeId::NUMBER));
    }
}
