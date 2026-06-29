// #14346 stages 2–3: the `evaluate_request_result` boundary turns the
// per-request termination verdict into a typed `EvaluationResult`. These tests
// pin the boundary contract that keeps the channel parity-safe:
//
//   * a request that hit a bail reports `Incomplete { kind, partial }` where
//     `partial` is the opaque, relation-preserving `TypeId` the bail surfaced;
//   * `into_type_id()` returns that same `partial`, so every consumer's collapse
//     — and therefore the emitted type and diagnostics — is byte-identical to a
//     `Complete` result carrying the same `TypeId`;
//   * a request that did not hit a bail still reports `Complete`.
//
// Driving genuine bails through the full evaluator would be slow and fragile, so
// these exercise the boundary's verdict translation directly
// (`TypeEvaluator::request_result_verdict`), which is the exact function
// `evaluate_request_result` uses to choose the arm.

use crate::evaluation::result::Termination;

#[test]
fn iteration_exceeded_request_reports_incomplete_with_partial() {
    // The opaque, relation-preserving partial the bail returns unchanged.
    let partial = TypeId::STRING;
    let result = request_result_verdict(partial, Some(TerminationKind::IterationExceeded));

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
    let result = request_result_verdict(TypeId::NUMBER, None);

    assert!(!result.is_incomplete());
    assert_eq!(result.termination(), Termination::Complete);
    assert_eq!(result.into_type_id(), TypeId::NUMBER);
}

#[test]
fn every_guard_bail_reports_incomplete_with_its_kind_and_partial() {
    // Stage 3: each of the five guard bails surfaces its own kind while keeping
    // the same opaque partial, so `into_type_id` stays byte-identical across the
    // board. The partial varies per case only to prove it is threaded through
    // unchanged, never replaced by a sentinel.
    for (kind, partial) in [
        (TerminationKind::DepthExceeded, TypeId::ERROR),
        (TerminationKind::FuelExhausted, TypeId::ERROR),
        (TerminationKind::SolverStackFrames, TypeId::STRING),
        (TerminationKind::CrossEvalCycle, TypeId::NUMBER),
        (TerminationKind::QueryOpBudget, TypeId::BOOLEAN),
    ] {
        let result = request_result_verdict(partial, Some(kind));

        assert!(result.is_incomplete());
        assert_eq!(
            result.termination(),
            Termination::Incomplete { kind, partial }
        );
        assert_eq!(result.into_type_id(), partial);
        assert!(result.is_identity_for(partial));
    }
}

#[test]
fn reset_clears_typed_and_legacy_termination_state() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.simulate_unrelated_recursion_bail_for_test();
    evaluator.simulate_incomplete_request_verdict_for_test(TerminationKind::QueryOpBudget);
    evaluator.mark_unresolved_def_seen();
    evaluator.app_body_limit_epoch = evaluator.limit_epoch;

    assert!(evaluator.recursion_limit_hit());
    assert!(evaluator.has_incomplete_request_verdict());
    assert!(evaluator.unresolved_def_seen());
    assert_ne!(evaluator.limit_epoch, 0);
    assert_ne!(evaluator.app_body_limit_epoch, 0);

    evaluator.reset();

    assert!(!evaluator.recursion_limit_hit());
    assert!(!evaluator.has_incomplete_request_verdict());
    assert!(!evaluator.unresolved_def_seen());
    assert_eq!(evaluator.limit_epoch, 0);
    assert_eq!(evaluator.app_body_limit_epoch, 0);
}

#[test]
fn depth_marker_for_request_records_depth_verdict() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.mark_depth_exceeded_for_request();

    let result = evaluator.request_result_for_test(TypeId::ERROR);
    assert_eq!(
        result.termination(),
        Termination::Incomplete {
            kind: TerminationKind::DepthExceeded,
            partial: TypeId::ERROR,
        }
    );
    assert_eq!(result.into_type_id(), TypeId::ERROR);
}

#[test]
fn raw_depth_marker_allows_specific_fuel_verdict() {
    let interner = TypeInterner::new();
    let mut evaluator = TypeEvaluator::new(&interner);

    evaluator.mark_depth_exceeded();
    evaluator.note_request_termination(TerminationKind::FuelExhausted);

    let result = evaluator.request_result_for_test(TypeId::ERROR);
    assert_eq!(
        result.termination(),
        Termination::Incomplete {
            kind: TerminationKind::FuelExhausted,
            partial: TypeId::ERROR,
        }
    );
    assert_eq!(result.into_type_id(), TypeId::ERROR);
}
