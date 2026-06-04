#[test]
fn test_get_members_non_object() {
    let setup = JudgeSetup::new();
    let judge = setup.judge();

    let members = judge.get_members(TypeId::NUMBER);
    assert!(members.is_empty());
}

#[test]
fn test_get_call_signatures_function() {
    let setup = JudgeSetup::new();
    let interner = &setup.interner;
    let judge = setup.judge();

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let sigs = judge.get_call_signatures(func);
    assert_eq!(sigs.len(), 1);
    assert_eq!(sigs[0].return_type, TypeId::STRING);
    assert_eq!(sigs[0].params.len(), 1);
}

#[test]
fn test_get_construct_signatures_constructor() {
    let setup = JudgeSetup::new();
    let interner = &setup.interner;
    let judge = setup.judge();

    let ctor = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::OBJECT,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: true,
        is_method: false,
    });

    let call_sigs = judge.get_call_signatures(ctor);
    assert!(
        call_sigs.is_empty(),
        "Constructor should have no call signatures"
    );

    let construct_sigs = judge.get_construct_signatures(ctor);
    assert_eq!(construct_sigs.len(), 1);
    assert_eq!(construct_sigs[0].return_type, TypeId::OBJECT);
}

#[test]
fn test_object_with_string_index_is_subtype() {
    let setup = JudgeSetup::new();
    let interner = &setup.interner;
    let judge = setup.judge();

    // { [key: string]: number } should accept any object with compatible properties
    let indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    // An object with specific number-typed properties should be a subtype
    let x_atom = interner.intern_string("x");
    let specific = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);

    assert!(
        judge.is_subtype(specific, indexed),
        "Object with number properties should be subtype of string-indexed number object"
    );
}

#[test]
fn test_function_is_not_subtype_of_number_index_target() {
    // Regression: parseTypes.ts conformance fingerprint —
    // `(s: string) => void` is NOT assignable to `{ [x: number]: number; }`
    // because a function value provides no number index signature.
    let setup = JudgeSetup::new();
    let interner = &setup.interner;
    let judge = setup.judge();

    let fn_type = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: None,
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let number_indexed = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: None,
        number_index: Some(IndexSignature {
            key_type: TypeId::NUMBER,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
    });

    assert!(
        !judge.is_subtype(fn_type, number_indexed),
        "function (s: string) => void must NOT be assignable to {{ [x: number]: number }}"
    );
}

#[test]
fn test_literal_union_subtype() {
    let setup = JudgeSetup::new();
    let interner = &setup.interner;
    let judge = setup.judge();

    let hello = interner.literal_string("hello");
    let world = interner.literal_string("world");
    let union = interner.union(vec![hello, world]);

    // "hello" <: "hello" | "world"
    assert!(judge.is_subtype(hello, union));

    // "world" <: "hello" | "world"
    assert!(judge.is_subtype(world, union));

    // "hello" | "world" <: string
    assert!(judge.is_subtype(union, TypeId::STRING));

    // string is NOT <: "hello" | "world"
    assert!(!judge.is_subtype(TypeId::STRING, union));
}
