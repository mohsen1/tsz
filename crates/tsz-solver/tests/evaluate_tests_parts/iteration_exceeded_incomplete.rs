// #14346 stage 2: the iteration-limit bail (`RecursionResult::IterationExceeded`)
// is the first real producer of `Termination::Incomplete`. These tests pin the
// boundary contract that keeps the channel parity-safe:
//
//   * a request that hit the iteration bail reports
//     `Incomplete { kind: IterationExceeded, partial }` where `partial` is the
//     opaque, relation-preserving `TypeId` the bail surfaced;
//   * `into_type_id()` returns that same `partial`, so every consumer's collapse
//     — and therefore the emitted type and diagnostics — is byte-identical to a
//     `Complete` result carrying the same `TypeId`;
//   * a request that did not hit the bail still reports `Complete`.
//
// Driving a genuine >100k-iteration bail through the full evaluator would be slow
// and fragile, so these exercise the boundary's verdict translation directly
// (`TypeEvaluator::request_result_verdict`), which is the exact function
// `evaluate_request_result` uses to choose the arm.

use crate::evaluation::result::Termination;

#[test]
fn iteration_exceeded_request_reports_incomplete_with_partial() {
    // The opaque, relation-preserving partial the bail returns unchanged.
    let partial = TypeId::STRING;
    let result = request_result_verdict(partial, true);

    assert!(result.is_incomplete());
    assert_eq!(
        result.termination(),
        Termination::Incomplete {
            kind: TerminationKind::IterationExceeded,
            partial,
        }
    );
    // Byte-identical collapse: consumers see exactly the opaque partial.
    assert_eq!(result.into_type_id(), partial);
    assert!(result.is_identity_for(partial));
}

#[test]
fn non_bailed_request_reports_complete() {
    let result = request_result_verdict(TypeId::NUMBER, false);

    assert!(!result.is_incomplete());
    assert_eq!(result.termination(), Termination::Complete);
    assert_eq!(result.into_type_id(), TypeId::NUMBER);
}
