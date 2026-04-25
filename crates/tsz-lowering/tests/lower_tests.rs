use crate::TypeLowering;
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::*;

#[test]
fn test_intrinsic_type_ids() {
    assert_eq!(TypeId::ANY.0, 4);
    assert_eq!(TypeId::STRING.0, 10);
    assert_eq!(TypeId::NUMBER.0, 9);
}

#[test]
fn test_lowering_new() {
    let arena = NodeArena::new();
    let interner = TypeInterner::new();
    let _lowering = TypeLowering::new(&arena, &interner);
}

#[test]
fn test_lower_intrinsic_type_annotation() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = string;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    assert_eq!(type_id, TypeId::STRING);
}

#[test]
fn test_lower_literal_string_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = \"hello\";");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "hello");
        }
        _ => panic!("Expected string literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_number_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 42;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, 42.0);
        }
        _ => panic!("Expected number literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_hex_number_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0xFF;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, 255.0);
        }
        _ => panic!("Expected hex literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_binary_number_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0b1010;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, 10.0);
        }
        _ => panic!("Expected binary literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_octal_number_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0o77;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, 63.0);
        }
        _ => panic!("Expected octal literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_number_with_separators() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 1_234_567;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, 1_234_567.0);
        }
        _ => panic!("Expected number literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_hex_number_with_separators() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0xFF_FF;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, 65_535.0);
        }
        _ => panic!("Expected hex literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_binary_number_with_separators() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0b1010_0101;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, 165.0);
        }
        _ => panic!("Expected binary literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_octal_number_with_separators() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0o12_34;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, 668.0);
        }
        _ => panic!("Expected octal literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_bigint_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 123n;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "123");
        }
        _ => panic!("Expected bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_hex_bigint_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0xFFn;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "255");
        }
        _ => panic!("Expected hex bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_binary_bigint_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0b1010n;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "10");
        }
        _ => panic!("Expected binary bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_octal_bigint_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0o77n;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "63");
        }
        _ => panic!("Expected octal bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_bigint_with_separators() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 1_000n;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "1000");
        }
        _ => panic!("Expected bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_hex_bigint_with_separators() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0xFF_FFn;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "65535");
        }
        _ => panic!("Expected hex bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_binary_bigint_with_separators() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0b1010_0101n;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "165");
        }
        _ => panic!("Expected binary bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_octal_bigint_with_separators() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = 0o12_34n;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "668");
        }
        _ => panic!("Expected octal bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_negative_number_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = -42;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, -42.0);
        }
        _ => panic!("Expected negative number literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_negative_hex_number_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = -0x2A;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Number(num)) => {
            assert_eq!(num.0, -42.0);
        }
        _ => panic!("Expected negative hex literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_negative_bigint_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = -123n;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "-123");
        }
        _ => panic!("Expected negative bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_negative_hex_bigint_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = -0x2An;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::BigInt(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "-42");
        }
        _ => panic!("Expected negative hex bigint literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_literal_boolean_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = true;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::Boolean(true)) => {}
        _ => panic!("Expected boolean literal type, got {key:?}"),
    }
}

#[test]
fn test_lower_unique_symbol_type() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = unique symbol;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::UniqueSymbol(_) => {}
        _ => panic!("Expected unique symbol type, got {key:?}"),
    }
}

#[test]
fn test_lower_keyof_type_operator() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = keyof string;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::KeyOf(inner) => {
            assert_eq!(inner, TypeId::STRING);
        }
        _ => panic!("Expected keyof type, got {key:?}"),
    }
}

#[test]
fn test_lower_readonly_type_operator() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = readonly string[];");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::ReadonlyType(inner) => {
            let inner_key = interner.lookup(inner).expect("Inner type should exist");
            match inner_key {
                TypeData::Array(element) => {
                    assert_eq!(element, TypeId::STRING);
                }
                _ => panic!("Expected readonly array type, got {inner_key:?}"),
            }
        }
        _ => panic!("Expected readonly type, got {key:?}"),
    }
}

#[test]
fn test_lower_array_type_reference() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = Array<string>;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Array(element) => {
            assert_eq!(element, TypeId::STRING);
        }
        _ => panic!("Expected array type, got {key:?}"),
    }
}

#[test]
fn test_lower_readonly_array_type_reference() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = ReadonlyArray<string>;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::ReadonlyType(inner) => {
            let inner_key = interner.lookup(inner).expect("Inner type should exist");
            match inner_key {
                TypeData::Array(element) => {
                    assert_eq!(element, TypeId::STRING);
                }
                _ => panic!("Expected readonly array type, got {inner_key:?}"),
            }
        }
        _ => panic!("Expected readonly type, got {key:?}"),
    }
}

#[test]
fn test_lower_array_type_reference_respects_resolver() {
    use tsz_solver::def::DefId;

    // Use a custom type name (not built-in) to test resolver behavior
    let (arena, type_idx) = parse_type_alias_type_node("type T = MyArray<string>;");
    let interner = TypeInterner::new();

    // Use def_id_resolver for type identity
    let def_id_resolver = |node_idx: NodeIndex| {
        arena
            .get(node_idx)
            .and_then(|node| arena.get_identifier(node))
            .and_then(|ident| {
                if ident.escaped_text == "MyArray" {
                    Some(DefId(1))
                } else {
                    None
                }
            })
    };
    let value_resolver = |_node_idx: NodeIndex| None;
    // Use with_hybrid_resolver to provide def_id_resolver
    let lowering = TypeLowering::with_hybrid_resolver(
        &arena,
        &interner,
        &|_| None, // type_resolver not needed
        &def_id_resolver,
        &value_resolver,
    );

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            assert_eq!(app.args, vec![TypeId::STRING]);
            match interner.lookup(app.base) {
                Some(TypeData::Lazy(_def_id)) => {} // Uses Lazy(DefId)
                other => panic!("Expected Lazy base type, got {other:?}"),
            }
        }
        _ => panic!("Expected Application type, got {key:?}"),
    }
}

#[test]
fn test_lower_readonly_array_type_reference_respects_resolver() {
    use tsz_solver::def::DefId;

    // Use a custom type name (not built-in) to test resolver behavior
    let (arena, type_idx) = parse_type_alias_type_node("type T = MyReadonlyArray<string>;");
    let interner = TypeInterner::new();

    // Use def_id_resolver for type identity
    let def_id_resolver = |node_idx: NodeIndex| {
        arena
            .get(node_idx)
            .and_then(|node| arena.get_identifier(node))
            .and_then(|ident| {
                if ident.escaped_text == "MyReadonlyArray" {
                    Some(DefId(2))
                } else {
                    None
                }
            })
    };
    let value_resolver = |_node_idx: NodeIndex| None;
    // Use with_hybrid_resolver to provide def_id_resolver
    let lowering = TypeLowering::with_hybrid_resolver(
        &arena,
        &interner,
        &|_| None, // type_resolver not needed
        &def_id_resolver,
        &value_resolver,
    );

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            assert_eq!(app.args, vec![TypeId::STRING]);
            match interner.lookup(app.base) {
                Some(TypeData::Lazy(_def_id)) => {} // Uses Lazy(DefId)
                other => panic!("Expected Lazy base type, got {other:?}"),
            }
        }
        _ => panic!("Expected Application type, got {key:?}"),
    }
}

#[test]
fn test_lower_conditional_type_with_infer() {
    let (arena, type_idx) =
        parse_type_alias_type_node("type T = string extends infer R ? string : never;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Conditional(cond_id) => {
            let cond = interner.conditional_type(cond_id);
            assert_eq!(cond.check_type, TypeId::STRING);
            assert_eq!(cond.true_type, TypeId::STRING);
            assert_eq!(cond.false_type, TypeId::NEVER);
            match interner.lookup(cond.extends_type) {
                Some(TypeData::Infer(info)) => {
                    assert_eq!(interner.resolve_atom(info.name), "R");
                    assert!(info.constraint.is_none());
                }
                other => panic!("Expected infer type in extends, got {other:?}"),
            }
        }
        _ => panic!("Expected Conditional type, got {key:?}"),
    }
}

#[test]
fn test_lower_infer_type_with_constraint() {
    let (arena, type_idx) = parse_type_alias_type_node(
        "type T = string extends infer R extends string ? string : never;",
    );
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Conditional(cond_id) => {
            let cond = interner.conditional_type(cond_id);
            match interner.lookup(cond.extends_type) {
                Some(TypeData::Infer(info)) => {
                    assert_eq!(interner.resolve_atom(info.name), "R");
                    assert_eq!(info.constraint, Some(TypeId::STRING));
                }
                other => panic!("Expected infer type in extends, got {other:?}"),
            }
        }
        _ => panic!("Expected Conditional type, got {key:?}"),
    }
}

#[test]
fn test_lower_conditional_infer_binding() {
    let (arena, type_idx) =
        parse_type_alias_type_node("type T = string extends infer R ? R : never;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Conditional(cond_id) => {
            let cond = interner.conditional_type(cond_id);
            assert_eq!(cond.true_type, cond.extends_type);
            match interner.lookup(cond.true_type) {
                Some(TypeData::Infer(info)) => {
                    assert_eq!(interner.resolve_atom(info.name), "R");
                }
                other => panic!("Expected infer type in true branch, got {other:?}"),
            }
        }
        _ => panic!("Expected Conditional type, got {key:?}"),
    }
}

#[test]
fn test_lower_conditional_infer_binding_false_branch() {
    let (arena, type_idx) =
        parse_type_alias_type_node("type T = string extends infer R ? never : R;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Conditional(cond_id) => {
            let cond = interner.conditional_type(cond_id);
            assert_eq!(cond.true_type, TypeId::NEVER);
            assert_eq!(cond.false_type, cond.extends_type);
            match interner.lookup(cond.false_type) {
                Some(TypeData::Infer(info)) => {
                    assert_eq!(interner.resolve_atom(info.name), "R");
                }
                other => panic!("Expected infer type in false branch, got {other:?}"),
            }
        }
        _ => panic!("Expected Conditional type, got {key:?}"),
    }
}

#[test]
fn test_lower_conditional_distributive_flag() {
    let (arena, func_idx) =
        parse_type_alias("type F = <T>() => T extends string ? number : boolean;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            match interner.lookup(shape.return_type) {
                Some(TypeData::Conditional(cond_id)) => {
                    let cond = interner.conditional_type(cond_id);
                    assert!(cond.is_distributive);
                }
                other => panic!("Expected conditional return type, got {other:?}"),
            }
        }
        _ => panic!("Expected function type, got {key:?}"),
    }
}

#[test]
fn test_lower_conditional_non_distributive_flag() {
    let (arena, func_idx) =
        parse_type_alias("type F = <T>() => [T] extends [string] ? number : boolean;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            match interner.lookup(shape.return_type) {
                Some(TypeData::Conditional(cond_id)) => {
                    let cond = interner.conditional_type(cond_id);
                    assert!(!cond.is_distributive);
                }
                other => panic!("Expected conditional return type, got {other:?}"),
            }
        }
        _ => panic!("Expected function type, got {key:?}"),
    }
}

#[test]
fn test_lower_deduplicates_identical_types() {
    let (arena_one, type_one) = parse_type_alias_type_node("type A = \"same\";");
    let (arena_two, type_two) = parse_type_alias_type_node("type B = \"same\";");
    let interner = TypeInterner::new();

    let lowering_one = TypeLowering::new(&arena_one, &interner);
    let lowering_two = TypeLowering::new(&arena_two, &interner);

    let type_id_one = lowering_one.lower_type(type_one);
    let type_id_two = lowering_two.lower_type(type_two);

    assert_eq!(type_id_one, type_id_two);
}

// =============================================================================
// Type Parameter Lowering Tests
// =============================================================================

/// Parse `source` as `"test.ts"`, assert no parse diagnostics, and return
/// the owned arena. Used as the shared prelude for the per-kind
/// `parse_…` helpers below.
///
/// Rationale: workstream 8 item 9 in `docs/plan/ROADMAP.md`
/// ("Create parser/scanner/binder/lowering fixtures").
fn parse_and_take_arena(source: &str) -> NodeArena {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );
    std::mem::take(&mut parser.arena)
}

/// Helper to parse a type alias and return the type node index
fn parse_type_alias(source: &str) -> (NodeArena, tsz_parser::parser::base::NodeIndex) {
    let arena = parse_and_take_arena(source);

    // Find the function-type or constructor-type node inside the alias.
    for i in 0..arena.len() {
        let idx = tsz_parser::parser::base::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && (node.kind == syntax_kind_ext::FUNCTION_TYPE
                || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE)
        {
            return (arena, idx);
        }
    }

    panic!("Could not find function type in parsed AST");
}

/// Helper to parse a type alias and return its type node index
fn parse_type_alias_type_node(source: &str) -> (NodeArena, tsz_parser::parser::base::NodeIndex) {
    let arena = parse_and_take_arena(source);
    let mut type_node = tsz_parser::parser::base::NodeIndex::NONE;
    for i in 0..arena.len() {
        let idx = tsz_parser::parser::base::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            && let Some(alias) = arena.get_type_alias(node)
        {
            type_node = alias.type_node;
            break;
        }
    }

    if type_node == tsz_parser::parser::base::NodeIndex::NONE {
        panic!("Could not find type alias in parsed AST");
    }

    (arena, type_node)
}

/// Helper to parse a type alias and return the tuple type node index
fn parse_tuple_type(source: &str) -> (NodeArena, tsz_parser::parser::base::NodeIndex) {
    let arena = parse_and_take_arena(source);
    for i in 0..arena.len() {
        let idx = tsz_parser::parser::base::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::TUPLE_TYPE
        {
            return (arena, idx);
        }
    }

    panic!("Could not find tuple type in parsed AST");
}

/// Helper to parse a type alias and return the template literal type node index
fn parse_template_literal_type(source: &str) -> (NodeArena, tsz_parser::parser::base::NodeIndex) {
    let arena = parse_and_take_arena(source);
    for i in 0..arena.len() {
        let idx = tsz_parser::parser::base::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE
        {
            return (arena, idx);
        }
    }

    panic!("Could not find template literal type in parsed AST");
}

/// Helper to parse a type alias and return the mapped type node index.
fn parse_mapped_type(source: &str) -> (NodeArena, tsz_parser::parser::base::NodeIndex) {
    let arena = parse_and_take_arena(source);
    for i in 0..arena.len() {
        let idx = tsz_parser::parser::base::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::MAPPED_TYPE
        {
            return (arena, idx);
        }
    }

    panic!("Could not find mapped type in parsed AST");
}

/// Helper to parse a type alias and return the type reference node index for a name.
fn parse_type_reference(
    source: &str,
    name: &str,
) -> (NodeArena, tsz_parser::parser::base::NodeIndex) {
    let arena = parse_and_take_arena(source);
    for i in 0..arena.len() {
        let idx = tsz_parser::parser::base::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(data) = arena.get_type_ref(node)
            && let Some(type_name_node) = arena.get(data.type_name)
            && let Some(ident) = arena.get_identifier(type_name_node)
            && ident.escaped_text == name
        {
            return (arena, idx);
        }
    }

    panic!("Could not find type reference in parsed AST");
}

/// Helper to parse a type alias and return the type literal node index.
fn parse_type_literal(source: &str) -> (NodeArena, tsz_parser::parser::base::NodeIndex) {
    let arena = parse_and_take_arena(source);
    for i in 0..arena.len() {
        let idx = tsz_parser::parser::base::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::TYPE_LITERAL
        {
            return (arena, idx);
        }
    }

    panic!("Could not find type literal in parsed AST");
}

/// Helper to parse interface declarations by name.
fn parse_interface_declarations(source: &str, name: &str) -> (NodeArena, Vec<NodeIndex>) {
    let arena = parse_and_take_arena(source);
    let mut declarations = Vec::new();
    for i in 0..arena.len() {
        let idx = tsz_parser::parser::base::NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            && let Some(interface) = arena.get_interface(node)
            && let Some(name_node) = arena.get(interface.name)
            && let Some(ident) = arena.get_identifier(name_node)
            && ident.escaped_text == name
        {
            declarations.push(idx);
        }
    }

    assert!(
        !declarations.is_empty(),
        "Could not find interface '{name}'"
    );
    (arena, declarations)
}

#[test]
fn test_lower_function_type_with_type_parameter() {
    // Parse: type F = <T>(x: T) => T
    let (arena, func_type_idx) = parse_type_alias("type F = <T>(x: T) => T;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);

    // Verify it's a function type
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            // Should have 1 type parameter named "T"
            assert_eq!(shape.type_params.len(), 1, "Expected 1 type parameter");
            assert_eq!(
                interner.resolve_atom(shape.type_params[0].name).as_str(),
                "T"
            );
            assert!(
                shape.type_params[0].constraint.is_none(),
                "T should have no constraint"
            );
            assert!(
                shape.type_params[0].default.is_none(),
                "T should have no default"
            );
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_type_predicate_return() {
    let (arena, func_type_idx) = parse_type_alias("type F = (x: any) => x is string;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::BOOLEAN);
            let predicate = shape
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(!predicate.asserts);
            match predicate.target {
                TypePredicateTarget::Identifier(atom) => {
                    assert_eq!(interner.resolve_atom(atom).as_str(), "x");
                }
                _ => panic!("Expected identifier predicate target"),
            }
            assert_eq!(predicate.type_id, Some(TypeId::STRING));
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_this_predicate_return() {
    let (arena, func_type_idx) = parse_type_alias("type F = (this: any) => this is string;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::BOOLEAN);
            let predicate = shape
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(!predicate.asserts);
            match predicate.target {
                TypePredicateTarget::This => {}
                _ => panic!("Expected this predicate target"),
            }
            assert_eq!(predicate.type_id, Some(TypeId::STRING));
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_asserts_predicate_return() {
    let (arena, func_type_idx) = parse_type_alias("type F = (x: any) => asserts x is string;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::VOID);
            let predicate = shape
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(predicate.asserts);
            match predicate.target {
                TypePredicateTarget::Identifier(atom) => {
                    assert_eq!(interner.resolve_atom(atom).as_str(), "x");
                }
                _ => panic!("Expected identifier predicate target"),
            }
            assert_eq!(predicate.type_id, Some(TypeId::STRING));
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_asserts_this_predicate_return() {
    let (arena, func_type_idx) =
        parse_type_alias("type F = (this: any) => asserts this is string;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::VOID);
            let predicate = shape
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(predicate.asserts);
            match predicate.target {
                TypePredicateTarget::This => {}
                _ => panic!("Expected this predicate target"),
            }
            assert_eq!(predicate.type_id, Some(TypeId::STRING));
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_asserts_this_predicate_without_is() {
    let (arena, func_type_idx) = parse_type_alias("type F = (this: any) => asserts this;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::VOID);
            let predicate = shape
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(predicate.asserts);
            match predicate.target {
                TypePredicateTarget::This => {}
                _ => panic!("Expected this predicate target"),
            }
            assert_eq!(predicate.type_id, None);
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_asserts_predicate_without_is() {
    let (arena, func_type_idx) = parse_type_alias("type F = (x: any) => asserts x;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.return_type, TypeId::VOID);
            let predicate = shape
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(predicate.asserts);
            match predicate.target {
                TypePredicateTarget::Identifier(atom) => {
                    assert_eq!(interner.resolve_atom(atom).as_str(), "x");
                }
                _ => panic!("Expected identifier predicate target"),
            }
            assert_eq!(predicate.type_id, None);
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_this_param_separate() {
    let (arena, func_type_idx) = parse_type_alias("type F = (this: any, x: string) => number;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.this_type, Some(TypeId::ANY));
            assert_eq!(shape.params.len(), 1);
            assert_eq!(shape.params[0].type_id, TypeId::STRING);
            let name = shape.params[0].name.expect("Expected parameter name");
            assert_eq!(interner.resolve_atom(name).as_str(), "x");
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_parameter_usage() {
    let (arena, func_type_idx) = parse_type_alias("type F = <T>(x: T) => T;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.params.len(), 1);
            assert_eq!(shape.params[0].type_id, shape.return_type);

            let param_key = interner
                .lookup(shape.params[0].type_id)
                .expect("Type should exist");
            match param_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(interner.resolve_atom(info.name), "T");
                }
                _ => panic!("Expected type parameter type, got {param_key:?}"),
            }
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_constrained_type_parameter() {
    // Parse: type F = <T extends string>(x: T) => T
    let (arena, func_type_idx) = parse_type_alias("type F = <T extends string>(x: T) => T;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);

    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.type_params.len(), 1);
            assert_eq!(
                interner.resolve_atom(shape.type_params[0].name).as_str(),
                "T"
            );
            // Should have a constraint
            assert!(
                shape.type_params[0].constraint.is_some(),
                "T should have constraint"
            );
            let constraint = shape.type_params[0].constraint.unwrap();
            assert_eq!(constraint, TypeId::STRING, "Constraint should be string");
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_constrained_type_parameter_usage() {
    let (arena, func_type_idx) = parse_type_alias("type F = <T extends string>(x: T) => T;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            let param_key = interner
                .lookup(shape.params[0].type_id)
                .expect("Type should exist");
            match param_key {
                TypeData::TypeParameter(info) => {
                    assert_eq!(info.constraint, Some(TypeId::STRING));
                }
                _ => panic!("Expected type parameter type, got {param_key:?}"),
            }
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_default_type_parameter() {
    // Parse: type F = <T = string>() => T
    let (arena, func_type_idx) = parse_type_alias("type F = <T = string>() => T;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);

    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.type_params.len(), 1);
            assert_eq!(
                interner.resolve_atom(shape.type_params[0].name).as_str(),
                "T"
            );
            assert!(shape.type_params[0].constraint.is_none());
            // Should have a default
            assert!(
                shape.type_params[0].default.is_some(),
                "T should have default"
            );
            let default = shape.type_params[0].default.unwrap();
            assert_eq!(default, TypeId::STRING, "Default should be string");
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_multiple_type_parameters() {
    // Parse: type F = <T, U, V>(x: T, y: U) => V
    let (arena, func_type_idx) = parse_type_alias("type F = <T, U, V>(x: T, y: U) => V;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);

    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.type_params.len(), 3, "Expected 3 type parameters");
            assert_eq!(
                interner.resolve_atom(shape.type_params[0].name).as_str(),
                "T"
            );
            assert_eq!(
                interner.resolve_atom(shape.type_params[1].name).as_str(),
                "U"
            );
            assert_eq!(
                interner.resolve_atom(shape.type_params[2].name).as_str(),
                "V"
            );
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_with_constraint_and_default() {
    // Parse: type F = <T extends object = {}>(x: T) => T
    let (arena, func_type_idx) = parse_type_alias("type F = <T extends object = {}>(x: T) => T;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);

    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.type_params.len(), 1);
            assert_eq!(
                interner.resolve_atom(shape.type_params[0].name).as_str(),
                "T"
            );
            // Should have both constraint and default
            assert!(
                shape.type_params[0].constraint.is_some(),
                "T should have constraint"
            );
            assert!(
                shape.type_params[0].default.is_some(),
                "T should have default"
            );
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_type_no_type_parameters() {
    // Parse: type F = (x: string) => number
    let (arena, func_type_idx) = parse_type_alias("type F = (x: string) => number;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);

    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.type_params.len(), 0, "Expected no type parameters");
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_tuple_type_metadata() {
    let (arena, tuple_idx) = parse_tuple_type("type T = [x?: string, string?, ...number[]];");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(tuple_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Tuple(elements) => {
            let elements = interner.tuple_list(elements);
            assert_eq!(elements.len(), 3);

            let first = &elements[0];
            assert_eq!(
                first.name.map(|a| interner.resolve_atom(a)),
                Some("x".to_string())
            );
            assert!(first.optional);
            assert!(!first.rest);
            assert_eq!(first.type_id, TypeId::STRING);

            let second = &elements[1];
            assert!(second.name.is_none());
            assert!(second.optional);
            assert!(!second.rest);
            assert_eq!(second.type_id, TypeId::STRING);

            let third = &elements[2];
            assert!(third.name.is_none());
            assert!(!third.optional);
            assert!(third.rest);
            match interner.lookup(third.type_id) {
                Some(TypeData::Array(elem)) => assert_eq!(elem, TypeId::NUMBER),
                other => panic!("Expected array type for rest element, got {other:?}"),
            }
        }
        _ => panic!("Expected Tuple type, got {key:?}"),
    }
}

#[test]
fn test_lower_union_type_normalization() {
    let (arena, union_idx) = parse_type_alias_type_node("type T = string | number | string;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(union_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            // string | number | string deduplicates to string | number (insertion order)
            assert_eq!(members.as_ref(), [TypeId::STRING, TypeId::NUMBER]);
        }
        _ => panic!("Expected Union type, got {key:?}"),
    }
}

#[test]
fn test_lower_intersection_type_normalization() {
    let (arena, intersection_idx) =
        parse_type_alias_type_node("type T = string & number & string;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(intersection_idx);
    assert_eq!(type_id, TypeId::NEVER);
}

#[test]
fn test_lower_function_parameter_names() {
    let (arena, func_type_idx) = parse_type_alias("type F = (x: string, y?: number) => void;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.params.len(), 2);
            assert_eq!(
                shape.params[0].name.map(|a| interner.resolve_atom(a)),
                Some("x".to_string())
            );
            assert_eq!(shape.params[0].type_id, TypeId::STRING);
            assert!(!shape.params[0].optional);

            assert_eq!(
                shape.params[1].name.map(|a| interner.resolve_atom(a)),
                Some("y".to_string())
            );
            // Optional param `y?: number` is lowered to `number | undefined` to match tsc.
            // The type_id is a union, not plain NUMBER.
            assert_ne!(
                shape.params[1].type_id,
                TypeId::UNDEFINED,
                "optional param type should not be bare undefined"
            );
            assert!(shape.params[1].optional);

            assert_eq!(shape.return_type, TypeId::VOID);
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_function_rest_parameter() {
    let (arena, func_type_idx) = parse_type_alias("type F = (...args: string[]) => void;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            assert_eq!(shape.params.len(), 1);
            let param = &shape.params[0];
            assert_eq!(
                param.name.map(|a| interner.resolve_atom(a)),
                Some("args".to_string())
            );
            assert!(!param.optional);
            assert!(param.rest);

            let param_key = interner
                .lookup(param.type_id)
                .expect("Param type should exist");
            match param_key {
                TypeData::Array(element) => {
                    assert_eq!(element, TypeId::STRING);
                }
                _ => panic!("Expected rest param to be array type, got {param_key:?}"),
            }
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_generic_type_reference_uses_type_parameter_args() {
    use tsz_solver::def::DefId;

    let (arena, func_type_idx) = parse_type_alias("type F = <T>(x: T) => Box<T>;");
    let interner = TypeInterner::new();

    // Use def_id_resolver for type identity
    let def_id_resolver = |node_idx: NodeIndex| {
        arena
            .get(node_idx)
            .and_then(|node| arena.get_identifier(node))
            .and_then(|ident| {
                if ident.escaped_text == "Box" {
                    Some(DefId(1))
                } else {
                    None
                }
            })
    };

    // Use with_hybrid_resolver to provide def_id_resolver
    let lowering = TypeLowering::with_hybrid_resolver(
        &arena,
        &interner,
        &|_| None, // type_resolver not needed
        &def_id_resolver,
        &|_| None, // value_resolver not needed
    );

    let type_id = lowering.lower_type(func_type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Function(shape_id) => {
            let shape = interner.function_shape(shape_id);
            let return_key = interner
                .lookup(shape.return_type)
                .expect("Type should exist");
            match return_key {
                TypeData::Application(app_id) => {
                    let app = interner.type_application(app_id);
                    let base_key = interner.lookup(app.base).expect("Type should exist");
                    match base_key {
                        TypeData::Lazy(_def_id) => {} // Uses Lazy(DefId)
                        _ => panic!("Expected lazy base type, got {base_key:?}"),
                    }

                    assert_eq!(app.args.len(), 1);
                    let arg_key = interner.lookup(app.args[0]).expect("Type should exist");
                    match arg_key {
                        TypeData::TypeParameter(info) => {
                            assert_eq!(interner.resolve_atom(info.name), "T");
                        }
                        _ => panic!("Expected type parameter argument, got {arg_key:?}"),
                    }
                }
                _ => panic!("Expected application type, got {return_key:?}"),
            }
        }
        _ => panic!("Expected Function type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_reference_with_arguments() {
    use tsz_solver::def::DefId;

    let (arena, type_ref_idx) = parse_type_reference("type T = Box<string>;", "Box");
    let interner = TypeInterner::new();

    // Use def_id_resolver for type identity
    let def_id_resolver = |node_idx: NodeIndex| {
        arena
            .get(node_idx)
            .and_then(|node| arena.get_identifier(node))
            .and_then(|ident| {
                if ident.escaped_text == "Box" {
                    Some(DefId(1))
                } else {
                    None
                }
            })
    };

    // Use with_hybrid_resolver to provide def_id_resolver
    let lowering = TypeLowering::with_hybrid_resolver(
        &arena,
        &interner,
        &|_| None, // type_resolver not needed
        &def_id_resolver,
        &|_| None, // value_resolver not needed
    );

    let type_id = lowering.lower_type(type_ref_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            assert_eq!(app.args, vec![TypeId::STRING]);
            match interner.lookup(app.base) {
                Some(TypeData::Lazy(_def_id)) => {} // Uses Lazy(DefId)
                other => panic!("Expected Lazy base type, got {other:?}"),
            }
        }
        _ => panic!("Expected Application type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_query_uses_value_resolver() {
    use tsz_solver::def::DefId;

    let (arena, type_idx) = parse_type_alias_type_node("type T = Foo | typeof Foo;");
    let interner = TypeInterner::new();

    // Use def_id_resolver for Foo reference
    let def_id_resolver = |_node_idx: NodeIndex| Some(DefId(1));
    let type_resolver = |_node_idx: NodeIndex| None; // Not needed with def_id_resolver
    let value_resolver = |_node_idx: NodeIndex| Some(2);

    // Use with_hybrid_resolver to provide def_id_resolver and value_resolver
    let lowering = TypeLowering::with_hybrid_resolver(
        &arena,
        &interner,
        &type_resolver,
        &def_id_resolver,
        &value_resolver,
    );

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            let mut saw_lazy = false;
            let mut saw_query = false;
            for &member in members.iter() {
                match interner.lookup(member) {
                    Some(TypeData::Lazy(_def_id)) => {
                        // Uses Lazy(DefId)
                        saw_lazy = true;
                    }
                    Some(TypeData::TypeQuery(SymbolRef(sym_id))) => {
                        assert_eq!(sym_id, 2);
                        saw_query = true;
                    }
                    other => panic!("Unexpected union member {other:?}"),
                }
            }
            assert!(saw_lazy, "Expected union to include lazy type reference");
            assert!(saw_query, "Expected union to include typeof query");
        }
        _ => panic!("Expected Union type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_query_with_type_arguments() {
    let (arena, type_idx) = parse_type_alias_type_node("type T = typeof Foo<string>;");
    let interner = TypeInterner::new();

    let type_resolver = |_node_idx: NodeIndex| None;
    let value_resolver = |node_idx: NodeIndex| {
        arena
            .get(node_idx)
            .and_then(|node| arena.get_identifier(node))
            .and_then(|ident| {
                if ident.escaped_text == "Foo" {
                    Some(2)
                } else {
                    None
                }
            })
    };
    let lowering = TypeLowering::with_resolvers(&arena, &interner, &type_resolver, &value_resolver);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            assert_eq!(app.args, vec![TypeId::STRING]);
            match interner.lookup(app.base) {
                Some(TypeData::TypeQuery(SymbolRef(sym_id))) => assert_eq!(sym_id, 2),
                other => panic!("Expected TypeQuery base type, got {other:?}"),
            }
        }
        _ => panic!("Expected Application type, got {key:?}"),
    }
}

#[test]
fn test_lower_template_literal_type_spans() {
    let (arena, template_idx) = parse_template_literal_type("type T = `hello${string}world`;");

    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 3);
            match spans[0] {
                TemplateSpan::Text(atom) => assert_eq!(interner.resolve_atom(atom), "hello"),
                _ => panic!("Expected head text span"),
            }
            match spans[1] {
                TemplateSpan::Type(t) => assert_eq!(t, TypeId::STRING),
                _ => panic!("Expected type span"),
            }
            match spans[2] {
                TemplateSpan::Text(atom) => assert_eq!(interner.resolve_atom(atom), "world"),
                _ => panic!("Expected tail text span"),
            }
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_lower_mapped_type_modifiers_and_constraint() {
    let (arena, mapped_idx) = parse_mapped_type("type T = { readonly [K in string]?: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(mapped_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Mapped(mapped_id) => {
            let mapped = interner.mapped_type(mapped_id);
            assert_eq!(interner.resolve_atom(mapped.type_param.name), "K");
            assert_eq!(mapped.constraint, TypeId::STRING);
            assert_eq!(mapped.template, TypeId::NUMBER);
            assert_eq!(mapped.readonly_modifier, Some(MappedModifier::Add));
            assert_eq!(mapped.optional_modifier, Some(MappedModifier::Add));
        }
        _ => panic!("Expected Mapped type, got {key:?}"),
    }
}

#[test]
fn test_lower_mapped_type_remove_modifiers() {
    let (arena, mapped_idx) = parse_mapped_type("type T = { -readonly [K in string]-?: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(mapped_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Mapped(mapped_id) => {
            let mapped = interner.mapped_type(mapped_id);
            assert_eq!(mapped.readonly_modifier, Some(MappedModifier::Remove));
            assert_eq!(mapped.optional_modifier, Some(MappedModifier::Remove));
        }
        _ => panic!("Expected Mapped type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_object_properties() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { readonly foo?: string; bar: number; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let foo = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "foo")
                .expect("Expected foo property");
            assert_eq!(foo.type_id, TypeId::STRING);
            assert!(foo.optional);
            assert!(foo.readonly);

            let bar = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "bar")
                .expect("Expected bar property");
            assert_eq!(bar.type_id, TypeId::NUMBER);
            assert!(!bar.optional);
            assert!(!bar.readonly);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_nested_object() {
    let (arena, literal_idx) =
        parse_type_alias_type_node("type T = { config: { enabled: boolean; retries?: number }; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let config = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "config")
                .expect("Expected config property");

            match interner.lookup(config.type_id) {
                Some(TypeData::Object(nested_id)) => {
                    let nested = interner.object_shape(nested_id);
                    let enabled = nested
                        .properties
                        .iter()
                        .find(|prop| interner.resolve_atom(prop.name) == "enabled")
                        .expect("Expected enabled property");
                    assert_eq!(enabled.type_id, TypeId::BOOLEAN);
                    assert!(!enabled.optional);

                    let retries = nested
                        .properties
                        .iter()
                        .find(|prop| interner.resolve_atom(prop.name) == "retries")
                        .expect("Expected retries property");
                    assert_eq!(retries.type_id, TypeId::NUMBER);
                    assert!(retries.optional);
                }
                other => panic!("Expected nested Object type, got {other:?}"),
            }
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_call_signature() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { (x: string): number; foo: string; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 1);
            assert_eq!(callable.construct_signatures.len(), 0);
            assert_eq!(callable.properties.len(), 1);
            assert_eq!(interner.resolve_atom(callable.properties[0].name), "foo");
            assert_eq!(callable.properties[0].type_id, TypeId::STRING);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_call_signature_this_param() {
    let (arena, literal_idx) = parse_type_literal("type T = { (this: any, x: string): number; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 1);
            let sig = &callable.call_signatures[0];
            assert_eq!(sig.this_type, Some(TypeId::ANY));
            assert_eq!(sig.params.len(), 1);
            assert_eq!(sig.params[0].type_id, TypeId::STRING);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_call_signature_type_predicate() {
    let (arena, literal_idx) = parse_type_literal("type T = { (x: any): x is string; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 1);
            let sig = &callable.call_signatures[0];
            assert_eq!(sig.return_type, TypeId::BOOLEAN);
            let predicate = sig
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(!predicate.asserts);
            match predicate.target {
                TypePredicateTarget::Identifier(atom) => {
                    assert_eq!(interner.resolve_atom(atom).as_str(), "x");
                }
                _ => panic!("Expected identifier predicate target"),
            }
            assert_eq!(predicate.type_id, Some(TypeId::STRING));
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_call_signature_asserts_predicate_without_is() {
    let (arena, literal_idx) = parse_type_literal("type T = { (x: any): asserts x; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 1);
            let sig = &callable.call_signatures[0];
            assert_eq!(sig.return_type, TypeId::VOID);
            let predicate = sig
                .type_predicate
                .as_ref()
                .expect("Expected type predicate");
            assert!(predicate.asserts);
            match predicate.target {
                TypePredicateTarget::Identifier(atom) => {
                    assert_eq!(interner.resolve_atom(atom).as_str(), "x");
                }
                _ => panic!("Expected identifier predicate target"),
            }
            assert_eq!(predicate.type_id, None);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_overloaded_call_signatures() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { (x: string): number; (x: number): string; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 2);

            let first = &callable.call_signatures[0];
            assert_eq!(first.params.len(), 1);
            assert_eq!(first.params[0].type_id, TypeId::STRING);
            assert_eq!(first.return_type, TypeId::NUMBER);

            let second = &callable.call_signatures[1];
            assert_eq!(second.params.len(), 1);
            assert_eq!(second.params[0].type_id, TypeId::NUMBER);
            assert_eq!(second.return_type, TypeId::STRING);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_construct_signature() {
    let (arena, literal_idx) = parse_type_literal("type T = { new (x: string): number; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Callable(callable_id) => {
            let callable = interner.callable_shape(callable_id);
            assert_eq!(callable.call_signatures.len(), 0);
            assert_eq!(callable.construct_signatures.len(), 1);
        }
        _ => panic!("Expected Callable type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_index_signature() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { [key: string]: number; foo: number; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(shape.properties.len(), 1);
            assert_eq!(interner.resolve_atom(shape.properties[0].name), "foo");
            let string_index = shape
                .string_index
                .as_ref()
                .expect("Expected string index signature");
            assert_eq!(string_index.key_type, TypeId::STRING);
            assert_eq!(string_index.value_type, TypeId::NUMBER);
        }
        _ => panic!("Expected ObjectWithIndex type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_literal_index_signature_mismatch() {
    let (arena, literal_idx) =
        parse_type_literal("type T = { [key: string]: number; foo: string; };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    assert_ne!(type_id, TypeId::ERROR);
}

#[test]
fn test_lower_interface_index_signature_mismatch() {
    let source = "interface Foo { [key: string]: number; foo: string; }";
    let (arena, declarations) = parse_interface_declarations(source, "Foo");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    assert_ne!(type_id, TypeId::ERROR);
}

#[test]
fn test_lower_interface_single_with_two_properties() {
    // Regression test: Single interface with two properties
    let source = "interface Point { x: number; y: number; }";
    let (arena, declarations) = parse_interface_declarations(source, "Point");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            assert_eq!(
                shape.properties.len(),
                2,
                "Expected 2 properties, got {}",
                shape.properties.len()
            );

            let mut found_x = None;
            let mut found_y = None;
            for prop in &shape.properties {
                let name = interner.resolve_atom(prop.name);
                match name.as_str() {
                    "x" => found_x = Some(prop),
                    "y" => found_y = Some(prop),
                    other => panic!("Unexpected property name: {other}"),
                }
            }

            let x = found_x.expect("Expected property x");
            let y = found_y.expect("Expected property y");
            assert_eq!(x.type_id, TypeId::NUMBER, "Expected x to be number");
            assert_eq!(y.type_id, TypeId::NUMBER, "Expected y to be number");
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_interface_merges_properties() {
    let source = "interface Foo { a: string; } interface Foo { b?: number; }";
    let (arena, declarations) = parse_interface_declarations(source, "Foo");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let mut found_a = None;
            let mut found_b = None;
            for prop in &shape.properties {
                match interner.resolve_atom(prop.name).as_str() {
                    "a" => found_a = Some(prop),
                    "b" => found_b = Some(prop),
                    _ => {}
                }
            }

            let a = found_a.expect("Expected property a");
            let b = found_b.expect("Expected property b");
            assert_eq!(a.type_id, TypeId::STRING);
            assert!(!a.optional);
            assert_eq!(b.type_id, TypeId::NUMBER);
            assert!(b.optional);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_interface_conflicting_property_types() {
    let source = "interface Foo { a: string; } interface Foo { a: number; }";
    let (arena, declarations) = parse_interface_declarations(source, "Foo");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "a")
                .expect("Expected property a");
            assert_eq!(prop.type_id, TypeId::ERROR);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_interface_method_overload_accumulates() {
    let source =
        "interface Foo { bar(x: string): number; } interface Foo { bar(x: number): string; }";
    let (arena, declarations) = parse_interface_declarations(source, "Foo");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_interface_declarations(&declarations);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| interner.resolve_atom(prop.name) == "bar")
                .expect("Expected property bar");
            let prop_key = interner.lookup(prop.type_id).expect("Type should exist");
            match prop_key {
                TypeData::Callable(callable_id) => {
                    let callable = interner.callable_shape(callable_id);
                    assert_eq!(callable.call_signatures.len(), 2);
                    let mut combos: Vec<(TypeId, TypeId)> = callable
                        .call_signatures
                        .iter()
                        .map(|sig| (sig.params[0].type_id, sig.return_type))
                        .collect();
                    combos.sort_by_key(|(param, _)| param.0);
                    assert_eq!(
                        combos,
                        vec![
                            (TypeId::NUMBER, TypeId::STRING),
                            (TypeId::STRING, TypeId::NUMBER)
                        ]
                    );
                }
                _ => panic!("Expected Callable type, got {prop_key:?}"),
            }
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

// ============================================================================
// Template Literal Edge Case Tests
// ============================================================================

#[test]
fn test_template_literal_empty_string() {
    let (arena, template_idx) = parse_template_literal_type("type T = ``;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Empty template literal is collapsed to empty string literal
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "");
        }
        _ => panic!("Expected empty string Literal type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_single_text_span() {
    let (arena, template_idx) = parse_template_literal_type("type T = `hello`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Text-only templates are collapsed to string literals
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            assert_eq!(interner.resolve_atom(atom), "hello");
        }
        _ => panic!("Expected string Literal type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_multiple_interpolations() {
    let (arena, template_idx) =
        parse_template_literal_type("type T = `${string}-${number}-${boolean}`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 5); // type, text, type, text, type

            assert!(matches!(spans[0], TemplateSpan::Type(TypeId::STRING)));
            if let TemplateSpan::Text(atom) = &spans[1] {
                assert_eq!(interner.resolve_atom(*atom), "-");
            } else {
                panic!("Expected text span");
            }
            assert!(matches!(spans[2], TemplateSpan::Type(TypeId::NUMBER)));
            if let TemplateSpan::Text(atom) = &spans[3] {
                assert_eq!(interner.resolve_atom(*atom), "-");
            } else {
                panic!("Expected text span");
            }
            assert!(matches!(spans[4], TemplateSpan::Type(TypeId::BOOLEAN)));
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_consecutive_text_normalization() {
    let (arena, template_idx) = parse_template_literal_type("type T = `hello${string}world`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            // Should have 3 spans: "hello", string, "world"
            assert_eq!(spans.len(), 3);

            if let TemplateSpan::Text(atom) = &spans[0] {
                assert_eq!(interner.resolve_atom(*atom), "hello");
            } else {
                panic!("Expected text span");
            }

            if let TemplateSpan::Type(t) = spans[1] {
                assert_eq!(t, TypeId::STRING);
            } else {
                panic!("Expected type span");
            }

            if let TemplateSpan::Text(atom) = &spans[2] {
                assert_eq!(interner.resolve_atom(*atom), "world");
            } else {
                panic!("Expected text span");
            }
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_only_interpolation() {
    let (arena, template_idx) = parse_template_literal_type("type T = `${string}`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 1);
            assert!(matches!(spans[0], TemplateSpan::Type(TypeId::STRING)));
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_trailing_text() {
    let (arena, template_idx) = parse_template_literal_type("type T = `${string}!`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 2);
            assert!(matches!(spans[0], TemplateSpan::Type(TypeId::STRING)));
            if let TemplateSpan::Text(atom) = &spans[1] {
                assert_eq!(interner.resolve_atom(*atom), "!");
            } else {
                panic!("Expected text span");
            }
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_leading_text() {
    let (arena, template_idx) = parse_template_literal_type("type T = `!${string}`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 2);
            if let TemplateSpan::Text(atom) = &spans[0] {
                assert_eq!(interner.resolve_atom(*atom), "!");
            } else {
                panic!("Expected text span");
            }
            assert!(matches!(spans[1], TemplateSpan::Type(TypeId::STRING)));
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_escape_sequences() {
    let (arena, template_idx) = parse_template_literal_type(r#"type T = `hello\nworld`;"#);
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Text-only templates are collapsed to string literals
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            // The escape sequence should be processed
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "hello\nworld");
        }
        _ => panic!("Expected string Literal type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_escape_dollar_brace() {
    let (arena, template_idx) = parse_template_literal_type(r#"type T = `hello\${string}`;"#);
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Text-only templates are collapsed to string literals
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            // The escaped ${ should become literal ${ (not an interpolation)
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "hello${string}");
        }
        _ => panic!("Expected string Literal type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_with_union() {
    let (arena, template_idx) =
        parse_template_literal_type("type T = `prefix-${\"a\" | \"b\"}-suffix`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    // Should not exceed expansion limit and create a union
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Union(list_id) => {
            let members = interner.type_list(list_id);
            // Should have expanded to "prefix-a-suffix" | "prefix-b-suffix"
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected Union type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_with_multiple_unions() {
    // Test Cartesian product: `${"a" | "b"}-${"x" | "y"}` should produce 4 combinations
    let interner = TypeInterner::new();

    let a = interner.literal_string("a");
    let b = interner.literal_string("b");
    let union1 = interner.union(vec![a, b]);

    let x = interner.literal_string("x");
    let y = interner.literal_string("y");
    let union2 = interner.union(vec![x, y]);

    let spans = vec![
        TemplateSpan::Type(union1),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(union2),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Union(list_id) => {
            let members = interner.type_list(list_id);
            // Should have 4 combinations: "a-x", "a-y", "b-x", "b-y"
            assert_eq!(members.len(), 4);

            // Verify all expected strings are present
            let mut strings: Vec<String> = members
                .iter()
                .filter_map(|&m| match interner.lookup(m) {
                    Some(TypeData::Literal(LiteralValue::String(atom))) => {
                        Some(interner.resolve_atom(atom))
                    }
                    _ => None,
                })
                .collect();
            strings.sort();
            assert_eq!(strings, vec!["a-x", "a-y", "b-x", "b-y"]);
        }
        _ => panic!("Expected Union type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_single_string_literal() {
    // Template literal with single string literal interpolation should collapse to string literal
    let interner = TypeInterner::new();

    let a = interner.literal_string("hello");
    let spans = vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(a),
        TemplateSpan::Text(interner.intern_string("-suffix")),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "prefix-hello-suffix");
        }
        _ => panic!("Expected Literal type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_only_texts_becomes_literal() {
    // Template literal with only text spans should collapse to string literal
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Text(interner.intern_string(" ")),
        TemplateSpan::Text(interner.intern_string("world")),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            let text = interner.resolve_atom(atom);
            assert_eq!(text, "hello world");
        }
        _ => panic!("Expected Literal type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_with_non_string_literal_stays_template() {
    // Template literal with non-expandable types should stay as template literal
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(TypeId::STRING), // string primitive, not expandable
        TemplateSpan::Text(interner.intern_string("-suffix")),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(_) => {
            // Expected: remains as template literal type
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_normalization_merges_consecutive_texts() {
    let interner = TypeInterner::new();

    // Create spans with consecutive text that should be merged
    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Text(interner.intern_string(" ")),
        TemplateSpan::Text(interner.intern_string("world")),
    ];

    let type_id = interner.template_literal(spans);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Text-only templates are collapsed to string literals
    match key {
        TypeData::Literal(LiteralValue::String(atom)) => {
            // After normalization and expansion, text-only becomes string literal
            assert_eq!(interner.resolve_atom(atom), "hello world");
        }
        _ => panic!("Expected string Literal type, got {key:?}"),
    }
}

#[test]
fn test_template_literal_interpolation_positions() {
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("prefix")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("-")),
        TemplateSpan::Type(TypeId::NUMBER),
    ];

    let type_id = interner.template_literal(spans);
    let positions = interner.template_literal_interpolation_positions(type_id);

    assert_eq!(positions, vec![1, 3]); // Type spans at indices 1 and 3
}

#[test]
fn test_template_literal_get_span() {
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Type(TypeId::STRING),
    ];

    let type_id = interner.template_literal(spans);

    let span_0 = interner.template_literal_get_span(type_id, 0);
    assert!(span_0.is_some());
    assert!(span_0.unwrap().is_text());

    let span_1 = interner.template_literal_get_span(type_id, 1);
    assert!(span_1.is_some());
    assert!(span_1.unwrap().is_type());

    let span_2 = interner.template_literal_get_span(type_id, 2);
    assert!(span_2.is_none()); // Out of bounds
}

#[test]
fn test_template_literal_span_count() {
    let interner = TypeInterner::new();

    let spans = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Type(TypeId::STRING),
        TemplateSpan::Text(interner.intern_string("world")),
    ];

    let type_id = interner.template_literal(spans);
    assert_eq!(interner.template_literal_span_count(type_id), 3);
}

#[test]
fn test_template_literal_is_text_only() {
    let interner = TypeInterner::new();

    // Text only
    let spans_text_only = vec![TemplateSpan::Text(interner.intern_string("hello"))];
    let type_id_text_only = interner.template_literal(spans_text_only);
    assert!(interner.template_literal_is_text_only(type_id_text_only));

    // With interpolation
    let spans_with_type = vec![
        TemplateSpan::Text(interner.intern_string("hello")),
        TemplateSpan::Type(TypeId::STRING),
    ];
    let type_id_with_type = interner.template_literal(spans_with_type);
    assert!(!interner.template_literal_is_text_only(type_id_with_type));

    // Non-template literal type
    assert!(!interner.template_literal_is_text_only(TypeId::STRING));
}

// =============================================================================
// Interface Merge Ordering Tests
// =============================================================================

/// Helper to find all interface declarations for a given name in the arena
fn find_interface_declarations(arena: &NodeArena, name: &str) -> Vec<NodeIndex> {
    let mut decls = Vec::new();
    for i in 0..arena.len() {
        let idx = NodeIndex(i as u32);
        if let Some(node) = arena.get(idx)
            && node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            && let Some(interface) = arena.get_interface(node)
            && let Some(name_node) = arena.get(interface.name)
            && let Some(id_data) = arena.get_identifier(name_node)
            && id_data.escaped_text == name
        {
            decls.push(idx);
        }
    }
    decls
}

/// TypeScript's interface merging puts later declarations' method overloads first.
/// This is critical for overload resolution: e.g., `PromiseConstructor`'s tuple overload
/// from es2015.promise.d.ts (later) should be tried before the Iterable overload from
/// es2015.iterable.d.ts (earlier).
#[test]
fn test_merged_interface_method_overloads_later_first() {
    // Two interface declarations for Foo, each with a method bar(...)
    // Declaration 1 has bar(x: string): string
    // Declaration 2 has bar(x: number): number
    // After merging, bar's overloads should be [number->number, string->string]
    // (later declaration first)
    let source = r#"
interface Foo {
    bar(x: string): string;
}
interface Foo {
    bar(x: number): number;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let arena = std::mem::take(&mut parser.arena);
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let decls = find_interface_declarations(&arena, "Foo");
    assert_eq!(decls.len(), 2, "Should find 2 interface declarations");

    let type_id = lowering.lower_interface_declarations(&decls);

    // The result should be an Object or Callable type with the bar method
    let type_data = interner.lookup(type_id).expect("Type should exist");
    match type_data {
        TypeData::Callable(callable_shape_id) => {
            let callable = interner.callable_shape(callable_shape_id);
            // bar should have 2 call signatures
            assert_eq!(callable.call_signatures.len(), 2, "Should have 2 overloads");
            // The second declaration's overload (number->number) should be first
            let first_sig = &callable.call_signatures[0];
            assert_eq!(
                first_sig.return_type,
                TypeId::NUMBER,
                "First overload should be from later declaration (number->number)"
            );
            let second_sig = &callable.call_signatures[1];
            assert_eq!(
                second_sig.return_type,
                TypeId::STRING,
                "Second overload should be from earlier declaration (string->string)"
            );
        }
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = interner.object_shape(shape_id);
            // Find the bar property
            let bar_prop = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "bar")
                .expect("Should have bar property");
            // bar should be a callable with 2 overloads
            let bar_data = interner
                .lookup(bar_prop.type_id)
                .expect("bar type should exist");
            match bar_data {
                TypeData::Callable(callable_shape_id) => {
                    let callable = interner.callable_shape(callable_shape_id);
                    assert_eq!(callable.call_signatures.len(), 2, "Should have 2 overloads");
                    // Later declaration's overload should be first
                    let first_sig = &callable.call_signatures[0];
                    assert_eq!(
                        first_sig.return_type,
                        TypeId::NUMBER,
                        "First overload should be from later declaration (number->number)"
                    );
                    let second_sig = &callable.call_signatures[1];
                    assert_eq!(
                        second_sig.return_type,
                        TypeId::STRING,
                        "Second overload should be from earlier declaration (string->string)"
                    );
                }
                _ => panic!("Expected Callable type for bar, got {bar_data:?}"),
            }
        }
        _ => panic!("Expected Object or Callable type, got {type_data:?}"),
    }
}

// =============================================================================
// Advanced Type Lowering Tests
// =============================================================================

#[test]
fn test_lower_nested_generics() {
    // Map<string, Map<number, boolean>> - nested generic type application
    let (arena, type_idx) =
        parse_type_alias_type_node("type T = Map<string, Map<number, boolean>>;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Should be an Application type with nested Application as an argument
    match key {
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            assert_eq!(app.args.len(), 2);
            // First arg should be STRING
            assert_eq!(app.args[0], TypeId::STRING);
            // Second arg should be another Application (Map<number, boolean>)
            match interner.lookup(app.args[1]) {
                Some(TypeData::Application(_)) => {} // Expected nested Application
                other => panic!("Expected nested Application type, got {other:?}"),
            }
        }
        _ => panic!("Expected Application type, got {key:?}"),
    }
}

#[test]
fn test_lower_type_with_multiple_type_params() {
    // T<T1, T2, T3> - generic type with 3 type arguments
    let (arena, type_idx) = parse_type_alias_type_node("type X = Record<string, number, boolean>;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Application(app_id) => {
            let app = interner.type_application(app_id);
            assert_eq!(app.args.len(), 3);
            assert_eq!(app.args[0], TypeId::STRING);
            assert_eq!(app.args[1], TypeId::NUMBER);
            assert_eq!(app.args[2], TypeId::BOOLEAN);
        }
        _ => panic!("Expected Application type, got {key:?}"),
    }
}

#[test]
fn test_lower_mapped_type_keyof() {
    // { [K in keyof T]: T[K] } - mapped type with keyof and indexed access
    let (arena, mapped_idx) = parse_mapped_type("type T<U> = { [K in keyof U]: U[K] };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(mapped_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Mapped(mapped_id) => {
            let mapped = interner.mapped_type(mapped_id);
            assert_eq!(interner.resolve_atom(mapped.type_param.name), "K");
            // constraint should be KeyOf type
            match interner.lookup(mapped.constraint) {
                Some(TypeData::KeyOf(_)) => {} // Expected
                other => panic!("Expected KeyOf constraint, got {other:?}"),
            }
            // template should be IndexAccess type
            match interner.lookup(mapped.template) {
                Some(TypeData::IndexAccess(_, _)) => {} // Expected
                other => panic!("Expected IndexAccess template, got {other:?}"),
            }
        }
        _ => panic!("Expected Mapped type, got {key:?}"),
    }
}

#[test]
fn test_lower_conditional_type_infer() {
    // T extends Array<infer U> ? U : T - conditional with infer in array
    let (arena, type_idx) =
        parse_type_alias_type_node("type Unwrap<T> = T extends Array<infer U> ? U : T;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Conditional(cond_id) => {
            let cond = interner.conditional_type(cond_id);
            // extends_type should be an Array with infer U as element
            match interner.lookup(cond.extends_type) {
                Some(TypeData::Array(elem)) => match interner.lookup(elem) {
                    Some(TypeData::Infer(info)) => {
                        assert_eq!(interner.resolve_atom(info.name), "U");
                    }
                    other => panic!("Expected Infer type in array element, got {other:?}"),
                },
                other => panic!("Expected Array type in extends, got {other:?}"),
            }
        }
        _ => panic!("Expected Conditional type, got {key:?}"),
    }
}

#[test]
fn test_lower_template_literal_type() {
    // `on${string}` - template literal with interpolation
    let (arena, template_idx) = parse_template_literal_type("type T = `on${string}`;");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(template_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::TemplateLiteral(spans) => {
            let spans = interner.template_list(spans);
            assert_eq!(spans.len(), 2);
            // First span: "on"
            if let TemplateSpan::Text(atom) = &spans[0] {
                assert_eq!(interner.resolve_atom(*atom), "on");
            } else {
                panic!("Expected text span");
            }
            // Second span: string type
            assert!(matches!(spans[1], TemplateSpan::Type(TypeId::STRING)));
        }
        _ => panic!("Expected TemplateLiteral type, got {key:?}"),
    }
}

#[test]
fn test_lower_index_access_type() {
    // T['key'] - indexed access type
    let (arena, type_idx) = parse_type_alias_type_node("type V<T> = T['key'];");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::IndexAccess(_obj_type, index_type) => {
            // Index should be a string literal "key"
            match interner.lookup(index_type) {
                Some(TypeData::Literal(LiteralValue::String(atom))) => {
                    assert_eq!(interner.resolve_atom(atom), "key");
                }
                other => panic!("Expected string literal index, got {other:?}"),
            }
        }
        _ => panic!("Expected IndexAccess type, got {key:?}"),
    }
}

#[test]
fn test_lower_keyof_type() {
    // keyof { a: string; b: number } - keyof type operator on concrete type
    // Note: The lowering produces a KeyOf type; evaluation to union happens in solver
    let (arena, type_idx) = parse_type_alias_type_node("type K = keyof { a: string; b: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Lowering produces a KeyOf type; the solver evaluates it to union of literals
    match key {
        TypeData::KeyOf(inner) => {
            // Inner should be an Object type with properties a and b
            match interner.lookup(inner) {
                Some(TypeData::Object(_)) => {} // Expected
                other => panic!("Expected Object type for inner, got {other:?}"),
            }
        }
        _ => panic!("Expected KeyOf type, got {key:?}"),
    }
}

#[test]
fn test_lower_tuple_type() {
    // [string, number, boolean] - tuple with 3 elements
    let (arena, tuple_idx) = parse_tuple_type("type T = [string, number, boolean];");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(tuple_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Tuple(elements) => {
            let elements = interner.tuple_list(elements);
            assert_eq!(elements.len(), 3);
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert_eq!(elements[1].type_id, TypeId::NUMBER);
            assert_eq!(elements[2].type_id, TypeId::BOOLEAN);
        }
        _ => panic!("Expected Tuple type, got {key:?}"),
    }
}

#[test]
fn test_lower_tuple_with_rest() {
    // [string, ...number[]] - tuple with rest element
    let (arena, tuple_idx) = parse_tuple_type("type T = [string, ...number[]];");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(tuple_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Tuple(elements) => {
            let elements = interner.tuple_list(elements);
            assert_eq!(elements.len(), 2);

            // First element: string
            assert_eq!(elements[0].type_id, TypeId::STRING);
            assert!(!elements[0].rest);

            // Second element: rest number[]
            assert!(elements[1].rest);
            match interner.lookup(elements[1].type_id) {
                Some(TypeData::Array(elem)) => assert_eq!(elem, TypeId::NUMBER),
                other => panic!("Expected Array type for rest element, got {other:?}"),
            }
        }
        _ => panic!("Expected Tuple type, got {key:?}"),
    }
}

#[test]
fn test_lower_optional_property() {
    // { name?: string } - object with optional property
    let (arena, literal_idx) = parse_type_literal("type T = { name?: string };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "name")
                .expect("Expected name property");
            assert_eq!(prop.type_id, TypeId::STRING);
            assert!(prop.optional);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_readonly_property() {
    // { readonly id: number } - object with readonly property
    let (arena, literal_idx) = parse_type_literal("type T = { readonly id: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(literal_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            let shape = interner.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "id")
                .expect("Expected id property");
            assert_eq!(prop.type_id, TypeId::NUMBER);
            assert!(prop.readonly);
        }
        _ => panic!("Expected Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_intersection_type() {
    // { a: string } & { b: number } - intersection of two object types
    // Note: The lowering normalizes object intersections into a merged Object type
    let (arena, type_idx) = parse_type_alias_type_node("type T = { a: string } & { b: number };");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    match key {
        TypeData::Object(shape_id) => {
            // The two objects should be merged into one with both properties
            let shape = interner.object_shape(shape_id);
            let prop_a = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "a");
            let prop_b = shape
                .properties
                .iter()
                .find(|p| interner.resolve_atom(p.name) == "b");
            assert!(prop_a.is_some(), "Should have property 'a'");
            assert!(prop_b.is_some(), "Should have property 'b'");
        }
        _ => panic!("Expected merged Object type, got {key:?}"),
    }
}

#[test]
fn test_lower_parenthesized_type() {
    // (string | number) - parenthesized union type
    let (arena, type_idx) = parse_type_alias_type_node("type T = (string | number);");
    let interner = TypeInterner::new();
    let lowering = TypeLowering::new(&arena, &interner);

    let type_id = lowering.lower_type(type_idx);
    let key = interner.lookup(type_id).expect("Type should exist");
    // Parentheses are typically transparent - should get the inner type (union)
    match key {
        TypeData::Union(members) => {
            let members = interner.type_list(members);
            assert_eq!(members.as_ref(), [TypeId::STRING, TypeId::NUMBER]);
        }
        _ => panic!("Expected Union type, got {key:?}"),
    }
}
