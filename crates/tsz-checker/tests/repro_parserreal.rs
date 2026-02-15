use crate::diagnostics::diagnostic_codes;
use crate::{CheckerOptions, CheckerState};
use tsz_binder::BinderState;
use tsz_binder::SymbolId;
use tsz_parser::parser::{NodeIndex, ParserState, node::NodeAccess};
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeData, TypeId, TypeInterner};

#[cfg(test)]
#[test]
fn repro_parser_real_14_type_ids() {
    let source = include_str!(
        "/Users/mohsenazimi/code/tsz-5/TypeScript/tests/cases/conformance/parser/ecmascript5/parserRealSource14.ts"
    );
    run_and_print_source_line(source, "parserRealSource14.ts", 36, 20, "clone");
}

#[cfg(test)]
#[test]
fn repro_parser_harness_type_ids() {
    let source = include_str!(
        "/Users/mohsenazimi/code/tsz-5/TypeScript/tests/cases/conformance/parser/ecmascript5/RealWorld/parserharness.ts"
    );
    run_and_print_source_line(source, "parserharness.ts", 611, 64, "Dataset");
}

#[cfg(test)]
fn run_and_print_source_line(
    source: &str,
    file_name: &str,
    line: usize,
    column: usize,
    expected_token: &str,
) {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);

    let mut line_starts = Vec::with_capacity(128);
    line_starts.push(0usize);
    for (offset, ch) in source.char_indices() {
        if ch == '\n' {
            line_starts.push(offset + 1);
        }
    }

    let mut target_pos: u32 = 0;
    if line > 0 && column > 0 {
        let line_start = line_starts
            .get(line - 1)
            .copied()
            .unwrap_or(*line_starts.last().unwrap_or(&0));
        target_pos = (line_start + column - 1) as u32;
    }

    if target_pos == 0 {
        panic!("invalid target position");
    }

    println!("=== {file_name} pos={line}:{column} ({target_pos}) ===");

    let mut found = false;
    let mut seen = std::collections::BTreeSet::<u32>::new();

    for (idx, node) in parser.get_arena().nodes.iter().enumerate() {
        if !(node.pos <= target_pos && target_pos < node.end) {
            continue;
        }

        let node_idx = NodeIndex(idx as u32);
        let node_type = checker.get_type_of_node(node_idx);
        let text = parser
            .get_arena()
            .get_identifier_text(node_idx)
            .unwrap_or("");
        let kind = node.kind;
        seen.insert(node_type.0);

        if kind == SyntaxKind::Identifier as u16 {
            println!(
                "cover id node={} kind={} pos={}..{} text={} -> {}",
                idx, kind, node.pos, node.end, text, node_type.0
            );

            if let Some(symbol) = checker.resolve_identifier_symbol(NodeIndex(idx as u32)) {
                println!("  symbol resolved for identifier: {}", symbol.0);
            }
            if let crate::symbol_resolver::TypeSymbolResolution::Type(sym) =
                checker.resolve_identifier_symbol_in_type_position(NodeIndex(idx as u32))
            {
                println!("  type-position symbol resolved: {}", sym.0);
            }
            if let crate::symbol_resolver::TypeSymbolResolution::ValueOnly(sym) =
                checker.resolve_identifier_symbol_in_type_position(NodeIndex(idx as u32))
            {
                println!("  type-position value-only symbol: {}", sym.0);
            }
        } else {
            println!(
                "cover node={} kind={} pos={}..{} -> {}",
                idx, kind, node.pos, node.end, node_type.0
            );
        }

        if !text.is_empty() && text.contains(expected_token) {
            found = true;
        }
    }

    if !seen.is_empty() {
        println!("type id summary:");
        for id in seen {
            if let Some(data) = checker.ctx.types.lookup(TypeId(id)) {
                println!("  {} => {:?}", id, data);
            }
        }
    }

    if !found {
        println!(
            "Note: expected token '{}' not found directly under target cover",
            expected_token
        );
    }

    for &idx in &[2453, 2547, 2548, 1531] {
        let Some(node) = parser.get_arena().get(NodeIndex(idx)) else {
            continue;
        };
        println!(
            "inspect node {} => kind={} pos={}..{}",
            idx, node.kind, node.pos, node.end
        );
        if let Some(class) = parser.get_arena().get_class(node) {
            if let Some(name_text) = parser.get_arena().get_identifier_text(class.name) {
                println!("  class name: {}", name_text);
            }
            println!("  class members start: {}", class.members.nodes.len());
        }
        if let Some(ext) = parser.get_arena().get_extended(NodeIndex(idx)) {
            println!("  parent node: {}", ext.parent.0);
        }
    }

    if let Some(ast_path_class_sym) =
        find_symbol_id_by_text(&checker, parser.get_arena(), "AstPath")
    {
        if let Some(ast_path_instance) = checker.class_instance_type_from_symbol(ast_path_class_sym)
        {
            println!(
                "class_instance_type_from_symbol(AstPath) => {}",
                ast_path_instance.0
            );
            if let TypeData::ObjectWithIndex(shape_id) =
                checker.ctx.types.lookup(ast_path_instance).unwrap()
            {
                let shape = checker.ctx.types.object_shape(shape_id);
                println!(
                    "AstPath shape: symbol={:?} props={} string_index={:?} number_index={:?}",
                    shape.symbol,
                    shape.properties.len(),
                    shape.string_index,
                    shape.number_index
                );
            }
        }
    }

    if let Some(dataset_sym) = find_symbol_id_by_text(&checker, parser.get_arena(), "Dataset") {
        if let Some(dataset_instance) = checker.class_instance_type_from_symbol(dataset_sym) {
            println!(
                "class_instance_type_from_symbol(Dataset) => {}",
                dataset_instance.0
            );
            if let TypeData::ObjectWithIndex(shape_id) =
                checker.ctx.types.lookup(dataset_instance).unwrap()
            {
                let shape = checker.ctx.types.object_shape(shape_id);
                println!(
                    "Dataset shape: symbol={:?} props={} string_index={:?} number_index={:?}",
                    shape.symbol,
                    shape.properties.len(),
                    shape.string_index,
                    shape.number_index
                );
            }
        }
    }

    if let Some(TypeData::Union(members_id)) = checker
        .ctx
        .types
        .lookup(checker.get_type_of_node(NodeIndex(2532)))
    {
        println!("Members of node 2532 (|| expression) union:");
        for member in checker.ctx.types.type_list(members_id).iter() {
            match checker.ctx.types.lookup(*member) {
                Some(TypeData::ObjectWithIndex(shape_id)) => {
                    let shape = checker.ctx.types.object_shape(shape_id);
                    println!(
                        "  member {} => symbol {:?} props {}",
                        member.0,
                        shape.symbol,
                        shape.properties.len()
                    );
                }
                Some(other) => {
                    println!("  member {} => {:?}", member.0, other);
                }
                None => println!("  member {} => <missing>", member.0),
            }
        }
    }

    println!("symbol_instance_types cached:");
    for (sym, ty) in checker.ctx.symbol_instance_types.iter() {
        let symbol_name = checker
            .ctx
            .binder
            .get_symbol(*sym)
            .map(|s| s.escaped_name.clone())
            .unwrap_or_else(|| "<unknown>".to_string());
        println!("  {:?} ({}) => {}", sym.0, symbol_name, ty.0);
    }

    println!(
        "class_instance_type_cache size={}",
        checker.ctx.class_instance_type_cache.len()
    );
    for (class_idx, ty) in checker.ctx.class_instance_type_cache.iter() {
        match checker.ctx.types.lookup(*ty) {
            Some(tsz_solver::TypeData::ObjectWithIndex(shape_id)) => {
                let shape = checker.ctx.types.object_shape(shape_id);
                println!(
                    "  node {} => {} (ObjectWithIndex symbol={:?} props={})",
                    class_idx.0,
                    ty.0,
                    shape.symbol,
                    shape.properties.len()
                );
            }
            Some(other) => {
                println!("  node {} => {} ({:?})", class_idx.0, ty.0, other);
            }
            None => {
                println!("  node {} => {} (missing type)", class_idx.0, ty.0);
            }
        }
    }

    let mut has_ts2322 = false;
    for d in checker.ctx.diagnostics.iter() {
        if d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE {
            has_ts2322 = true;
            println!(
                "diag: pos={} len={} msg={}",
                d.start, d.length, d.message_text
            );
        }
    }
    assert!(
        !has_ts2322,
        "Unexpected TS2322 in parser recovery/type-id repro for {file_name}"
    );
}

fn find_symbol_id_by_text(
    checker: &CheckerState<'_>,
    arena: &tsz_parser::NodeArena,
    name: &str,
) -> Option<SymbolId> {
    for node_idx in 0..arena.nodes.len() {
        let idx = NodeIndex(node_idx as u32);
        let Some(text) = arena.get_identifier_text(idx) else {
            continue;
        };
        if text != name {
            continue;
        }

        if let Some(symbol) = checker.resolve_identifier_symbol(idx) {
            return Some(symbol);
        }
    }

    None
}
