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

    // The type alias is typically node 1 (after source file at 0)
    // We need to find the FunctionType node
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
