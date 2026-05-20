//! Typed result boundary for generic call resolution.
//!
//! `GenericCallResult` carries the call outcome together with the
//! side-channel data produced during resolution: the instantiated type
//! predicate (used for type-guard narrowing) and the instantiated parameter
//! types (used for post-inference excess-property checking). Bundling these
//! into a named result makes the resolver's output explicit and allows callers
//! to avoid reading mutable side-channel fields from `CallEvaluator`.

use crate::operations::CallResult;
use crate::types::{ParamInfo, TypePredicate};

/// The complete output of a generic call resolution request.
///
/// Carries the `CallResult` together with any resolver-side data that callers
/// need after resolution completes. Previously these were exposed as mutable
/// fields on `CallEvaluator`; bundling them here makes the output boundary
/// explicit and easier to extend.
#[derive(Debug)]
pub struct GenericCallResult {
    call_result: CallResult,
    /// Instantiated type predicate, set when the resolved function carries a
    /// type predicate (type guard). Used by the checker for narrowing.
    instantiated_predicate: Option<(TypePredicate, Vec<ParamInfo>)>,
    /// Instantiated parameter types after type-argument inference. The checker
    /// uses these for post-inference excess-property checking on the concrete
    /// parameter types rather than the pre-inference generic parameter types.
    instantiated_params: Option<Vec<ParamInfo>>,
}

impl GenericCallResult {
    pub const fn new(call_result: CallResult) -> Self {
        Self {
            call_result,
            instantiated_predicate: None,
            instantiated_params: None,
        }
    }

    pub fn with_instantiated_predicate(
        mut self,
        predicate: Option<(TypePredicate, Vec<ParamInfo>)>,
    ) -> Self {
        self.instantiated_predicate = predicate;
        self
    }

    pub fn with_instantiated_params(mut self, params: Option<Vec<ParamInfo>>) -> Self {
        self.instantiated_params = params;
        self
    }

    pub fn into_call_result(self) -> CallResult {
        self.call_result
    }

    pub const fn take_instantiated_predicate(&mut self) -> Option<(TypePredicate, Vec<ParamInfo>)> {
        self.instantiated_predicate.take()
    }

    pub const fn take_instantiated_params(&mut self) -> Option<Vec<ParamInfo>> {
        self.instantiated_params.take()
    }
}

#[cfg(test)]
mod tests {
    use super::GenericCallResult;
    use crate::operations::CallResult;
    use crate::types::TypeId;

    #[test]
    fn result_wraps_call_result() {
        let cr = CallResult::Success(TypeId::STRING);
        let result = GenericCallResult::new(cr);
        assert!(matches!(result.into_call_result(), CallResult::Success(t) if t == TypeId::STRING));
    }

    #[test]
    fn result_into_call_result_consumes() {
        let result = GenericCallResult::new(CallResult::Success(TypeId::NUMBER));
        let cr = result.into_call_result();
        assert!(matches!(cr, CallResult::Success(t) if t == TypeId::NUMBER));
    }

    #[test]
    fn result_starts_with_no_side_channel_data() {
        let mut result = GenericCallResult::new(CallResult::Success(TypeId::VOID));
        assert!(result.take_instantiated_predicate().is_none());
        assert!(result.take_instantiated_params().is_none());
    }

    #[test]
    fn result_take_instantiated_params_returns_and_clears() {
        let mut result = GenericCallResult::new(CallResult::Success(TypeId::VOID))
            .with_instantiated_params(Some(vec![]));
        let taken = result.take_instantiated_params();
        assert!(taken.is_some());
        assert!(result.take_instantiated_params().is_none());
    }
}
