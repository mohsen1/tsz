use super::*;

// =================================================================

#[test]
fn format_callable_single_call_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let callable = db.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let result = fmt.format(callable);
    // Single call sig with no props/index = arrow-style
    assert!(result.contains("x: number"));
    assert!(result.contains("=> string"));
}

/// Test for conformance issue: { <S>() : S[]; } should format as <S>() => S[]
#[test]
fn format_callable_generic_single_call_signature_arrow_syntax() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Create type param S
    let s_param = TypeParamInfo {
        name: db.intern_string("S"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let s_type = db.type_param(s_param);

    // Create S[] array type
    let s_array = db.array(s_type);

    // Create callable: { <S>() : S[]; } - single call signature, no params, returns S[]
    let callable = db.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: vec![s_param],
            params: vec![],
            this_type: None,
            return_type: s_array,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let result = fmt.format(callable);
    // Single call signature with no props/construct/index should use arrow syntax
    // Expected: <S>() => S[]
    // NOT: { <S>() : S[];; } (object syntax with double semicolons)
    assert!(
        result.contains("=>"),
        "Expected arrow syntax for single call signature callable, got: {result}"
    );
    assert!(
        !result.contains("{{"),
        "Should NOT use object braces for single call signature callable, got: {result}"
    );
    assert!(
        !result.contains(";;"),
        "Should NOT have double semicolons, got: {result}"
    );
    assert_eq!(
        result, "<S>() => S[]",
        "Exact match for single call signature callable"
    );
}

#[test]
fn format_callable_single_construct_signature() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    // Anonymous callable with single construct signature should use arrow notation:
    // `new <T>(x: T, y: T) => string` instead of `{ new <T>(x: T, y: T): string; }`
    let type_param = TypeParamInfo {
        name: db.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    };
    let t_type = db.type_param(type_param);

    let callable = db.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![type_param],
            params: vec![
                ParamInfo {
                    name: Some(db.intern_string("x")),
                    type_id: t_type,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(db.intern_string("y")),
                    type_id: t_type,
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let result = fmt.format(callable);
    // Single construct sig with no props/index = arrow-style with 'new' prefix
    assert!(
        result.contains("new") && result.contains("<T>") && result.contains("=> string"),
        "Single construct signature should use arrow notation like 'new <T>(x: T, y: T) => string', got: {result}"
    );
    // Should NOT have braces (object literal format)
    assert!(
        !result.starts_with('{'),
        "Single construct signature should NOT use object notation, got: {result}"
    );
}

#[test]
fn format_callable_multiple_call_signatures() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let callable = db.callable(CallableShape {
        call_signatures: vec![
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(db.intern_string("x")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::NUMBER,
                type_predicate: None,
                is_method: false,
            },
            CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: Some(db.intern_string("x")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type: TypeId::STRING,
                type_predicate: None,
                is_method: false,
            },
        ],
        construct_signatures: vec![],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });
    let result = fmt.format(callable);
    // Multiple signatures => object-like format with { sig1; sig2 }
    assert!(
        result.contains("{") && result.contains("}"),
        "Multiple sigs should use object format, got: {result}"
    );
}

#[test]
fn format_construct_only_interface_callable_uses_type_name() {
    let db = TypeInterner::new();
    let mut symbols = tsz_binder::SymbolArena::new();
    let sym_id = symbols.alloc(tsz_binder::symbol_flags::INTERFACE, "ConstructableA".into());

    let callable = db.callable(CallableShape {
        call_signatures: vec![],
        construct_signatures: vec![CallSignature {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::ANY,
            type_predicate: None,
            is_method: false,
        }],
        properties: vec![],
        string_index: None,
        number_index: None,
        symbol: Some(sym_id),
        is_abstract: false,
    });

    let mut fmt = TypeFormatter::with_symbols(&db, &symbols);
    assert_eq!(fmt.format(callable), "ConstructableA");
}

// =================================================================
// Recursive / BoundParameter formatting
// =================================================================

#[test]
fn format_recursive_index() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let rec = db.recursive(0);
    assert_eq!(fmt.format(rec), "Recursive(0)");

    let rec2 = db.recursive(3);
    assert_eq!(fmt.format(rec2), "Recursive(3)");
}

#[test]
fn format_bound_parameter() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let bp = db.bound_parameter(0);
    assert_eq!(fmt.format(bp), "BoundParameter(0)");

    let bp1 = db.bound_parameter(1);
    assert_eq!(fmt.format(bp1), "BoundParameter(1)");
}

// =================================================================
// Property name quoting edge cases
// =================================================================

#[test]
fn needs_property_name_quotes_edge_cases() {
    // Leading digit is not a valid identifier start
    assert!(super::needs_property_name_quotes("1abc"));
    // Underscore-only is valid
    assert!(!super::needs_property_name_quotes("_"));
    assert!(!super::needs_property_name_quotes("__proto__"));
    // Dollar-only
    assert!(!super::needs_property_name_quotes("$"));
    assert!(!super::needs_property_name_quotes("$0"));
    // Special characters
    assert!(super::needs_property_name_quotes("."));
    assert!(super::needs_property_name_quotes("@"));
    assert!(super::needs_property_name_quotes("#private"));
}

#[test]
fn needs_property_name_quotes_bracket_wrapped() {
    // Computed symbol property names wrapped in brackets should not be quoted
    assert!(!super::needs_property_name_quotes("[Symbol.asyncIterator]"));
    assert!(!super::needs_property_name_quotes("[Symbol.iterator]"));
    assert!(!super::needs_property_name_quotes("[Symbol.hasInstance]"));
    assert!(!super::needs_property_name_quotes("[Symbol.toPrimitive]"));
    // Single bracket only (not a computed property) should still need quotes
    assert!(super::needs_property_name_quotes("["));
    assert!(super::needs_property_name_quotes("]"));
    // Bracket at start but not end (not computed property syntax)
    assert!(super::needs_property_name_quotes("[foo"));
    assert!(super::needs_property_name_quotes("foo]"));
}

// =================================================================
// Method shorthand formatting
// =================================================================

#[test]
fn format_object_method_shorthand() {
    let db = TypeInterner::new();
    let mut fmt = TypeFormatter::new(&db);

    let method_type = db.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(db.intern_string("x")),
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
    let mut method_prop = PropertyInfo::new(db.intern_string("greet"), method_type);
    method_prop.is_method = true;

    let obj = db.object(vec![method_prop]);
    let result = fmt.format(obj);
    // Method shorthand: greet(x: number): string
    assert!(
        result.contains("greet(") && result.contains("): string"),
        "Expected method shorthand, got: {result}"
    );
    // Should NOT use arrow notation
    assert!(
        !result.contains("=>"),
        "Method shorthand should use ':' not '=>', got: {result}"
    );
}

// =================================================================
// Const type parameter
