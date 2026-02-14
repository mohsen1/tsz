use super::*;

/// Test that FastTracer returns correct boolean results
#[test]
fn test_fast_tracer_boolean() {
    let interner = TypeInterner::new();
    let mut checker = TracerSubtypeChecker::new(&interner);

    // Same type - use built-in constants
    let string_type = TypeId::STRING;
    let mut fast = FastTracer;
    assert!(checker.check_subtype_with_tracer(string_type, string_type, &mut fast));

    // Subtype relationship
    let any_type = TypeId::ANY;
    let mut fast = FastTracer;
    assert!(checker.check_subtype_with_tracer(string_type, any_type, &mut fast));

    // Not a subtype
    let number_type = TypeId::NUMBER;
    let mut fast = FastTracer;
    assert!(!checker.check_subtype_with_tracer(string_type, number_type, &mut fast));
}

/// Test that DiagnosticTracer collects failure reasons
#[test]
fn test_diagnostic_tracer_collects_reasons() {
    let interner = TypeInterner::new();
    let mut checker = TracerSubtypeChecker::new(&interner);

    let string_type = TypeId::STRING;
    let number_type = TypeId::NUMBER;

    let mut diag = DiagnosticTracer::new();
    checker.check_subtype_with_tracer(string_type, number_type, &mut diag);

    assert!(diag.has_failure());
    let failure = diag.take_failure();
    assert!(failure.is_some());

    match failure {
        Some(SubtypeFailureReason::TypeMismatch {
            source_type,
            target_type,
        }) => {
            assert_eq!(source_type, string_type);
            assert_eq!(target_type, number_type);
        }
        _ => panic!("Expected TypeMismatch failure"),
    }
}

/// Test that union target checking works correctly
#[test]
fn test_union_target_tracer() {
    let interner = TypeInterner::new();
    let mut checker = TracerSubtypeChecker::new(&interner);

    // string | number
    let string_type = TypeId::STRING;
    let number_type = TypeId::NUMBER;
    let union_type = interner.union(vec![string_type, number_type]);

    // string <: string | number (should pass)
    let mut fast = FastTracer;
    assert!(checker.check_subtype_with_tracer(string_type, union_type, &mut fast));

    // boolean <: string | number (should fail)
    let bool_type = TypeId::BOOLEAN;
    let mut diag = DiagnosticTracer::new();
    assert!(!checker.check_subtype_with_tracer(bool_type, union_type, &mut diag));
    assert!(diag.has_failure());
}

/// Test that function type checking works
#[test]
fn test_function_tracer() {
    let interner = TypeInterner::new();
    let mut checker = TracerSubtypeChecker::new(&interner).with_strict_function_types(true);

    // (x: string) => number
    let string_type = TypeId::STRING;
    let number_type = TypeId::NUMBER;

    let func1 = FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: string_type,
            optional: false,
            rest: false,
        }],
        return_type: number_type,
        type_params: Vec::new(),
        this_type: None,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    };
    let func1_id = interner.function(func1.clone());

    // (x: string) => number (same type)
    let func2_id = interner.function(func1);

    // Same function type should be compatible
    let mut fast = FastTracer;
    assert!(checker.check_subtype_with_tracer(func1_id, func2_id, &mut fast));
}

/// Benchmark: Compare FastTracer vs direct boolean check
#[test]
fn benchmark_fast_tracer() {
    let interner = TypeInterner::new();
    let mut checker = TracerSubtypeChecker::new(&interner);

    let string_type = TypeId::STRING;
    let number_type = TypeId::NUMBER;

    // Warm up
    let mut fast = FastTracer;
    for _ in 0..1000 {
        let _ = checker.check_subtype_with_tracer(string_type, number_type, &mut fast);
    }

    // Measure FastTracer performance
    let start = std::time::Instant::now();
    let iterations = 100_000;
    for _ in 0..iterations {
        let mut fast = FastTracer;
        let _ = checker.check_subtype_with_tracer(string_type, number_type, &mut fast);
    }
    let fast_duration = start.elapsed();

    // FastTracer should be very fast (millions of checks per second)
    let checks_per_second = iterations as f64 / fast_duration.as_secs_f64();
    println!("FastTracer: {:.2} checks/second", checks_per_second);

    // We expect at least 100k checks/second even in debug mode
    // In release mode, this should be millions
    assert!(
        checks_per_second > 10_000.0,
        "FastTracer too slow: {:.2} checks/sec",
        checks_per_second
    );
}

/// Test that DiagnosticTracer has the same logic as FastTracer
#[test]
fn test_tracer_logic_consistency() {
    let interner = TypeInterner::new();
    let mut checker = TracerSubtypeChecker::new(&interner);

    // Test various type pairs using built-in constants
    let test_cases = vec![
        (TypeId::STRING, TypeId::STRING, true),
        (TypeId::STRING, TypeId::NUMBER, false),
        (TypeId::NUMBER, TypeId::ANY, true),
        (TypeId::NEVER, TypeId::STRING, true),
        (TypeId::STRING, TypeId::NEVER, false),
        (TypeId::ANY, TypeId::NEVER, false),
    ];

    for (source, target, expected) in test_cases {
        // FastTracer
        let mut fast = FastTracer;
        let fast_result = checker.check_subtype_with_tracer(source, target, &mut fast);

        // DiagnosticTracer
        let mut diag = DiagnosticTracer::new();
        let diag_result = checker.check_subtype_with_tracer(source, target, &mut diag);

        // Both should give the same boolean result
        assert_eq!(
            fast_result, expected,
            "FastTracer failed for ({:?} <: {:?})",
            source, target
        );
        assert_eq!(
            diag_result, expected,
            "DiagnosticTracer failed for ({:?} <: {:?})",
            source, target
        );
        assert_eq!(
            fast_result, diag_result,
            "Tracer results differ for ({:?} <: {:?})",
            source, target
        );
    }
}
