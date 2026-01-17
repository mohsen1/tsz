use super::*;

#[test]
fn test_parse_empty() {
    let mut parser = ParserState::new("test.ts".to_string(), "".to_string());
    let sf_idx = parser.parse_source_file();

    assert!(parser.arena.get(sf_idx).is_some());
}

#[test]
fn test_parse_variable_declaration() {
    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_expression() {
    let mut parser = ParserState::new("test.ts".to_string(), "1 + 2;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_function() {
    let mut parser = ParserState::new("test.ts".to_string(), "function foo() { return 1; }".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_function_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "type Fn = (x: number) => string;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        // Verify we parsed a type alias
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            // Verify the type is a FunctionType
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::FunctionType(_)), "Expected FunctionType");
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_constructor_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "type Ctor = new (x: number) => MyClass;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        // Verify we parsed a type alias
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            // Verify the type is a ConstructorType
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::ConstructorType(_)), "Expected ConstructorType");
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_generic_function_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "type GenericFn = <T>(x: T) => T;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            if let Node::FunctionType(func_type) = type_node {
                assert!(func_type.type_parameters.is_some(), "Expected type parameters");
            } else {
                panic!("Expected FunctionType");
            }
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_conditional_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "type Check<T> = T extends string ? 'yes' : 'no';".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::ConditionalType(_)), "Expected ConditionalType, got {:?}", type_node);
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_infer_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "type Unpacked<T> = T extends Array<infer U> ? U : T;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            // Should be a conditional type with infer inside
            assert!(matches!(type_node, Node::ConditionalType(_)), "Expected ConditionalType");
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_typeof() {
    let mut parser = ParserState::new("test.ts".to_string(), "type T = typeof myVariable;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::TypeQuery(_)), "Expected TypeQuery, got {:?}", type_node);
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_keyof() {
    let mut parser = ParserState::new("test.ts".to_string(), "type Keys = keyof { a: 1; b: 2 };".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::TypeOperator(_)), "Expected TypeOperator (keyof), got {:?}", type_node);
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_readonly_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "type ReadonlyArr = readonly number[];".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::TypeOperator(_)), "Expected TypeOperator (readonly), got {:?}", type_node);
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_mapped_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "type Readonly<T> = { readonly [K in keyof T]: T[K] };".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::MappedType(_)), "Expected MappedType, got {:?}", type_node);
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_indexed_access_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "type PropType = T['prop'];".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::IndexedAccessType(_)), "Expected IndexedAccessType, got {:?}", type_node);
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_mapped_type_with_as() {
    let mut parser = ParserState::new("test.ts".to_string(), "type Renamed<T> = { [K in keyof T as string]: T[K] };".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::TypeAliasDeclaration(type_alias) = stmt {
            let type_node = parser.arena.get(type_alias.type_node).unwrap();
            assert!(matches!(type_node, Node::MappedType(_)), "Expected MappedType, got {:?}", type_node);
        } else {
            panic!("Expected TypeAliasDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_json_serialization() {
    let mut parser = ParserState::new("test.ts".to_string(), "const x = 1;".to_string());
    let sf_idx = parser.parse_source_file();

    // Test node serialization
    let json = parser.serialize_node_to_json(NodeIndex(sf_idx.0));
    assert!(json.contains("\"kind\":"));
    assert!(json.contains("\"pos\":"));
    assert!(json.contains("\"end\":"));

    // Test arena serialization
    let arena_json = parser.get_arena_json();
    assert!(arena_json.starts_with("["));
    assert!(arena_json.ends_with("]"));
}

#[test]
fn test_parse_jsx_self_closing() {
    let mut parser = ParserState::new("test.tsx".to_string(), "const x = <Foo />;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        // Should have a variable statement with JSX self-closing element
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_jsx_element() {
    let mut parser = ParserState::new("test.tsx".to_string(), "const x = <div></div>;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_jsx_with_attributes() {
    let mut parser = ParserState::new("test.tsx".to_string(), "const x = <Button onClick={handler} disabled />;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_jsx_fragment() {
    let mut parser = ParserState::new("test.tsx".to_string(), "const x = <></>;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_decorator_class() {
    let mut parser = ParserState::new("test.ts".to_string(), "@Component class MyClass {}".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::ClassDeclaration(class_decl) = stmt {
            // Should have decorators in modifiers
            assert!(class_decl.modifiers.is_some());
        } else {
            panic!("Expected ClassDeclaration, got {:?}", stmt);
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_decorator_with_call() {
    let mut parser = ParserState::new("test.ts".to_string(), "@Injectable() class Service {}".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_multiple_decorators() {
    let mut parser = ParserState::new("test.ts".to_string(), "@A @B @C class Multi {}".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::ClassDeclaration(class_decl) = stmt {
            // Should have 3 decorators
            assert!(class_decl.modifiers.is_some());
            assert_eq!(class_decl.modifiers.as_ref().unwrap().len(), 3);
        } else {
            panic!("Expected ClassDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_assignment_expression() {
    let mut parser = ParserState::new("test.ts".to_string(), "i = i + 1;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::ExpressionStatement(expr_stmt) = stmt {
            // The expression should be a binary expression with = operator
            let expr = parser.arena.get(expr_stmt.expression).unwrap();
            assert!(matches!(expr, Node::BinaryExpression(_)), "Expected BinaryExpression for assignment");
        } else {
            panic!("Expected ExpressionStatement");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_assignment_in_block() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"
            while (x < 10) {
                x = x + 1;
            }
        "#.to_string(),
    );
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        assert!(matches!(stmt, Node::WhileStatement(_)), "Expected WhileStatement");
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_compound_assignment() {
    let mut parser = ParserState::new("test.ts".to_string(), "x += 5;".to_string());
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::ExpressionStatement(expr_stmt) = stmt {
            let expr = parser.arena.get(expr_stmt.expression).unwrap();
            if let Node::BinaryExpression(bin) = expr {
                assert_eq!(bin.operator_token, SyntaxKind::PlusEqualsToken);
            } else {
                panic!("Expected BinaryExpression");
            }
        } else {
            panic!("Expected ExpressionStatement");
        }
    } else {
        panic!("Expected SourceFile");
    }
}

#[test]
fn test_parse_index_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Dict { [key: string]: number; }".to_string(),
    );
    let sf_idx = parser.parse_source_file();

    let sf = parser.arena.get(sf_idx).unwrap();
    if let Node::SourceFile(source_file) = sf {
        assert_eq!(source_file.statements.len(), 1);
        let stmt = parser.arena.get(source_file.statements.nodes[0]).unwrap();
        if let Node::InterfaceDeclaration(iface) = stmt {
            assert_eq!(iface.members.len(), 1);
            let member = parser.arena.get(iface.members.nodes[0]).unwrap();
            assert!(
                matches!(member, Node::IndexSignatureDeclaration(_)),
                "Expected IndexSignatureDeclaration"
            );
        } else {
            panic!("Expected InterfaceDeclaration");
        }
    } else {
        panic!("Expected SourceFile");
    }
}
