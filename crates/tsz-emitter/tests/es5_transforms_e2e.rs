//! End-to-end ES5 transform tests using the full `lower_and_print` pipeline.
//!
//! These tests verify that the complete chain (parse -> lower -> print) produces
//! correct ES5 output for destructuring, class, and async transforms.

use crate::context::emit::EmitContext;
use crate::emitter::ModuleKind;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use crate::output::printer::{PrintOptions, lower_and_print};
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;

fn emit_with_target(source: &str, target: ScriptTarget) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut opts = PrintOptions {
        target,
        ..PrintOptions::default()
    };
    opts.remove_comments = true;
    lower_and_print(&parser.arena, root, opts).code
}

fn emit_es5(source: &str) -> String {
    emit_with_target(source, ScriptTarget::ES5)
}

fn emit_es5_with_module(source: &str, module: ModuleKind) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut opts = PrintOptions {
        target: ScriptTarget::ES5,
        module,
        ..PrintOptions::default()
    };
    opts.remove_comments = true;
    lower_and_print(&parser.arena, root, opts).code
}

include!("es5_transforms_e2e_parts/part_00.rs");
include!("es5_transforms_e2e_parts/part_01.rs");
