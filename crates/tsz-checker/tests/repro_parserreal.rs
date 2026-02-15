use crate::diagnostics::diagnostic_codes;
use crate::{CheckerOptions, CheckerState};
use std::io::Write;
use std::path::PathBuf;
use tsz_binder::BinderState;
use tsz_parser::parser::{NodeIndex, ParserState, node::NodeAccess};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeInterner;

#[cfg(test)]
#[test]
fn repro_parser_real_14_type_ids() {
    let Some(source) = load_test_source(
        "TypeScript/tests/cases/conformance/parser/ecmascript5/parserRealSource14.ts",
    ) else {
        return;
    };
    run_and_print_source_line(&source, "parserRealSource14.ts", 36, 20, "clone");
}

#[cfg(test)]
#[test]
fn repro_parser_harness_type_ids() {
    let Some(source) = load_test_source(
        "TypeScript/tests/cases/conformance/parser/ecmascript5/RealWorld/parserharness.ts",
    ) else {
        return;
    };
    run_and_print_source_line(&source, "parserharness.ts", 611, 64, "Dataset");
}

#[cfg(test)]
fn load_test_source(rel_path: &str) -> Option<String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../").join(rel_path),
        manifest_dir.join("../../../").join(rel_path),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return std::fs::read_to_string(candidate).ok();
        }
    }

    let _ = writeln!(
        std::io::stderr(),
        "Skipping repro_parserreal test; fixture not found: {rel_path}"
    );
    None
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

        if kind == SyntaxKind::Identifier as u16 {
            println!(
                "cover id node={} kind={} pos={}..{} text={} -> {}",
                idx, kind, node.pos, node.end, text, node_type.0
            );
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

    if !found {
        println!("Note: expected token '{expected_token}' not found directly under target cover");
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

#[cfg(test)]
#[test]
fn repro_async_generator_computed_yield_method() {
    let source = r"
interface yield {}
class C21 {
    async * [yield]() {
    }
}
";
    let mut parser = ParserState::new(
        "yieldInClassComputedPropertyIsError.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "yieldInClassComputedPropertyIsError.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    for d in checker.ctx.diagnostics.iter() {
        if d.code == diagnostic_codes::IDENTIFIER_EXPECTED
            || d.code == diagnostic_codes::EXPECTED
            || d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
        {
            println!("diag: code={} msg={}", d.code, d.message_text);
        }
    }

    let count_2693 = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
        })
        .count();

    let mut found_expression = None;
    for (idx, node) in parser.get_arena().nodes.iter().enumerate() {
        if node.kind == SyntaxKind::Identifier as u16
            && let Some(text) = parser
                .get_arena()
                .get_identifier_text(NodeIndex(idx as u32))
            && text == "yield"
        {
            let t = checker.get_type_of_node(NodeIndex(idx as u32));
            println!(
                "identifier node={} pos={}..{} type={} text={}",
                idx, node.pos, node.end, node.kind, t.0
            );
            if let Some(symbol) = checker.resolve_identifier_symbol(NodeIndex(idx as u32)) {
                let flags = checker.ctx.binder.get_symbol(symbol).unwrap().flags;
                println!("  symbol={} flags={}", symbol.0, flags);
            }

            if let Some(en) = parser
                .get_arena()
                .get(NodeIndex(idx as u32))
                .and_then(|_| parser.get_arena().get_extended(NodeIndex(idx as u32)))
            {
                println!(
                    "  parent={} kind={}",
                    en.parent.0,
                    parser
                        .get_arena()
                        .get(en.parent)
                        .map(|n| n.kind)
                        .unwrap_or(0)
                );
            }

            found_expression = Some(NodeIndex(idx as u32));
        }
    }

    println!("found expression={found_expression:?}");
    assert!(
        count_2693 > 0,
        "expected TS2693 for async generator computed yield member"
    );
}

#[cfg(test)]
#[test]
fn repro_async_generator_computed_yield_method_with_parse_errors() {
    let source = r"
interface yield {}
class C21 {
    async * [yield]() {
    }
}";
    let mut parser = ParserState::new(
        "yieldInClassComputedPropertyIsError.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "yieldInClassComputedPropertyIsError.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.ctx.has_parse_errors = true;
    checker.ctx.has_syntax_parse_errors = true;
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let count_2693 = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
        })
        .count();
    assert!(
        count_2693 > 0,
        "expected TS2693 despite parse-error suppression"
    );
}

#[cfg(test)]
#[test]
fn repro_async_generator_class_methods_ast_shape() {
    let source = include_str!(
        "/Users/mohsenazimi/code/tsz-5/TypeScript/tests/cases/conformance/parser/ecmascript2018/asyncGenerators/parser.asyncGenerators.classMethods.es2018.ts"
    );

    let mut parser = ParserState::new(
        "parser.asyncGenerators.classMethods.es2018.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "parser.asyncGenerators.classMethods.es2018.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    println!("yield-ish nodes:");
    let mut line_starts = Vec::with_capacity(64);
    line_starts.push(0usize);
    for (offset, ch) in source.char_indices() {
        if ch == '\n' {
            line_starts.push(offset + 1);
        }
    }
    let lookup_line = |start_pos: u32| -> (usize, usize) {
        let pos = start_pos as usize;
        let line_idx = match line_starts.binary_search(&pos) {
            Ok(found) => found,
            Err(ins) => ins.saturating_sub(1),
        };
        (line_idx + 1, pos.saturating_sub(line_starts[line_idx]) + 1)
    };

    for (idx, node) in parser.get_arena().nodes.iter().enumerate() {
        let idx = NodeIndex(idx as u32);
        if let Some(ext) = parser.get_arena().get_extended(idx) {
            let parent_kind = parser
                .get_arena()
                .get(ext.parent)
                .map(|n| n.kind)
                .unwrap_or(0);
            if node.kind == SyntaxKind::Identifier as u16
                && let Some(text) = parser.get_arena().get_identifier_text(idx)
                && text == "yield"
            {
                let pos = parser
                    .get_arena()
                    .get(idx)
                    .map(|n| (n.pos, n.end))
                    .unwrap_or((0, 0));
                let (line, column) = lookup_line(pos.0);
                let mut has_parent_computed = false;
                let mut has_method_ancestor = false;
                let mut parent_chain = Vec::new();
                let mut cursor = ext.parent;
                let mut depth = 0;
                while cursor.0 != 0 && depth < 12 {
                    if let Some(parent_node) = parser.get_arena().get(cursor) {
                        parent_chain.push(parent_node.kind);
                        if parent_node.kind == tsz_parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                            has_parent_computed = true;
                        }
                        if parent_node.kind == tsz_parser::syntax_kind_ext::METHOD_DECLARATION {
                            has_method_ancestor = true;
                        }

                        if let Some(parent_ext) = parser.get_arena().get_extended(cursor) {
                            cursor = parent_ext.parent;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                    depth += 1;
                }
                let value_type = checker.get_type_of_node(idx).0;
                println!(
                    "id={:?} pos={:?}..{:?} line={}:{} parent={:?} kind={:?} computed_parent={has_parent_computed} method_ancestor={has_method_ancestor} type={}",
                    idx, pos.0, pos.1, line, column, ext.parent, parent_kind, value_type
                );
                println!("  ancestor kinds: {parent_chain:?}");
                if let Some(symbol) = checker.resolve_identifier_symbol(idx) {
                    println!("  symbol={}", symbol.0);
                }
                if checker.get_source_location(idx).is_some() {
                    println!(
                        "  diag_flags: has_parse_errors={} has_syntax_parse_errors={}",
                        checker.has_parse_errors(),
                        checker.has_syntax_parse_errors()
                    );
                }
            } else if node.kind == tsz_parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                let pos = parser
                    .get_arena()
                    .get(idx)
                    .map(|n| (n.pos, n.end))
                    .unwrap_or((0, 0));
                println!(
                    "computed node={:?} pos={:?}..{:?} parent={:?} grandparent={:?}",
                    idx,
                    pos.0,
                    pos.1,
                    ext.parent,
                    parser
                        .get_arena()
                        .get_extended(ext.parent)
                        .map(|p_ext| p_ext.parent.0)
                );
            }
        }
    }

    for d in checker.ctx.diagnostics.iter() {
        if d.code == diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE
            || d.code == diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO
            || d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
        {
            println!("diag line: code={} msg={}", d.code, d.message_text);
        }
    }
}

#[cfg(test)]
#[test]
fn repro_async_generator_class_methods_cross_file() {
    let interface_source = "interface yield {}\n";
    let class_source = "class C21 {\n    async * [yield]() {\n    }\n}\n";

    let mut iface_parser = ParserState::new(
        "yieldAsTypeIsStrictError.ts".to_string(),
        interface_source.to_string(),
    );
    let iface_root = iface_parser.parse_source_file();

    let mut class_parser = ParserState::new(
        "yieldInClassComputedPropertyIsError.ts".to_string(),
        class_source.to_string(),
    );
    let class_root = class_parser.parse_source_file();

    let iface_arena = std::sync::Arc::new(iface_parser.into_arena());
    let class_arena = std::sync::Arc::new(class_parser.into_arena());

    let mut iface_binder = BinderState::new();
    iface_binder.bind_source_file(&iface_arena, iface_root);
    let mut class_binder = BinderState::new();
    class_binder.bind_source_file(&class_arena, class_root);

    let iface_binder = std::sync::Arc::new(iface_binder);
    let class_binder = std::sync::Arc::new(class_binder);
    let all_arenas = std::sync::Arc::new(vec![iface_arena.clone(), class_arena.clone()]);
    let all_binders = std::sync::Arc::new(vec![iface_binder, class_binder.clone()]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &class_arena,
        &class_binder,
        &types,
        "yieldInClassComputedPropertyIsError.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.report_unresolved_imports = true;
    checker.ctx.has_parse_errors = true;
    checker.ctx.has_syntax_parse_errors = true;

    checker.check_source_file(class_root);

    let mut has_2693 = false;
    let mut found_in_class = false;

    for (idx, node) in class_arena.nodes.iter().enumerate() {
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && class_arena
                .get_identifier_text(tsz_parser::NodeIndex(idx as u32))
                .is_some_and(|t| t == "yield")
        {
            let idx = tsz_parser::NodeIndex(idx as u32);
            found_in_class = true;
            if let Some(sym) = checker.resolve_identifier_symbol(idx) {
                let sym_name = checker
                    .ctx
                    .binder
                    .get_symbol(sym)
                    .map(|s| s.escaped_name.clone())
                    .unwrap_or_else(|| "<none>".to_string());
                let sym_flags = checker
                    .ctx
                    .binder
                    .get_symbol(sym)
                    .map(|s| s.flags)
                    .unwrap_or(0);
                let _ = writeln!(
                    std::io::stderr(),
                    "yield symbol={sym_name} flags={sym_flags}"
                );
            }
        }
    }

    for d in checker.ctx.diagnostics.iter() {
        let _ = writeln!(
            std::io::stderr(),
            "diag code={} start={} len={} msg={}",
            d.code,
            d.start,
            d.length,
            d.message_text
        );
        if d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE {
            has_2693 = true;
        }
    }

    let _ = writeln!(
        std::io::stderr(),
        "found_in_class={found_in_class} has_2693={has_2693}"
    );
    assert!(found_in_class);
    assert!(has_2693);
}

#[cfg(test)]
#[test]
fn repro_async_generator_class_methods_ast_shape_parse_errors() {
    let source = include_str!(
        "/Users/mohsenazimi/code/tsz-5/TypeScript/tests/cases/conformance/parser/ecmascript2018/asyncGenerators/parser.asyncGenerators.classMethods.es2018.ts"
    );

    let mut parser = ParserState::new(
        "parser.asyncGenerators.classMethods.es2018.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "parser.asyncGenerators.classMethods.es2018.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.has_parse_errors = true;
    checker.ctx.has_syntax_parse_errors = true;
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    println!("parse-error mode diagnostics:");
    let target = (129, 14);
    for d in checker.ctx.diagnostics.iter() {
        if d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE {
            let start = d.start;
            let line = source[..start as usize].matches('\n').count() + 1;
            let line_start = source[..start as usize]
                .rfind('\n')
                .map_or(0, |idx| idx + 1);
            let column = start as usize - line_start + 1;
            if (line, column) == target {
                println!("target diag at line129:14 => {:?}", d.message_text);
            }
            println!(
                "diag code={} start={} line={} col={} msg={}",
                d.code, d.start, line, column, d.message_text
            );
        }
    }

    let count_2693 = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
        })
        .count();
    assert!(count_2693 > 0);
}

#[cfg(test)]
#[test]
fn repro_async_generator_class_methods_forced_parse_errors() {
    let source = include_str!(
        "/Users/mohsenazimi/code/tsz-5/TypeScript/tests/cases/conformance/parser/ecmascript2018/asyncGenerators/parser.asyncGenerators.classMethods.es2018.ts"
    );
    let mut parser = ParserState::new(
        "parser.asyncGenerators.classMethods.es2018.ts".to_string(),
        source.to_string(),
    );
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "parser.asyncGenerators.classMethods.es2018.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.has_parse_errors = true;
    checker.ctx.has_syntax_parse_errors = true;
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let count_2693 = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
        })
        .count();
    println!("count_2693_forced_parse_errors={count_2693}");
    for d in checker.ctx.diagnostics.iter() {
        if d.code == diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE {
            println!("diag line: {}", d.message_text);
        }
    }
    assert!(count_2693 > 0);
}
