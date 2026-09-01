// Tests for AMD outFile bundled output:
// 1. Relative specifiers resolved to AMD module IDs in define() dep arrays
// 2. Factory parameter names globally unique across the bundle (m1_1, m1_2, …)

use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use rustc_hash::FxHashMap;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_amd_bundled(source: &str, module_name: &str, counters: FxHashMap<String, u32>) -> String {
    let mut parser = ParserState::new(format!("{module_name}.ts"), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        module: ModuleKind::AMD,
        target: ScriptTarget::ES5,
        bundled_module_name: Some(module_name.to_string()),
        bundle_module_counters: counters,
        ..Default::default()
    };
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

// ── Fix 1: relative specifiers must resolve to AMD module IDs ────────────────

#[test]
fn amd_bundle_relative_dep_resolves_to_module_id() {
    // Rule: in AMD bundled output, `"./m1"` relative to module `"m2"` must
    // appear as `"m1"` in the define() dep array, not `"./m1"`.
    let source = r#"import { Cls } from "./m1";
Cls.prototype.foo = function() { return 1; };
"#;
    let output = emit_amd_bundled(source, "m2", FxHashMap::default());
    assert!(
        output.contains("\"m1\""),
        "resolved AMD module ID 'm1' must appear in dep array.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\"./m1\""),
        "raw relative specifier './m1' must not appear in bundled define() dep array.\nOutput:\n{output}"
    );
}

#[test]
fn amd_bundle_nested_relative_dep_resolves_correctly() {
    // Rule: relative resolution works with non-trivial paths, e.g.
    // `"../shared/util"` from module `"a/b"` resolves to `"shared/util"`.
    let source = r#"import { helper } from "../shared/util";
helper();
"#;
    let output = emit_amd_bundled(source, "a/b", FxHashMap::default());
    assert!(
        output.contains("\"shared/util\""),
        "path traversal '../shared/util' from 'a/b' must resolve to 'shared/util'.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\"../"),
        "resolved dep must not start with '../'.\nOutput:\n{output}"
    );
}

#[test]
fn amd_bundle_type_only_relative_dep_not_in_define_array() {
    // Rule: type-only imports must still be omitted from the define() dep array
    // even when resolved — the resolution must not accidentally re-admit them.
    let source = r#"import { MyType } from "./types";
var x: MyType;
"#;
    let output = emit_amd_bundled(source, "consumer", FxHashMap::default());
    assert!(
        !output.contains("\"types\""),
        "type-only import must not appear in AMD dep array after resolution.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("\"./types\""),
        "type-only import (raw) must not appear in AMD dep array.\nOutput:\n{output}"
    );
}

// ── Fix 2: factory parameter names globally unique across bundle ─────────────

#[test]
fn amd_bundle_counter_unique_across_files() {
    // Rule: when the same base module is imported in two different files of the
    // same bundle, the factory parameters must be numbered sequentially
    // (m1_1 in the first file, m1_2 in the second), not both m1_1.
    let src1 = r#"import { Cls } from "./m1";
Cls.prototype.foo = function() { return 1; };
"#;
    let output1 = emit_amd_bundled(src1, "m2", FxHashMap::default());
    assert!(
        output1.contains("m1_1"),
        "first file should use counter 1 for m1 dep.\nOutput:\n{output1}"
    );

    // Second file seeds its counter from the first file's result.
    let src2 = r#"import { Cls } from "./m1";
Cls.prototype.bar = function() { return 2; };
"#;
    let mut seed: FxHashMap<String, u32> = FxHashMap::default();
    seed.insert("m1".to_string(), 1);
    let output2 = emit_amd_bundled(src2, "m3", seed);
    assert!(
        output2.contains("m1_2"),
        "second file should use counter 2 for m1 dep, not m1_1.\nOutput:\n{output2}"
    );
    assert!(
        !output2.contains("m1_1"),
        "second file must not reuse the counter from the first file.\nOutput:\n{output2}"
    );
}

#[test]
fn amd_bundle_counter_increments_for_named_imports_across_files() {
    // Rule: named imports (non-namespace) go through the counter so the factory
    // parameter is unique across the bundle even when both files import the
    // same module.
    let src1 = r#"import { doA } from "./util";
doA();
"#;
    let output1 = emit_amd_bundled(src1, "a", FxHashMap::default());
    assert!(
        output1.contains("util_1"),
        "first file's named import from util should produce 'util_1'.\nOutput:\n{output1}"
    );

    // Seed the second file from first file's counter state.
    let src2 = r#"import { doB } from "./util";
doB();
"#;
    let mut seed: FxHashMap<String, u32> = FxHashMap::default();
    seed.insert("util".to_string(), 1);
    let output2 = emit_amd_bundled(src2, "b", seed);
    assert!(
        output2.contains("util_2"),
        "second file's named import from util should produce 'util_2'.\nOutput:\n{output2}"
    );
    assert!(
        !output2.contains("util_1"),
        "second file must not reuse 'util_1' from the first file.\nOutput:\n{output2}"
    );
}
