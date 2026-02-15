use crate::diagnostics::diagnostic_codes;
use crate::{CheckerOptions, CheckerState};
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

    eprintln!("Skipping repro_parserreal test; fixture not found: {rel_path}");
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
        println!(
            "Note: expected token '{}' not found directly under target cover",
            expected_token
        );
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
