#[test]
fn test_interface_extends_multiple_with_overlap() {
    // interface A { shared: string; a: number }
    // interface B { shared: string; b: boolean }
    // interface C extends A, B {} // shared property from both
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let shared = interner.intern_string("shared");
    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_a = interner.object(vec![
        PropertyInfo::new(shared, TypeId::STRING),
        PropertyInfo::new(a_prop, TypeId::NUMBER),
    ]);

    let interface_b = interner.object(vec![
        PropertyInfo::new(shared, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::BOOLEAN),
    ]);

    let interface_c = interner.object(vec![
        PropertyInfo::new(shared, TypeId::STRING),
        PropertyInfo::new(a_prop, TypeId::NUMBER),
        PropertyInfo::new(b_prop, TypeId::BOOLEAN),
    ]);

    // C extends both
    assert!(checker.is_subtype_of(interface_c, interface_a));
    assert!(checker.is_subtype_of(interface_c, interface_b));
}

#[test]
fn test_interface_extends_multiple_methods() {
    // interface Readable { read(): string }
    // interface Writable { write(s: string): void }
    // interface ReadWritable extends Readable, Writable {}
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let read = interner.intern_string("read");
    let write = interner.intern_string("write");

    let read_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let write_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("s")),
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

    let readable = interner.object(vec![PropertyInfo::method(read, read_method)]);

    let writable = interner.object(vec![PropertyInfo::method(write, write_method)]);

    let read_writable = interner.object(vec![
        PropertyInfo::method(read, read_method),
        PropertyInfo::method(write, write_method),
    ]);

    assert!(checker.is_subtype_of(read_writable, readable));
    assert!(checker.is_subtype_of(read_writable, writable));
}

#[test]
fn test_interface_diamond_extends() {
    // interface A { a: string }
    // interface B extends A { b: number }
    // interface C extends A { c: boolean }
    // interface D extends B, C {} // diamond
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");
    let c_prop = interner.intern_string("c");

    let interface_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let interface_b = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let interface_c = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(c_prop, TypeId::BOOLEAN),
    ]);

    let interface_d = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
        PropertyInfo::new(c_prop, TypeId::BOOLEAN),
    ]);

    // D extends all in diamond
    assert!(checker.is_subtype_of(interface_d, interface_a));
    assert!(checker.is_subtype_of(interface_d, interface_b));
    assert!(checker.is_subtype_of(interface_d, interface_c));
}

#[test]
fn test_interface_implements_partial() {
    // Object missing some properties from interface
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    let interface_ab = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    let partial = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Partial does not implement full interface
    assert!(!checker.is_subtype_of(partial, interface_ab));
}

#[test]
fn test_interface_implements_extra_properties() {
    // Object with extra properties still implements interface
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let extra_prop = interner.intern_string("extra");

    let interface_a = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    let with_extra = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(extra_prop, TypeId::NUMBER),
    ]);

    // Object with extra properties implements interface
    assert!(checker.is_subtype_of(with_extra, interface_a));
}

#[test]
fn test_interface_implements_wrong_type() {
    // Object with wrong property type doesn't implement interface
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");

    let interface_string = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    let has_number = interner.object(vec![PropertyInfo::new(value, TypeId::NUMBER)]);

    // Wrong property type
    assert!(!checker.is_subtype_of(has_number, interface_string));
}

#[test]
fn test_interface_merge_same_properties() {
    // interface A { a: string }
    // interface A { b: number } // declaration merging
    // Merged: { a: string; b: number }
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let a_prop = interner.intern_string("a");
    let b_prop = interner.intern_string("b");

    // First declaration
    let interface_a1 = interner.object(vec![PropertyInfo::new(a_prop, TypeId::STRING)]);

    // Merged interface (both declarations)
    let interface_merged = interner.object(vec![
        PropertyInfo::new(a_prop, TypeId::STRING),
        PropertyInfo::new(b_prop, TypeId::NUMBER),
    ]);

    // Merged is subtype of first declaration
    assert!(checker.is_subtype_of(interface_merged, interface_a1));
    // But not the reverse
    assert!(!checker.is_subtype_of(interface_a1, interface_merged));
}

#[test]
fn test_interface_merge_method_overloads() {
    // interface A { method(x: string): void }
    // interface A { method(x: number): void }
    // Merged should have both overloads
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let method_name = interner.intern_string("method");

    let string_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
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

    let number_method = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let interface_string = interner.object(vec![PropertyInfo::method(method_name, string_method)]);

    let interface_number = interner.object(vec![PropertyInfo::method(method_name, number_method)]);

    // Different signatures - not subtypes of each other
    assert!(!checker.is_subtype_of(interface_string, interface_number));
    assert!(!checker.is_subtype_of(interface_number, interface_string));
}

#[test]
fn test_interface_merge_compatible_properties() {
    // interface A { value: string | number }
    // interface A { value: string } // narrower - compatible in merge context
    let interner = TypeInterner::new();
    let mut checker = SubtypeChecker::new(&interner);

    let value = interner.intern_string("value");
    let string_or_number = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let interface_wide = interner.object(vec![PropertyInfo::new(value, string_or_number)]);

    let interface_narrow = interner.object(vec![PropertyInfo::new(value, TypeId::STRING)]);

    // Narrow is subtype of wide
    assert!(checker.is_subtype_of(interface_narrow, interface_wide));
}
