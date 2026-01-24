//! Tests for the Tracer Pattern
//!
//! These tests verify that the tracer pattern provides:
//! 1. Zero-cost abstraction for FastTracer
//! 2. Correct diagnostic collection for DiagnosticTracer
//! 3. Identical logic between fast and diagnostic paths

use crate::solver::diagnostics::{SubtypeTracer, FastTracer, DiagnosticTracer, SubtypeFailureReason};
use crate::interner::Atom;
use crate::solver::types::TypeId;

#[test]
fn test_fast_tracer_stops_immediately() {
    let mut tracer = FastTracer;

    // FastTracer should always return false (stop checking)
    let result = tracer.on_mismatch(|| {
        SubtypeFailureReason::TypeMismatch {
            source_type: TypeId::NUMBER,
            target_type: TypeId::STRING,
        }
    });

    assert!(!result, "FastTracer should return false to stop checking");
}

#[test]
fn test_fast_tracer_never_calls_closure() {
    let mut tracer = FastTracer;
    let mut closure_called = false;

    tracer.on_mismatch(|| {
        closure_called = true;
        SubtypeFailureReason::TypeMismatch {
            source_type: TypeId::NUMBER,
            target_type: TypeId::STRING,
        }
    });

    assert!(!closure_called, "FastTracer should never call the closure");
}

#[test]
fn test_diagnostic_tracer_collects_failure() {
    let mut tracer = DiagnosticTracer::new();

    // Collect a failure
    let result = tracer.on_mismatch(|| {
        SubtypeFailureReason::TypeMismatch {
            source_type: TypeId::NUMBER,
            target_type: TypeId::STRING,
        }
    });

    assert!(!result, "DiagnosticTracer should return false to stop checking");
    assert!(tracer.has_failure(), "Should have collected a failure");

    let failure = tracer.take_failure().expect("Should have a failure");
    match failure {
        SubtypeFailureReason::TypeMismatch { source_type, target_type } => {
            assert_eq!(source_type, TypeId::NUMBER);
            assert_eq!(target_type, TypeId::STRING);
        }
        _ => panic!("Wrong failure type"),
    }
}

#[test]
fn test_diagnostic_tracer_only_collects_first_failure() {
    let mut tracer = DiagnosticTracer::new();

    // Collect first failure
    tracer.on_mismatch(|| {
        SubtypeFailureReason::TypeMismatch {
            source_type: TypeId::NUMBER,
            target_type: TypeId::STRING,
        }
    });

    // Try to collect second failure (should be ignored)
    tracer.on_mismatch(|| {
        SubtypeFailureReason::LiteralTypeMismatch {
            source_type: TypeId::TRUE_LITERAL,
            target_type: TypeId::FALSE_LITERAL,
        }
    });

    let failure = tracer.take_failure().expect("Should have a failure");
    match failure {
        SubtypeFailureReason::TypeMismatch { .. } => {
            // Should be the first failure
        }
        _ => panic!("Should have collected the first failure, not the second"),
    }
}

#[test]
fn test_diagnostic_tracer_can_take_failure() {
    let mut tracer = DiagnosticTracer::new();

    tracer.on_mismatch(|| {
        SubtypeFailureReason::MissingProperty {
            property_name: Atom::from("name"),
            source_type: TypeId::ANY,
            target_type: TypeId::OBJECT,
        }
    });

    assert!(tracer.has_failure());

    let failure = tracer.take_failure();
    assert!(failure.is_some(), "Should return Some failure on first take");
    assert!(!tracer.has_failure(), "Should have no failure after take");

    let failure2 = tracer.take_failure();
    assert!(failure2.is_none(), "Should return None on second take");
}

#[test]
fn test_diagnostic_tracer_default() {
    let tracer: DiagnosticTracer = Default::default();
    assert!(!tracer.has_failure(), "Default tracer should have no failure");
}

#[test]
fn test_nested_failure_reasons() {
    let mut tracer = DiagnosticTracer::new();

    let nested_reason = Box::new(SubtypeFailureReason::IntrinsicTypeMismatch {
        source_type: TypeId::NUMBER,
        target_type: TypeId::STRING,
    });

    tracer.on_mismatch(|| {
        SubtypeFailureReason::PropertyTypeMismatch {
            property_name: Atom::from("age"),
            source_property_type: TypeId::STRING,
            target_property_type: TypeId::NUMBER,
            nested_reason: Some(nested_reason),
        }
    });

    let failure = tracer.take_failure().expect("Should have a failure");
    match failure {
        SubtypeFailureReason::PropertyTypeMismatch { nested_reason: n, .. } => {
            assert!(n.is_some(), "Should have nested reason");
        }
        _ => panic!("Wrong failure type"),
    }
}

#[test]
fn test_all_failure_reasons() {
    // Test that all SubtypeFailureReason variants can be collected
    let reasons = vec![
        SubtypeFailureReason::MissingProperty {
            property_name: Atom::from("x"),
            source_type: TypeId::UNDEFINED,
            target_type: TypeId::NUMBER,
        },
        SubtypeFailureReason::OptionalPropertyRequired {
            property_name: Atom::from("y"),
        },
        SubtypeFailureReason::ReadonlyPropertyMismatch {
            property_name: Atom::from("z"),
        },
        SubtypeFailureReason::TooManyParameters {
            source_count: 5,
            target_count: 3,
        },
        SubtypeFailureReason::TupleElementMismatch {
            source_count: 2,
            target_count: 3,
        },
        SubtypeFailureReason::TupleElementTypeMismatch {
            index: 0,
            source_element: TypeId::STRING,
            target_element: TypeId::NUMBER,
        },
        SubtypeFailureReason::ArrayElementMismatch {
            source_element: TypeId::STRING,
            target_element: TypeId::NUMBER,
        },
        SubtypeFailureReason::IndexSignatureMismatch {
            index_kind: "string",
            source_value_type: TypeId::STRING,
            target_value_type: TypeId::NUMBER,
        },
        SubtypeFailureReason::NoCommonProperties {
            source_type: TypeId::ANY,
            target_type: TypeId::NEVER,
        },
        SubtypeFailureReason::IntrinsicTypeMismatch {
            source_type: TypeId::NUMBER,
            target_type: TypeId::STRING,
        },
        SubtypeFailureReason::LiteralTypeMismatch {
            source_type: TypeId::TRUE_LITERAL,
            target_type: TypeId::FALSE_LITERAL,
        },
        SubtypeFailureReason::ErrorType {
            source_type: TypeId::ERROR,
            target_type: TypeId::NUMBER,
        },
    ];

    for reason in reasons {
        let mut tracer = DiagnosticTracer::new();
        let clone_reason = reason.clone();
        tracer.on_mismatch(move || clone_reason);
        assert!(tracer.has_failure());
    }
}

#[test]
fn test_tracer_pattern_with_conditional_check() {
    // Simulate a realistic subtype check with early exit

    fn check_subtype<T: SubtypeTracer>(
        source: TypeId,
        target: TypeId,
        tracer: &mut T,
    ) -> bool {
        // Fast path: same type
        if source == target {
            return true;
        }

        // Simulate type mismatch
        if !tracer.on_mismatch(|| SubtypeFailureReason::TypeMismatch {
            source_type: source,
            target_type: target,
        }) {
            return false;
        }

        // Would continue with more checks here...
        true
    }

    // Test with FastTracer (should be fast)
    let mut fast_tracer = FastTracer;
    let result = check_subtype(TypeId::NUMBER, TypeId::STRING, &mut fast_tracer);
    assert!(!result, "Should fail the check");

    // Test with DiagnosticTracer (should collect details)
    let mut diag_tracer = DiagnosticTracer::new();
    let result = check_subtype(TypeId::NUMBER, TypeId::STRING, &mut diag_tracer);
    assert!(!result, "Should fail the check");

    let failure = diag_tracer.take_failure().expect("Should have failure");
    match failure {
        SubtypeFailureReason::TypeMismatch { source_type, target_type } => {
            assert_eq!(source_type, TypeId::NUMBER);
            assert_eq!(target_type, TypeId::STRING);
        }
        _ => panic!("Wrong failure type"),
    }
}

#[test]
fn test_tracer_prevents_closure_allocation_on_fast_path() {
    // This test verifies that the closure is never called on the fast path
    let mut allocation_count = 0;

    let mut tracer = FastTracer;
    tracer.on_mismatch(|| {
        allocation_count += 1;
        SubtypeFailureReason::TypeMismatch {
            source_type: TypeId::NUMBER,
            target_type: TypeId::STRING,
        }
    });

    assert_eq!(allocation_count, 0, "Closure should not be called on fast path");
}
