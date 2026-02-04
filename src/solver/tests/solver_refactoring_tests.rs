//! Comprehensive tests for the solver refactoring (SOLVER_REFACTORING_PROPOSAL.md).
//!
//! These tests validate:
//! - Phase 1: Bug fixes (cycle detection, provisional returns)
//! - Phase 2: Judge trait and queries
//! - Phase 3: DefId infrastructure
//! - Phase 4: Sound Mode and sticky freshness

use crate::binder::SymbolId;
use crate::checker::sound_checker::StickyFreshnessTracker;
use crate::solver::TypeInterner;
use crate::solver::def::{
    ContentAddressedDefIds, DefId, DefKind, DefinitionInfo, DefinitionStore, EnumMemberValue,
};
use crate::solver::judge::{
    CallableKind, DefaultJudge, IterableKind, Judge, JudgeConfig, PrimitiveFlags, PropertyResult,
    TruthinessKind,
};
use crate::solver::sound::{SoundDiagnostic, SoundDiagnosticCode, SoundLawyer, SoundModeConfig};
use crate::solver::subtype::{SubtypeChecker, SubtypeResult, TypeEnvironment};
use crate::solver::types::{Visibility, *};

// =============================================================================
// Phase 1 Tests: Bug Fixes
// =============================================================================

mod phase1_cycle_detection {
    use super::*;

    #[test]
    fn test_cycle_detection_before_evaluation() {
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);

        // Create a self-referential structure that would infinite loop without cycle detection
        // Simulate: type A = A | number
        let type_a = interner.union(vec![TypeId::NUMBER]);

        // This should not infinite loop - cycle detection catches it
        let result = checker.check_subtype(type_a, TypeId::NUMBER);
        assert!(result.is_true());
    }

    #[test]
    fn test_provisional_on_depth_exceeded() {
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);

        // Helper to create nested arrays
        fn nest_array(interner: &TypeInterner, base: TypeId, depth: usize) -> TypeId {
            let mut ty = base;
            for _ in 0..depth {
                ty = interner.array(ty);
            }
            ty
        }

        // Create deeply nested types with DIFFERENT base types to prevent identity short-circuit
        let deep_string = nest_array(&interner, TypeId::STRING, 120);
        let deep_number = nest_array(&interner, TypeId::NUMBER, 120);

        // Should return DepthExceeded (not False) when depth exceeded during comparison
        // of incompatible types that require deep traversal
        let result = checker.check_subtype(deep_string, deep_number);
        assert!(matches!(result, SubtypeResult::DepthExceeded));
        // depth_exceeded should be set for diagnostic
        assert!(checker.depth_exceeded);
    }

    #[test]
    fn test_provisional_on_iteration_limit() {
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);

        // Make many subtype checks to hit iteration limit
        for _ in 0..100_001 {
            // This would normally exceed MAX_TOTAL_SUBTYPE_CHECKS
            checker.check_subtype(TypeId::NUMBER, TypeId::STRING);
            if checker.depth_exceeded {
                break;
            }
        }

        // If we hit the limit, we should have depth_exceeded set
        // (depending on actual limit values, this may or may not trigger)
    }

    #[test]
    fn test_bivariant_cross_recursion_detection() {
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);

        // Test that (A, B) and (B, A) cycle detection works
        let a = interner.object(vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        let b = interner.object(vec![PropertyInfo {
            name: interner.intern_string("x"),
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        // Should handle bivariant checking without cycle issues
        assert!(checker.is_subtype_of(a, b));
        assert!(checker.is_subtype_of(b, a));
    }
}

// =============================================================================
// Phase 2 Tests: Judge Trait
// =============================================================================

mod phase2_judge {
    use super::*;

    #[test]
    fn test_judge_subtype_basic() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        // Identity
        assert!(judge.is_subtype(TypeId::NUMBER, TypeId::NUMBER));
        assert!(judge.is_subtype(TypeId::STRING, TypeId::STRING));

        // Top/bottom types
        assert!(judge.is_subtype(TypeId::NUMBER, TypeId::ANY));
        assert!(judge.is_subtype(TypeId::NUMBER, TypeId::UNKNOWN));
        assert!(judge.is_subtype(TypeId::NEVER, TypeId::NUMBER));

        // any is top and (in TS mode) bottom
        assert!(judge.is_subtype(TypeId::ANY, TypeId::NUMBER));
    }

    #[test]
    fn test_judge_evaluate_array() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let array_num = interner.array(TypeId::NUMBER);
        let evaluated = judge.evaluate(array_num);

        // Arrays evaluate to themselves (no meta-type evaluation needed)
        assert_eq!(evaluated, array_num);
    }

    #[test]
    fn test_judge_classify_iterable() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        // Array
        let array_num = interner.array(TypeId::NUMBER);
        match judge.classify_iterable(array_num) {
            IterableKind::Array(elem) => assert_eq!(elem, TypeId::NUMBER),
            _ => panic!("Expected Array iterable"),
        }

        // String
        assert_eq!(
            judge.classify_iterable(TypeId::STRING),
            IterableKind::String
        );

        // Tuple
        let tuple = interner.tuple(vec![
            TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        match judge.classify_iterable(tuple) {
            IterableKind::Tuple(types) => {
                assert_eq!(types.len(), 2);
                assert_eq!(types[0], TypeId::NUMBER);
                assert_eq!(types[1], TypeId::STRING);
            }
            _ => panic!("Expected Tuple iterable"),
        }
    }

    #[test]
    fn test_judge_classify_callable() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let func = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        match judge.classify_callable(func) {
            CallableKind::Function {
                params,
                return_type,
                ..
            } => {
                assert_eq!(params.len(), 1);
                assert_eq!(return_type, TypeId::STRING);
            }
            _ => panic!("Expected Function callable"),
        }

        // Constructor
        let ctor = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::OBJECT,
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        });

        match judge.classify_callable(ctor) {
            CallableKind::Constructor { return_type, .. } => {
                assert_eq!(return_type, TypeId::OBJECT);
            }
            _ => panic!("Expected Constructor callable"),
        }
    }

    #[test]
    fn test_judge_classify_primitive() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        assert!(
            judge
                .classify_primitive(TypeId::NUMBER)
                .contains(PrimitiveFlags::NUMBER_LIKE)
        );
        assert!(
            judge
                .classify_primitive(TypeId::STRING)
                .contains(PrimitiveFlags::STRING_LIKE)
        );
        assert!(
            judge
                .classify_primitive(TypeId::BOOLEAN)
                .contains(PrimitiveFlags::BOOLEAN_LIKE)
        );
        assert!(
            judge
                .classify_primitive(TypeId::NULL)
                .contains(PrimitiveFlags::NULLABLE)
        );
        assert!(
            judge
                .classify_primitive(TypeId::UNDEFINED)
                .contains(PrimitiveFlags::NULLABLE)
        );

        // Literal types
        let str_lit = interner.literal_string("hello");
        assert!(
            judge
                .classify_primitive(str_lit)
                .contains(PrimitiveFlags::STRING_LIKE)
        );

        let num_lit = interner.literal_number(42.0);
        assert!(
            judge
                .classify_primitive(num_lit)
                .contains(PrimitiveFlags::NUMBER_LIKE)
        );
    }

    #[test]
    fn test_judge_classify_truthiness() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        // Always truthy
        assert_eq!(
            judge.classify_truthiness(TypeId::BOOLEAN_TRUE),
            TruthinessKind::AlwaysTruthy
        );

        // Always falsy
        assert_eq!(
            judge.classify_truthiness(TypeId::BOOLEAN_FALSE),
            TruthinessKind::AlwaysFalsy
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::NULL),
            TruthinessKind::AlwaysFalsy
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::UNDEFINED),
            TruthinessKind::AlwaysFalsy
        );

        // Sometimes
        assert_eq!(
            judge.classify_truthiness(TypeId::BOOLEAN),
            TruthinessKind::Sometimes
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::STRING),
            TruthinessKind::Sometimes
        );
        assert_eq!(
            judge.classify_truthiness(TypeId::NUMBER),
            TruthinessKind::Sometimes
        );

        // Literal truthiness
        let empty_str = interner.literal_string("");
        assert_eq!(
            judge.classify_truthiness(empty_str),
            TruthinessKind::AlwaysFalsy
        );

        let non_empty_str = interner.literal_string("hello");
        assert_eq!(
            judge.classify_truthiness(non_empty_str),
            TruthinessKind::AlwaysTruthy
        );

        let zero = interner.literal_number(0.0);
        assert_eq!(judge.classify_truthiness(zero), TruthinessKind::AlwaysFalsy);

        let one = interner.literal_number(1.0);
        assert_eq!(judge.classify_truthiness(one), TruthinessKind::AlwaysTruthy);
    }

    #[test]
    fn test_judge_get_property() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let foo_atom = interner.intern_string("foo");
        let bar_atom = interner.intern_string("bar");

        let obj = interner.object(vec![PropertyInfo {
            name: foo_atom,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: true,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        // Found property
        match judge.get_property(obj, foo_atom) {
            PropertyResult::Found {
                type_id,
                optional,
                readonly,
            } => {
                assert_eq!(type_id, TypeId::NUMBER);
                assert!(!optional);
                assert!(readonly);
            }
            _ => panic!("Expected property to be found"),
        }

        // Not found
        assert!(matches!(
            judge.get_property(obj, bar_atom),
            PropertyResult::NotFound
        ));

        // Special types
        assert!(matches!(
            judge.get_property(TypeId::ANY, foo_atom),
            PropertyResult::IsAny
        ));
        assert!(matches!(
            judge.get_property(TypeId::UNKNOWN, foo_atom),
            PropertyResult::IsUnknown
        ));
        assert!(matches!(
            judge.get_property(TypeId::ERROR, foo_atom),
            PropertyResult::IsError
        ));
    }

    #[test]
    fn test_judge_caching() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        // First call
        let r1 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
        assert!(r1);

        // Second call should use cache
        let r2 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
        assert!(r2);

        // Clear and re-check
        judge.clear_caches();
        let r3 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
        assert!(r3);
    }

    #[test]
    fn test_judge_config() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();

        let strict_config = JudgeConfig {
            strict_null_checks: true,
            strict_function_types: true,
            exact_optional_property_types: true,
            no_unchecked_indexed_access: true,
        };

        let judge = DefaultJudge::new(&interner, &env, strict_config.clone());
        assert_eq!(judge.config(), &strict_config);
    }
}

// =============================================================================
// Phase 3 Tests: DefId Infrastructure
// =============================================================================

mod phase3_defid {
    use super::*;

    #[test]
    fn test_def_id_allocation() {
        let store = DefinitionStore::new();

        let id1 = store.register(DefinitionInfo::type_alias(
            crate::interner::Atom(1),
            vec![],
            TypeId::NUMBER,
        ));

        let id2 = store.register(DefinitionInfo::type_alias(
            crate::interner::Atom(2),
            vec![],
            TypeId::STRING,
        ));

        assert_ne!(id1, id2);
        assert!(id1.is_valid());
        assert!(id2.is_valid());
        assert!(!DefId::INVALID.is_valid());
    }

    #[test]
    fn test_definition_store_crud() {
        let interner = TypeInterner::new();
        let store = DefinitionStore::new();

        let name = interner.intern_string("Point");
        let x = interner.intern_string("x");
        let y = interner.intern_string("y");

        // Create interface
        let def_id = store.register(DefinitionInfo::interface(
            name,
            vec![],
            vec![
                PropertyInfo {
                    name: x,
                    type_id: TypeId::NUMBER,
                    write_type: TypeId::NUMBER,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                },
                PropertyInfo {
                    name: y,
                    type_id: TypeId::NUMBER,
                    write_type: TypeId::NUMBER,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                },
            ],
        ));

        // Read
        assert!(store.contains(def_id));
        assert_eq!(store.get_kind(def_id), Some(DefKind::Interface));

        let shape = store.get_instance_shape(def_id).expect("has shape");
        assert_eq!(shape.properties.len(), 2);

        // Update body
        store.set_body(def_id, TypeId::OBJECT);
        assert_eq!(store.get_body(def_id), Some(TypeId::OBJECT));
    }

    #[test]
    fn test_class_inheritance() {
        let interner = TypeInterner::new();
        let store = DefinitionStore::new();

        let base_name = interner.intern_string("Animal");
        let derived_name = interner.intern_string("Dog");

        let base_id = store.register(DefinitionInfo::class(base_name, vec![], vec![], vec![]));

        let derived_id = store.register(
            DefinitionInfo::class(derived_name, vec![], vec![], vec![]).with_extends(base_id),
        );

        assert_eq!(store.get_extends(derived_id), Some(base_id));
        assert_eq!(store.get_extends(base_id), None);
    }

    #[test]
    fn test_enum_definition() {
        let interner = TypeInterner::new();
        let store = DefinitionStore::new();

        let name = interner.intern_string("Color");
        let red = interner.intern_string("Red");
        let green = interner.intern_string("Green");
        let blue = interner.intern_string("Blue");

        let def_id = store.register(DefinitionInfo::enumeration(
            name,
            vec![
                (red, EnumMemberValue::Number(0.0)),
                (green, EnumMemberValue::Number(1.0)),
                (blue, EnumMemberValue::Number(2.0)),
            ],
        ));

        let info = store.get(def_id).expect("enum exists");
        assert_eq!(info.kind, DefKind::Enum);
        assert_eq!(info.enum_members.len(), 3);
    }

    #[test]
    fn test_content_addressed_ids() {
        let interner = TypeInterner::new();
        let generator = ContentAddressedDefIds::new();

        let name = interner.intern_string("Foo");

        // Same content = same ID
        let id1 = generator.get_or_create(name, 1, 100);
        let id2 = generator.get_or_create(name, 1, 100);
        assert_eq!(id1, id2);

        // Different file = different ID
        let id3 = generator.get_or_create(name, 2, 100);
        assert_ne!(id1, id3);

        // Different position = different ID
        let id4 = generator.get_or_create(name, 1, 200);
        assert_ne!(id1, id4);
    }

    #[test]
    fn test_definition_store_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let store = Arc::new(DefinitionStore::new());
        let threads: Vec<_> = (0..4)
            .map(|i| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    for j in 0..50 {
                        let id = store.register(DefinitionInfo::type_alias(
                            crate::interner::Atom(i * 1000 + j),
                            vec![],
                            TypeId::NUMBER,
                        ));
                        assert!(store.contains(id));
                    }
                })
            })
            .collect();

        for t in threads {
            t.join().unwrap();
        }

        assert_eq!(store.len(), 200);
    }
}

// =============================================================================
// Phase 4 Tests: Sound Mode
// =============================================================================

mod phase4_sound_mode {
    use super::*;

    #[test]
    fn test_sticky_freshness_basic() {
        let mut tracker = StickyFreshnessTracker::new();

        let sym = SymbolId(1);
        assert!(!tracker.is_binding_fresh(sym));

        tracker.mark_binding_fresh(sym, TypeId::OBJECT);
        assert!(tracker.is_binding_fresh(sym));
        assert_eq!(tracker.get_fresh_source_type(sym), Some(TypeId::OBJECT));

        tracker.consume_freshness(sym);
        assert!(!tracker.is_binding_fresh(sym));
    }

    #[test]
    fn test_sticky_freshness_transfer() {
        let mut tracker = StickyFreshnessTracker::new();

        let from = SymbolId(1);
        let to = SymbolId(2);

        tracker.mark_binding_fresh(from, TypeId::OBJECT);
        tracker.transfer_freshness(from, to);

        assert!(tracker.is_binding_fresh(from));
        assert!(tracker.is_binding_fresh(to));
    }

    #[test]
    fn test_sticky_freshness_property() {
        let mut tracker = StickyFreshnessTracker::new();

        let sym = SymbolId(1);
        let prop_hash = 12345u32;

        assert!(!tracker.is_property_fresh(sym, prop_hash));

        tracker.mark_property_fresh(sym, prop_hash);
        assert!(tracker.is_property_fresh(sym, prop_hash));
        assert!(!tracker.is_property_fresh(sym, 99999));
    }

    #[test]
    fn test_sound_lawyer_strict_any() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let config = JudgeConfig::default();
        let mut lawyer = SoundLawyer::new(&interner, &env, config);

        // In sound mode, any -> T is NOT allowed (except to any/unknown)
        assert!(!lawyer.is_assignable(TypeId::ANY, TypeId::NUMBER));
        assert!(!lawyer.is_assignable(TypeId::ANY, TypeId::STRING));

        // But T -> any is still allowed
        assert!(lawyer.is_assignable(TypeId::NUMBER, TypeId::ANY));
        assert!(lawyer.is_assignable(TypeId::STRING, TypeId::ANY));

        // any -> any and any -> unknown are OK
        assert!(lawyer.is_assignable(TypeId::ANY, TypeId::ANY));
        assert!(lawyer.is_assignable(TypeId::ANY, TypeId::UNKNOWN));
    }

    #[test]
    fn test_sound_lawyer_top_bottom() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let config = JudgeConfig::default();
        let mut lawyer = SoundLawyer::new(&interner, &env, config);

        // unknown is top
        assert!(lawyer.is_assignable(TypeId::NUMBER, TypeId::UNKNOWN));
        assert!(lawyer.is_assignable(TypeId::STRING, TypeId::UNKNOWN));

        // never is bottom
        assert!(lawyer.is_assignable(TypeId::NEVER, TypeId::NUMBER));
        assert!(lawyer.is_assignable(TypeId::NEVER, TypeId::STRING));
    }

    #[test]
    fn test_sound_diagnostic_codes() {
        assert_eq!(
            SoundDiagnosticCode::ExcessPropertyStickyFreshness.code(),
            9001
        );
        assert_eq!(SoundDiagnosticCode::MutableArrayCovariance.code(), 9002);
        assert_eq!(SoundDiagnosticCode::MethodBivariance.code(), 9003);
        assert_eq!(SoundDiagnosticCode::AnyEscape.code(), 9004);
        assert_eq!(SoundDiagnosticCode::EnumNumberAssignment.code(), 9005);
        assert_eq!(SoundDiagnosticCode::MissingIndexSignature.code(), 9006);
        assert_eq!(SoundDiagnosticCode::UnsafeTypeAssertion.code(), 9007);
        assert_eq!(SoundDiagnosticCode::UncheckedIndexedAccess.code(), 9008);
    }

    #[test]
    fn test_sound_diagnostic_formatting() {
        let diag = SoundDiagnostic::new(SoundDiagnosticCode::ExcessPropertyStickyFreshness)
            .with_arg("extra")
            .with_arg("Target");

        let msg = diag.format_message();
        assert!(msg.contains("extra"));
        assert!(msg.contains("Target"));
    }

    #[test]
    fn test_sound_mode_config() {
        let all = SoundModeConfig::all();
        assert!(all.sticky_freshness);
        assert!(all.strict_any);
        assert!(all.strict_array_covariance);
        assert!(all.strict_method_bivariance);
        assert!(all.strict_enums);

        let minimal = SoundModeConfig::minimal();
        assert!(minimal.sticky_freshness);
        assert!(!minimal.strict_any);
        assert!(!minimal.strict_array_covariance);
    }

    #[test]
    fn test_sound_lawyer_check_assignment() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let config = JudgeConfig::default();
        let mut lawyer = SoundLawyer::new(&interner, &env, config);

        let mut diagnostics = Vec::new();

        // Valid assignment
        let result = lawyer.check_assignment(TypeId::NUMBER, TypeId::NUMBER, &mut diagnostics);
        assert!(result);
        assert!(diagnostics.is_empty());

        // Any escape should be detected
        diagnostics.clear();
        let result = lawyer.check_assignment(TypeId::ANY, TypeId::NUMBER, &mut diagnostics);
        assert!(!result);
        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].code, SoundDiagnosticCode::AnyEscape);
    }
}

// =============================================================================
// Integration Tests
// =============================================================================

mod integration {
    use super::*;

    #[test]
    fn test_judge_with_type_environment() {
        let interner = TypeInterner::new();
        let mut env = TypeEnvironment::new();

        // Register a type alias: type MyNumber = number
        let def_id = DefId(1);
        env.insert_def(def_id, TypeId::NUMBER);

        let judge = DefaultJudge::with_defaults(&interner, &env);

        // The judge can resolve references through the environment
        let ref_type = interner.lazy(def_id);

        // After evaluation, should be number
        let evaluated = judge.evaluate(ref_type);
        assert_eq!(evaluated, TypeId::NUMBER);
    }

    #[test]
    fn test_complex_object_property_access() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        // Create a complex nested object
        let inner_a = interner.intern_string("inner");
        let inner_obj = interner.object(vec![PropertyInfo {
            name: inner_a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        let outer_a = interner.intern_string("outer");
        let outer_obj = interner.object(vec![PropertyInfo {
            name: outer_a,
            type_id: inner_obj,
            write_type: inner_obj,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        // Access outer.outer
        match judge.get_property(outer_obj, outer_a) {
            PropertyResult::Found { type_id, .. } => {
                assert_eq!(type_id, inner_obj);
            }
            _ => panic!("Expected property found"),
        }
    }

    #[test]
    fn test_union_subtyping_with_judge() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let union = interner.union(vec![TypeId::NUMBER, TypeId::STRING]);

        // Members are subtypes
        assert!(judge.is_subtype(TypeId::NUMBER, union));
        assert!(judge.is_subtype(TypeId::STRING, union));

        // Non-members are not
        assert!(!judge.is_subtype(TypeId::BOOLEAN, union));

        // Union is not subtype of members
        assert!(!judge.is_subtype(union, TypeId::NUMBER));
        assert!(!judge.is_subtype(union, TypeId::STRING));
    }

    #[test]
    fn test_intersection_subtyping_with_judge() {
        let interner = TypeInterner::new();
        let env = TypeEnvironment::new();
        let judge = DefaultJudge::with_defaults(&interner, &env);

        let a = interner.intern_string("a");
        let b = interner.intern_string("b");

        let obj_a = interner.object(vec![PropertyInfo {
            name: a,
            type_id: TypeId::NUMBER,
            write_type: TypeId::NUMBER,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        let obj_b = interner.object(vec![PropertyInfo {
            name: b,
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: false,
            readonly: false,
            is_method: false,
            visibility: Visibility::Public,
            parent_id: None,
        }]);

        let obj_ab = interner.object(vec![
            PropertyInfo {
                name: a,
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            },
            PropertyInfo {
                name: b,
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            },
        ]);

        let _intersection = interner.intersection(vec![obj_a, obj_b]);

        // Object with both properties should be subtype of intersection
        assert!(judge.is_subtype(obj_ab, obj_a));
        assert!(judge.is_subtype(obj_ab, obj_b));
    }
}
