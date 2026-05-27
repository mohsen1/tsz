use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn checked_js_diagnostics(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();

    tsz_checker::test_utils::check_source(source, "test.js", options)
        .into_iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn strict_diagnostics_with_libs(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
    .apply_strict_defaults();
    let lib_files = tsz_checker::test_utils::load_lib_files(&["es5.d.ts"]);

    tsz_checker::test_utils::check_source_with_libs(source, "test.ts", options, &lib_files)
        .into_iter()
        .filter(|d| d.code != 2318 && d.code != 6133)
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn ts18048_messages(diagnostics: &[(u32, String)]) -> Vec<&str> {
    diagnostics
        .iter()
        .filter(|(code, _)| *code == 18048)
        .map(|(_, message)| message.as_str())
        .collect()
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of control_flow_type_guard_tests tests.
include!("control_flow_type_guard_tests_parts/part_00.rs");
include!("control_flow_type_guard_tests_parts/part_01.rs");
