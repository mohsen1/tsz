use super::*;
use crate::TypeInterner;

fn create_test_interner() -> TypeInterner {
    TypeInterner::new()
}

#[test]
fn test_is_subtype_identity() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::STRING));
    assert!(judge.is_subtype(TypeId::BOOLEAN, TypeId::BOOLEAN));
}

#[test]
fn test_is_subtype_any_unknown() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // Everything is subtype of any
    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::ANY));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::ANY));

    // Everything is subtype of unknown
    assert!(judge.is_subtype(TypeId::NUMBER, TypeId::UNKNOWN));
    assert!(judge.is_subtype(TypeId::STRING, TypeId::UNKNOWN));

    // any is subtype of everything
    assert!(judge.is_subtype(TypeId::ANY, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::ANY, TypeId::STRING));

    // never is subtype of everything
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::NUMBER));
    assert!(judge.is_subtype(TypeId::NEVER, TypeId::STRING));
}

#[test]
fn test_classify_primitive() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let flags = judge.classify_primitive(TypeId::NUMBER);
    assert!(flags.contains(PrimitiveFlags::NUMBER_LIKE));
    assert!(!flags.contains(PrimitiveFlags::STRING_LIKE));

    let flags = judge.classify_primitive(TypeId::STRING);
    assert!(flags.contains(PrimitiveFlags::STRING_LIKE));
    assert!(!flags.contains(PrimitiveFlags::NUMBER_LIKE));

    let flags = judge.classify_primitive(TypeId::NULL);
    assert!(flags.contains(PrimitiveFlags::NULLABLE));
    assert!(flags.contains(PrimitiveFlags::NULL));

    let flags = judge.classify_primitive(TypeId::UNDEFINED);
    assert!(flags.contains(PrimitiveFlags::NULLABLE));
    assert!(flags.contains(PrimitiveFlags::UNDEFINED));
}

#[test]
fn test_classify_truthiness() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert_eq!(
        judge.classify_truthiness(TypeId::BOOLEAN_TRUE),
        TruthinessKind::AlwaysTruthy
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::BOOLEAN_FALSE),
        TruthinessKind::AlwaysFalsy
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::BOOLEAN),
        TruthinessKind::Sometimes
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::NULL),
        TruthinessKind::AlwaysFalsy
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::UNDEFINED),
        TruthinessKind::AlwaysFalsy
    );
    assert_eq!(
        judge.classify_truthiness(TypeId::ANY),
        TruthinessKind::Unknown
    );
}

#[test]
fn test_classify_iterable_array() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let array_number = interner.array(TypeId::NUMBER);
    match judge.classify_iterable(array_number) {
        IterableKind::Array(elem) => assert_eq!(elem, TypeId::NUMBER),
        _ => panic!("Expected Array iterable kind"),
    }
}

#[test]
fn test_classify_iterable_string() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    assert_eq!(
        judge.classify_iterable(TypeId::STRING),
        IterableKind::String
    );
}

#[test]
fn test_classify_callable_function() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: None,
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

    match judge.classify_callable(fn_type) {
        CallableKind::Function {
            params,
            return_type,
            ..
        } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].type_id, TypeId::NUMBER);
            assert_eq!(return_type, TypeId::STRING);
        }
        _ => panic!("Expected Function callable kind"),
    }
}

#[test]
fn test_get_property_object() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let foo_atom = interner.intern_string("foo");
    let obj = interner.object(vec![PropertyInfo {
        name: foo_atom,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
        visibility: Visibility::Public,
        parent_id: None,
    }]);

    match judge.get_property(obj, foo_atom) {
        PropertyResult::Found {
            type_id,
            optional,
            readonly,
        } => {
            assert_eq!(type_id, TypeId::NUMBER);
            assert!(!optional);
            assert!(!readonly);
        }
        _ => panic!("Expected property to be found"),
    }

    let bar_atom = interner.intern_string("bar");
    match judge.get_property(obj, bar_atom) {
        PropertyResult::NotFound => {}
        _ => panic!("Expected property not found"),
    }
}

#[test]
fn test_get_property_special_types() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    let foo_atom = interner.intern_string("foo");

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
fn test_caching() {
    let interner = create_test_interner();
    let env = TypeEnvironment::new();
    let judge = DefaultJudge::with_defaults(&interner, &env);

    // First call - not cached
    let result1 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
    assert!(result1);

    // Second call - should be cached
    let result2 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
    assert!(result2);

    // Clear caches
    judge.clear_caches();

    // Third call - cache was cleared, should still work
    let result3 = judge.is_subtype(TypeId::NUMBER, TypeId::ANY);
    assert!(result3);
}
