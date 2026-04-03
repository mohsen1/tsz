use super::*;
use crate::TypeInterner;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, MappedModifier, MappedType, ObjectShape,
    ParamInfo, PropertyInfo, StringIntrinsicKind, TemplateSpan, TypeParamInfo,
};


    #[test]
    fn union_null_at_end() {
        let db = TypeInterner::new();
        // Create union: null | string  (null first in storage order)
        // union_preserve_members keeps the input order in storage
        let union_id = db.union_preserve_members(vec![TypeId::NULL, TypeId::STRING]);

        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(union_id);
        // null should appear at end, not beginning
        assert_eq!(result, "string | null");
    }

    #[test]
    fn union_undefined_at_end() {
        let db = TypeInterner::new();
        let union_id = db.union_preserve_members(vec![TypeId::UNDEFINED, TypeId::NUMBER]);

        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(union_id);
        assert_eq!(result, "number | undefined");
    }

    #[test]
    fn union_null_and_undefined_at_end() {
        let db = TypeInterner::new();
        let union_id =
            db.union_preserve_members(vec![TypeId::NULL, TypeId::UNDEFINED, TypeId::STRING]);

        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(union_id);
        // Non-nullish first, then null, then undefined
        assert_eq!(result, "string | null | undefined");
    }

    #[test]
    fn union_no_nullish_unchanged() {
        let db = TypeInterner::new();
        let union_id = db.union_preserve_members(vec![TypeId::NUMBER, TypeId::STRING]);

        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(union_id);
        // Union members are sorted by tsc's type creation order (string=8, number=9)
        assert_eq!(result, "string | number");
    }

    #[test]
    fn needs_property_name_quotes_basic() {
        // Valid identifiers: no quotes needed
        assert!(!super::needs_property_name_quotes("foo"));
        assert!(!super::needs_property_name_quotes("_private"));
        assert!(!super::needs_property_name_quotes("$jquery"));
        assert!(!super::needs_property_name_quotes("camelCase"));
        assert!(!super::needs_property_name_quotes("PascalCase"));
        assert!(!super::needs_property_name_quotes("x"));

        // Numeric: no quotes needed
        assert!(!super::needs_property_name_quotes("0"));
        assert!(!super::needs_property_name_quotes("42"));

        // Names with hyphens/spaces/etc: quotes needed
        assert!(super::needs_property_name_quotes("data-prop"));
        assert!(super::needs_property_name_quotes("aria-label"));
        assert!(super::needs_property_name_quotes("my name"));
        assert!(super::needs_property_name_quotes(""));
    }

    #[test]
    fn tuple_type_alias_preserved_in_format() {
        let db = TypeInterner::new();
        let def_store = crate::def::DefinitionStore::new();

        // Create a tuple type: [number, string, boolean]
        let tuple_id = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::BOOLEAN,
                name: None,
                optional: false,
                rest: false,
            },
        ]);

        // Register a type alias T1 = [number, string, boolean]
        let name = db.intern_string("T1");
        let info = crate::def::DefinitionInfo::type_alias(name, vec![], tuple_id);
        let _def_id = def_store.register(info);

        // Without def_store: should show structural form
        let mut fmt = TypeFormatter::new(&db);
        let without_alias = fmt.format(tuple_id);
        assert_eq!(without_alias, "[number, string, boolean]");

        // With def_store: should show alias name
        let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
        let with_alias = fmt.format(tuple_id);
        assert_eq!(with_alias, "T1");
    }

    #[test]
    fn object_type_with_hyphenated_property_quoted() {
        let db = TypeInterner::new();
        let name = db.intern_string("data-prop");
        let prop = PropertyInfo {
            name,
            type_id: TypeId::BOOLEAN,
            write_type: TypeId::BOOLEAN,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: crate::types::Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        };
        let obj = db.object(vec![prop]);
        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(obj);
        assert_eq!(result, "{ 'data-prop': boolean; }");
    }

    #[test]
    fn mapped_type_preserves_param_name() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("P"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: db.keyof(TypeId::STRING),
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        });
        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(mapped);
        assert!(
            result.contains("[P in "),
            "Expected [P in ...], got: {result}"
        );
    }

    #[test]
    fn mapped_type_shows_optional_modifier() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Add),
        });
        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(mapped);
        assert!(
            result.contains("]?:"),
            "Expected ]?: in mapped type, got: {result}"
        );
    }

    #[test]
    fn mapped_type_shows_readonly_modifier() {
        let db = TypeInterner::new();
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("P"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: Some(MappedModifier::Add),
            optional_modifier: None,
        });
        let mut fmt = TypeFormatter::new(&db);
        let result = fmt.format(mapped);
        assert!(
            result.contains("readonly [x: string]: number"),
            "Expected readonly index-signature display, got: {result}"
        );
    }

    // =================================================================
    // Primitive type formatting
    // =================================================================

    #[test]
    fn format_all_primitive_type_ids() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(TypeId::NEVER), "never");
        assert_eq!(fmt.format(TypeId::UNKNOWN), "unknown");
        assert_eq!(fmt.format(TypeId::ANY), "any");
        assert_eq!(fmt.format(TypeId::VOID), "void");
        assert_eq!(fmt.format(TypeId::UNDEFINED), "undefined");
        assert_eq!(fmt.format(TypeId::NULL), "null");
        assert_eq!(fmt.format(TypeId::BOOLEAN), "boolean");
        assert_eq!(fmt.format(TypeId::NUMBER), "number");
        assert_eq!(fmt.format(TypeId::STRING), "string");
        assert_eq!(fmt.format(TypeId::BIGINT), "bigint");
        assert_eq!(fmt.format(TypeId::SYMBOL), "symbol");
        assert_eq!(fmt.format(TypeId::OBJECT), "object");
        assert_eq!(fmt.format(TypeId::FUNCTION), "Function");
        assert_eq!(fmt.format(TypeId::ERROR), "error");
    }

    // =================================================================
    // Literal formatting
    // =================================================================

    #[test]
    fn format_string_literal_with_special_chars() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let empty = db.literal_string("");
        assert_eq!(fmt.format(empty), "\"\"");

        let spaces = db.literal_string("hello world");
        assert_eq!(fmt.format(spaces), "\"hello world\"");
    }

    #[test]
    fn format_number_literals() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(db.literal_number(0.0)), "0");
        assert_eq!(fmt.format(db.literal_number(-1.0)), "-1");
        assert_eq!(fmt.format(db.literal_number(3.15)), "3.15");
        assert_eq!(fmt.format(db.literal_number(1e10)), "10000000000");
        assert_eq!(fmt.format(db.literal_number(f64::INFINITY)), "Infinity");
        assert_eq!(
            fmt.format(db.literal_number(f64::NEG_INFINITY)),
            "-Infinity"
        );
        assert_eq!(fmt.format(db.literal_number(f64::NAN)), "NaN");
    }

    #[test]
    fn format_boolean_literals() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(TypeId::BOOLEAN_TRUE), "true");
        assert_eq!(fmt.format(TypeId::BOOLEAN_FALSE), "false");
    }

    #[test]
    fn format_bigint_literal() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let big = db.literal_bigint("123");
        assert_eq!(fmt.format(big), "123n");
    }

    // =================================================================
    // Union formatting
    // =================================================================

    #[test]
    fn format_union_two_members() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let union = db.union2(TypeId::STRING, TypeId::NUMBER);
        let result = fmt.format(union);
        assert!(result.contains("string"));
        assert!(result.contains("number"));
        assert!(result.contains(" | "));
    }

    #[test]
    fn format_union_three_members() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let union = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
        let result = fmt.format(union);
        assert!(result.contains("string"));
        assert!(result.contains("number"));
        assert!(result.contains("boolean"));
        // Should have exactly 2 "|" separators
        assert_eq!(result.matches(" | ").count(), 2);
    }

    #[test]
    fn format_union_with_literal_members() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let s1 = db.literal_string("a");
        let s2 = db.literal_string("b");
        let union = db.union2(s1, s2);
        let result = fmt.format(union);
        assert!(result.contains("\"a\""));
        assert!(result.contains("\"b\""));
        assert!(result.contains(" | "));
    }

    #[test]
    fn format_union_named_construct_callable_without_parentheses() {
        let db = TypeInterner::new();
        let mut symbols = tsz_binder::SymbolArena::new();
        let sym_id = symbols.alloc(tsz_binder::symbol_flags::INTERFACE, "ConstructableA".into());

        let constructable = db.callable(CallableShape {
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

        let union = db.union2(constructable, TypeId::STRING);
        let mut fmt = TypeFormatter::with_symbols(&db, &symbols);
        let rendered = fmt.format(union);
        assert!(rendered.contains("ConstructableA"));
        assert!(rendered.contains("string"));
        assert!(!rendered.contains("(ConstructableA)"));
    }

    #[test]
    fn format_large_union_truncation() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // Create a union with more members than max_union_members (default: 10)
        let members: Vec<TypeId> = (0..15).map(|i| db.literal_number(i as f64)).collect();
        let union = db.union_preserve_members(members);
        let result = fmt.format(union);
        // Should truncate with "..."
        assert!(
            result.contains("..."),
            "Large union should be truncated, got: {result}"
        );
    }

    // =================================================================
    // Intersection formatting
    // =================================================================

    #[test]
    fn format_intersection_two_type_params() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let u = db.type_param(TypeParamInfo {
            name: db.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let inter = db.intersection2(t, u);
        let result = fmt.format(inter);
        assert!(result.contains("T"));
        assert!(result.contains("U"));
        assert!(result.contains(" & "));
    }

    // =================================================================
    // Object type formatting
    // =================================================================

    #[test]
    fn format_empty_object() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![]);
        assert_eq!(fmt.format(obj), "{}");
    }

    #[test]
    fn format_object_single_property() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![PropertyInfo::new(
            db.intern_string("x"),
            TypeId::NUMBER,
        )]);
        assert_eq!(fmt.format(obj), "{ x: number; }");
    }

    #[test]
    fn format_object_multiple_properties() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![
            PropertyInfo::new(db.intern_string("x"), TypeId::NUMBER),
            PropertyInfo::new(db.intern_string("y"), TypeId::STRING),
        ]);
        let result = fmt.format(obj);
        assert!(result.contains("x: number"));
        assert!(result.contains("y: string"));
    }

    #[test]
    fn format_object_readonly_property() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mut prop = PropertyInfo::new(db.intern_string("x"), TypeId::NUMBER);
        prop.readonly = true;
        let obj = db.object(vec![prop]);
        let result = fmt.format(obj);
        assert!(
            result.contains("readonly x: number"),
            "Expected 'readonly x: number', got: {result}"
        );
    }

    #[test]
    fn format_object_many_properties_truncated() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // 10+ properties triggers truncation
        let props: Vec<PropertyInfo> = (0..12)
            .map(|i| PropertyInfo::new(db.intern_string(&format!("p{i}")), TypeId::NUMBER))
            .collect();
        let obj = db.object(props);
        let result = fmt.format(obj);
        assert!(
            result.contains("..."),
            "Object with >=10 properties should truncate, got: {result}"
        );
    }

    #[test]
    fn format_object_with_string_index_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: Some(crate::types::IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: false,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("[x: string]: number"),
            "Expected string index signature with default param name 'x', got: {result}"
        );
    }

    #[test]
    fn format_object_with_number_index_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: None,
            number_index: Some(crate::types::IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: false,
                param_name: None,
            }),
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("[x: number]: string"),
            "Expected number index signature with default param name 'x', got: {result}"
        );
    }

    #[test]
    fn format_object_with_readonly_number_index_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: None,
            number_index: Some(crate::types::IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                readonly: true,
                param_name: None,
            }),
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("readonly [x: number]: string"),
            "Expected readonly number index signature, got: {result}"
        );
    }

    #[test]
    fn format_object_with_readonly_string_index_signature() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let shape = crate::types::ObjectShape {
            properties: vec![],
            string_index: Some(crate::types::IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::NUMBER,
                readonly: true,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
            flags: Default::default(),
        };
        let obj = db.object_with_index(shape);
        let result = fmt.format(obj);
        assert!(
            result.contains("readonly [x: string]: number"),
            "Expected readonly string index signature, got: {result}"
        );
    }

    // =================================================================
    // Function type formatting
    // =================================================================

    #[test]
    fn format_function_no_params() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(result, "() => void");
    }

    #[test]
    fn format_function_two_params() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![
                ParamInfo {
                    name: Some(db.intern_string("a")),
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                },
                ParamInfo {
                    name: Some(db.intern_string("b")),
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                },
            ],
            this_type: None,
            return_type: TypeId::BOOLEAN,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(result, "(a: string, b: number) => boolean");
    }

    #[test]
    fn format_function_rest_param() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let arr = db.array(TypeId::STRING);
        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("args")),
                type_id: arr,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("...args"),
            "Expected rest param, got: {result}"
        );
    }

    #[test]
    fn format_function_with_type_params() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t_atom = db.intern_string("T");
        let t_param = db.type_param(TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
        });
        let func = db.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: t_atom,
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(result.contains("<T>"), "Expected type param, got: {result}");
        assert!(result.contains("x: T"));
        assert!(result.contains("=> T"));
    }

    #[test]
    fn format_function_type_param_with_constraint() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t_atom = db.intern_string("T");
        let t_param = db.type_param(TypeParamInfo {
            name: t_atom,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
        });
        let func = db.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: t_atom,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("T extends string"),
            "Expected 'T extends string', got: {result}"
        );
    }

    #[test]
    fn format_function_type_param_with_default() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t_atom = db.intern_string("T");
        let t_param = db.type_param(TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: Some(TypeId::STRING),
            is_const: false,
        });
        let func = db.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: t_atom,
                constraint: None,
                default: Some(TypeId::STRING),
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("T = string"),
            "Expected 'T = string', got: {result}"
        );
    }

    #[test]
    fn format_constructor_function() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("new "),
            "Constructor should start with 'new', got: {result}"
        );
    }

    // =================================================================
    // Array/tuple formatting
    // =================================================================

    #[test]
    fn format_array_primitive() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(db.array(TypeId::STRING)), "string[]");
        assert_eq!(fmt.format(db.array(TypeId::NUMBER)), "number[]");
        assert_eq!(fmt.format(db.array(TypeId::BOOLEAN)), "boolean[]");
    }

    #[test]
    fn format_array_of_function_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let arr = db.array(func);
        let result = fmt.format(arr);
        assert!(
            result.starts_with('(') && result.ends_with(")[]"),
            "Array of function should be parenthesized, got: {result}"
        );
    }

    #[test]
    fn format_tuple_empty() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![]);
        assert_eq!(fmt.format(tuple), "[]");
    }

    #[test]
    fn format_tuple_single_element() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }]);
        assert_eq!(fmt.format(tuple), "[string]");
    }

    #[test]
    fn format_tuple_two_elements() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        assert_eq!(fmt.format(tuple), "[string, number]");
    }

    #[test]
    fn format_tuple_named_elements() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: Some(db.intern_string("name")),
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: Some(db.intern_string("age")),
                optional: false,
                rest: false,
            },
        ]);
        assert_eq!(fmt.format(tuple), "[name: string, age: number]");
    }

    #[test]
    fn format_tuple_optional_element() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tuple = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: true,
                rest: false,
            },
        ]);
        let result = fmt.format(tuple);
        assert_eq!(result, "[string, number?]");
    }

    #[test]
    fn format_tuple_rest_element() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let str_arr = db.array(TypeId::STRING);
        let tuple = db.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: str_arr,
                name: None,
                optional: false,
                rest: true,
            },
        ]);
        let result = fmt.format(tuple);
        assert_eq!(result, "[number, ...string[]]");
    }

    // =================================================================
    // Conditional type formatting
    // =================================================================

    #[test]
    fn format_conditional_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let cond = db.conditional(crate::types::ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::NUMBER,
            true_type: TypeId::BOOLEAN,
            false_type: TypeId::NEVER,
            is_distributive: false,
        });
        let result = fmt.format(cond);
        assert_eq!(result, "string extends number ? boolean : never");
    }

    #[test]
    fn format_conditional_type_nested() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // T extends string ? (T extends "a" ? 1 : 2) : 3
        let inner = db.conditional(crate::types::ConditionalType {
            check_type: TypeId::STRING,
            extends_type: db.literal_string("a"),
            true_type: db.literal_number(1.0),
            false_type: db.literal_number(2.0),
            is_distributive: false,
        });
        let outer = db.conditional(crate::types::ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: inner,
            false_type: db.literal_number(3.0),
            is_distributive: false,
        });
        let result = fmt.format(outer);
        assert!(result.contains("extends"));
        assert!(result.contains("?"));
        assert!(result.contains(":"));
    }

    // =================================================================
    // Mapped type formatting
    // =================================================================

    #[test]
    fn format_mapped_type_basic() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        });
        let result = fmt.format(mapped);
        assert_eq!(result, "{ [x: string]: number; }");
    }

    #[test]
    fn format_mapped_type_with_remove_optional() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: Some(MappedModifier::Remove),
        });
        let result = fmt.format(mapped);
        assert!(
            result.contains("]-?:"),
            "Expected remove optional modifier '-?', got: {result}"
        );
    }

    #[test]
    fn format_mapped_type_with_remove_readonly() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("K"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: Some(MappedModifier::Remove),
            optional_modifier: None,
        });
        let result = fmt.format(mapped);
        assert!(
            result.contains("-readonly"),
            "Expected remove readonly modifier, got: {result}"
        );
    }

    #[test]
    fn format_mapped_string_index_signature_like() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: db.intern_string("P"),
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: TypeId::NUMBER,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        });

        assert_eq!(fmt.format(mapped), "{ [x: string]: number; }");
    }

    #[test]
    fn format_mapped_preserves_key_dependent_template() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);
        let key_name = db.intern_string("P");
        let key_param = db.type_param(TypeParamInfo {
            name: key_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let mapped = db.mapped(MappedType {
            type_param: TypeParamInfo {
                name: key_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: TypeId::STRING,
            template: key_param,
            name_type: None,
            readonly_modifier: None,
            optional_modifier: None,
        });

        assert_eq!(fmt.format(mapped), "{ [P in string]: P; }");
    }

    // =================================================================
    // Template literal formatting
    // =================================================================

    #[test]
    fn format_template_literal_text_only() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tl = db.template_literal(vec![TemplateSpan::Text(db.intern_string("hello"))]);
        // Text-only template literals may be simplified by the interner
        // but if they survive, they should format with backticks
        let result = fmt.format(tl);
        assert!(
            result.contains("hello"),
            "Expected 'hello' in template literal, got: {result}"
        );
    }

    #[test]
    fn format_template_literal_with_type_interpolation() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tl = db.template_literal(vec![
            TemplateSpan::Text(db.intern_string("hello ")),
            TemplateSpan::Type(TypeId::STRING),
        ]);
        let result = fmt.format(tl);
        assert!(
            result.contains("hello "),
            "Expected 'hello ' prefix, got: {result}"
        );
        assert!(
            result.contains("${string}"),
            "Expected '${{string}}' interpolation, got: {result}"
        );
    }

    #[test]
    fn format_template_literal_complex() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tl = db.template_literal(vec![
            TemplateSpan::Text(db.intern_string("key_")),
            TemplateSpan::Type(TypeId::NUMBER),
            TemplateSpan::Text(db.intern_string("_suffix")),
        ]);
        let result = fmt.format(tl);
        assert!(result.contains("key_"), "Expected 'key_', got: {result}");
        assert!(
            result.contains("${number}"),
            "Expected '${{number}}', got: {result}"
        );
        assert!(
            result.contains("_suffix"),
            "Expected '_suffix', got: {result}"
        );
    }

    // =================================================================
    // String intrinsic formatting
    // =================================================================

    #[test]
    fn format_string_intrinsic_uppercase() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let upper = db.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);
        assert_eq!(fmt.format(upper), "Uppercase<string>");
    }

    #[test]
    fn format_string_intrinsic_lowercase() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let lower = db.string_intrinsic(StringIntrinsicKind::Lowercase, TypeId::STRING);
        assert_eq!(fmt.format(lower), "Lowercase<string>");
    }

    #[test]
    fn format_string_intrinsic_capitalize() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let cap = db.string_intrinsic(StringIntrinsicKind::Capitalize, TypeId::STRING);
        assert_eq!(fmt.format(cap), "Capitalize<string>");
    }

    #[test]
    fn format_string_intrinsic_uncapitalize() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let uncap = db.string_intrinsic(StringIntrinsicKind::Uncapitalize, TypeId::STRING);
        assert_eq!(fmt.format(uncap), "Uncapitalize<string>");
    }

    // =================================================================
    // Error type formatting
    // =================================================================

    #[test]
    fn format_error_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);
        assert_eq!(fmt.format(TypeId::ERROR), "error");
    }

    // =================================================================
    // Depth limiting (deeply nested types)
    // =================================================================

    #[test]
    fn format_deeply_nested_array_truncated() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // Create deeply nested arrays: string[][][][][][]...
        let mut current = TypeId::STRING;
        for _ in 0..10 {
            current = db.array(current);
        }
        let result = fmt.format(current);
        // At some depth, the formatter should produce "..." due to max_depth
        assert!(
            result.contains("..."),
            "Deeply nested type should hit depth limit and show '...', got: {result}"
        );
    }

    #[test]
    fn format_deeply_nested_union_truncated() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        // Create nested unions: wrap in array at each level to increase depth
        let mut current = TypeId::STRING;
        for _ in 0..10 {
            let inner_union = db.union2(current, TypeId::NUMBER);
            current = db.array(inner_union);
        }
        let result = fmt.format(current);
        // Should hit depth limit
        assert!(
            result.contains("..."),
            "Deeply nested type should truncate, got: {result}"
        );
    }

    // =================================================================
    // Special types
    // =================================================================

    #[test]
    fn format_type_parameter() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let tp = db.type_param(TypeParamInfo {
            name: db.intern_string("MyType"),
            constraint: None,
            default: None,
            is_const: false,
        });
        assert_eq!(fmt.format(tp), "MyType");
    }

    #[test]
    fn format_keyof_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let keyof = db.keyof(TypeId::STRING);
        assert_eq!(fmt.format(keyof), "keyof string");
    }

    #[test]
    fn format_keyof_intersection_operand_parenthesized() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t = db.type_param(TypeParamInfo {
            name: db.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let u = db.type_param(TypeParamInfo {
            name: db.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
        });
        let intersection = db.intersection2(t, u);
        let keyof = db.keyof(intersection);

        assert_eq!(fmt.format(keyof), "keyof (T & U)");
    }

    #[test]
    fn format_readonly_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let ro = db.readonly_type(TypeId::NUMBER);
        assert_eq!(fmt.format(ro), "readonly number");
    }

    #[test]
    fn format_index_access_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let idx = db.index_access(TypeId::STRING, TypeId::NUMBER);
        assert_eq!(fmt.format(idx), "string[number]");
    }

    #[test]
    fn format_this_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let this = db.this_type();
        assert_eq!(fmt.format(this), "this");
    }

    #[test]
    fn format_infer_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let infer = db.infer(TypeParamInfo {
            name: db.intern_string("R"),
            constraint: None,
            default: None,
            is_const: false,
        });
        assert_eq!(fmt.format(infer), "infer R");
    }

    #[test]
    fn format_unique_symbol() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let sym = db.unique_symbol(crate::types::SymbolRef(999));
        assert_eq!(fmt.format(sym), "unique symbol");
    }

    #[test]
    fn format_no_infer_type() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let no_infer = db.no_infer(TypeId::STRING);
        assert_eq!(fmt.format(no_infer), "NoInfer<string>");
    }

    // =================================================================
    // Generic application formatting
    // =================================================================

    #[test]
    fn format_application_single_arg() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let base = db.lazy(crate::def::DefId(100));
        let app = db.application(base, vec![TypeId::NUMBER]);
        let result = fmt.format(app);
        // Without def store, base resolves to "Lazy(100)"
        assert!(
            result.contains("Lazy(100)"),
            "Expected 'Lazy(100)', got: {result}"
        );
        assert!(
            result.contains("<number>"),
            "Expected '<number>', got: {result}"
        );
    }

    #[test]
    fn format_application_two_args() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let base = db.lazy(crate::def::DefId(200));
        let app = db.application(base, vec![TypeId::STRING, TypeId::NUMBER]);
        let result = fmt.format(app);
        assert!(
            result.contains("<string, number>"),
            "Expected '<string, number>', got: {result}"
        );
    }

    // =================================================================
    // Callable type formatting
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
    // =================================================================

    #[test]
    fn format_const_type_param() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let t_atom = db.intern_string("T");
        let t_param = db.type_param(TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: true,
        });
        let func = db.function(FunctionShape {
            type_params: vec![TypeParamInfo {
                name: t_atom,
                constraint: None,
                default: None,
                is_const: true,
            }],
            params: vec![ParamInfo {
                name: Some(db.intern_string("x")),
                type_id: t_param,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert!(
            result.contains("const T"),
            "Expected 'const T' in type params, got: {result}"
        );
    }

    #[test]
    fn generic_class_type_shows_type_params() {
        // When a generic class (e.g., `class B<T>`) has its instance type formatted,
        // the formatter should show `B<T>` not just `B`.
        let db = TypeInterner::new();
        let def_store = crate::def::DefinitionStore::new();

        // Create an empty object type as the class instance body
        let instance_type = db.object(vec![]);

        // Register a class definition with one type parameter T
        let name = db.intern_string("B");
        let t_name = db.intern_string("T");
        let info = crate::def::DefinitionInfo {
            kind: crate::def::DefKind::Class,
            name,
            type_params: vec![TypeParamInfo {
                name: t_name,
                constraint: None,
                default: None,
                is_const: false,
            }],
            body: Some(instance_type),
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            span: None,
            file_id: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        };
        let def_id = def_store.register(info);

        // Register the instance type -> def mapping
        def_store.register_type_to_def(instance_type, def_id);

        // Without def_store: should show structural form
        let mut fmt = TypeFormatter::new(&db);
        let without = fmt.format(instance_type);
        assert_eq!(without, "{}");

        // With def_store: should show `B<T>` with type parameter name
        let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
        let with = fmt.format(instance_type);
        assert_eq!(with, "B<T>", "Generic class should show type params");
    }

    #[test]
    fn application_lazy_shows_type_args() {
        // Application(Lazy(def_id), [string, number]) should format as `Name<string, number>`
        use crate::caches::db::QueryDatabase;
        let db = TypeInterner::new();
        let def_store = crate::def::DefinitionStore::new();

        // Register a definition
        let name = db.intern_string("MyClass");
        let info = crate::def::DefinitionInfo {
            kind: crate::def::DefKind::Class,
            name,
            type_params: vec![
                TypeParamInfo {
                    name: db.intern_string("T"),
                    constraint: None,
                    default: None,
                    is_const: false,
                },
                TypeParamInfo {
                    name: db.intern_string("U"),
                    constraint: None,
                    default: None,
                    is_const: false,
                },
            ],
            body: None,
            instance_shape: None,
            static_shape: None,
            extends: None,
            implements: Vec::new(),
            enum_members: Vec::new(),
            exports: Vec::new(),
            span: None,
            file_id: None,
            symbol_id: None,
            heritage_names: Vec::new(),
            is_abstract: false,
            is_const: false,
            is_exported: false,
            is_global_augmentation: false,
            is_declare: false,
        };
        let def_id = def_store.register(info);

        // Create Application(Lazy(def_id), [string, number])
        let factory = db.factory();
        let lazy = factory.lazy(def_id);
        let app = factory.application(lazy, vec![TypeId::STRING, TypeId::NUMBER]);

        // With def_store: should show `MyClass<string, number>`
        let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
        let result = fmt.format(app);
        assert_eq!(
            result, "MyClass<string, number>",
            "Application should show formatted type args"
        );
    }

    #[test]
    fn lazy_raw_def_id_falls_back_to_symbol_name() {
        let db = TypeInterner::new();
        let mut symbols = tsz_binder::SymbolArena::new();
        let sym_id = symbols.alloc(tsz_binder::symbol_flags::INTERFACE, "Num".to_string());
        let lazy = db.lazy(crate::def::DefId(sym_id.0));

        let mut fmt = TypeFormatter::with_symbols(&db, &symbols);
        assert_eq!(fmt.format(lazy), "Num");
    }

    // =================================================================
    // Optional parameter/property display (no redundant `| undefined`)
    // =================================================================

    #[test]
    fn optional_param_shows_undefined() {
        // The formatter displays whatever type is stored in ParamInfo.type_id.
        // The checker is responsible for adding `| undefined` to `?`-optional
        // params before storing them.  When the stored type is plain `string`,
        // the formatter shows `(a?: string)`.
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("a")),
                type_id: TypeId::STRING,
                optional: true,
                rest: false,
            }],
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(
            result, "(a?: string) => any",
            "Formatter displays stored type as-is; checker adds | undefined for ?-optional"
        );
    }

    #[test]
    fn optional_param_with_union_undefined_keeps_it() {
        // When the type is internally `string | undefined`, the formatter keeps
        // `undefined` for optional params — matches tsc behavior.
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("a")),
                type_id: str_or_undef,
                optional: true,
                rest: false,
            }],
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(
            result, "(a?: string | undefined) => any",
            "Optional param preserves '| undefined' — matches tsc display"
        );
    }

    #[test]
    fn optional_property_shows_undefined() {
        // tsc: `{ x?: string | undefined; }` — object properties show | undefined
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::STRING,
            write_type: TypeId::STRING,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: crate::types::Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }]);
        let result = fmt.format(obj);
        assert_eq!(
            result, "{ x?: string | undefined; }",
            "tsc shows '| undefined' for optional object properties"
        );
    }

    #[test]
    fn optional_property_never_shows_as_undefined() {
        // When the property type is `never` and it's optional, tsc displays just `undefined`
        // since `never | undefined = undefined`.
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let obj = db.object(vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: TypeId::NEVER,
            write_type: TypeId::NEVER,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: crate::types::Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }]);
        let result = fmt.format(obj);
        assert_eq!(
            result, "{ x?: undefined; }",
            "Optional never property displays as undefined, not 'never | undefined'"
        );
    }

    #[test]
    fn optional_property_with_union_undefined_keeps_it() {
        // When the type already has `string | undefined`, display as-is (no duplicate)
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let obj = db.object(vec![PropertyInfo {
            name: db.intern_string("x"),
            type_id: str_or_undef,
            write_type: str_or_undef,
            optional: true,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: crate::types::Visibility::Public,
            parent_id: None,
            declaration_order: 0,
            is_string_named: false,
        }]);
        let result = fmt.format(obj);
        assert_eq!(
            result, "{ x?: string | undefined; }",
            "Optional property with string | undefined should keep it as-is"
        );
    }

    #[test]
    fn empty_object_shape_formats_without_spurious_separator() {
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        assert_eq!(fmt.format(db.object(Vec::new())), "{}");
    }

    #[test]
    fn non_optional_param_keeps_undefined_in_union() {
        // Non-optional params should still show `| undefined` if it's in the type
        let db = TypeInterner::new();
        let mut fmt = TypeFormatter::new(&db);

        let str_or_undef = db.union_preserve_members(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let func = db.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(db.intern_string("a")),
                type_id: str_or_undef,
                optional: false,
                rest: false,
            }],
            return_type: TypeId::ANY,
            this_type: None,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = fmt.format(func);
        assert_eq!(
            result, "(a: string | undefined) => any",
            "Non-optional param should keep '| undefined' in union"
        );
    }
