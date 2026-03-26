//! Tests for TS2303: Circular definition of import alias.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str, file_name: &str) -> Vec<(u32, String)> {
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
        CheckerOptions {
            module: ModuleKind::CommonJS,
            isolated_modules: true,
            ..Default::default()
        },
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn export_as_namespace_is_not_circular_alias() {
    // `export as namespace X` creates an ALIAS-flagged symbol in the binder with
    // is_umd_export = true. This is an outbound UMD export, NOT an import alias.
    // The circular alias checker must skip these symbols.
    let source = r#"
export = React;
export as namespace React;

declare namespace React {
  type ReactNode = string | number | boolean | null | undefined;
  function createElement(): void;
}
"#;

    let diagnostics = get_diagnostics(source, "react-index.d.ts");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2303),
        "Should not emit TS2303 for `export as namespace`. Got: {diagnostics:?}"
    );
}

#[test]
fn ambient_require_alias_reexport_is_not_a_circular_alias() {
    let source = r#"
declare module "events" {
  interface EventEmitterOptions {
    captureRejections?: boolean;
  }
  class EventEmitter {
    constructor(options?: EventEmitterOptions);
  }
  export = EventEmitter;
}
declare module "node:events" {
  import events = require("events");
  export = events;
}
"#;

    let diagnostics = get_diagnostics(source, "events.d.ts");
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2303),
        "Did not expect TS2303 for ambient import alias re-export, got: {diagnostics:?}"
    );
}

#[test]
fn export_equals_global_augmentation_namespace_cycle_reports_ts2303_not_ts2686() {
    let source = r#"
declare global { namespace N {} }
export = N;
export as namespace N;
"#;

    let diagnostics = get_diagnostics(source, "a.d.ts");
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2303),
        "Expected TS2303 for export= cycle through global augmentation namespace. Got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2686),
        "Did not expect TS2686 for the export= cycle case. Got: {diagnostics:?}"
    );
}
