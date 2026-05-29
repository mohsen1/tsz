//! Unit tests for `check_utils` (program / `BoundFile` construction and the
//! source-resolution phase). Split out of `check_utils.rs` to keep the
//! production module under the 2000-line limit (§19; ratchet tracked by #9412).

use super::*;

/// Parse source text and return the `BoundFile` from a merged program.
fn bound_file(source: &str) -> BoundFile {
    let bind_result = parallel::parse_and_bind_single("test.ts".to_string(), source.to_string());
    let program = parallel::merge_bind_results(vec![bind_result]);
    program.files.into_iter().next().unwrap()
}

/// Extract helper names from `required_helpers` (at ES5 target by default).
fn helper_names(source: &str) -> Vec<&'static str> {
    helper_names_at(source, tsz_common::ScriptTarget::ES5)
}

fn helper_names_at(source: &str, target: tsz_common::ScriptTarget) -> Vec<&'static str> {
    let file = bound_file(source);
    required_helpers(&file, target, false, false, false)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect()
}

fn merged_program(files: &[(&str, &str)]) -> MergedProgram {
    let bind_results = files
        .iter()
        .map(|(name, source)| {
            parallel::parse_and_bind_single((*name).to_string(), (*source).to_string())
        })
        .collect();
    parallel::merge_bind_results(bind_results)
}

fn check_merged_program_file(files: &[(&str, &str)], entry_file: &str) -> Vec<Diagnostic> {
    let program = merged_program(files);
    let entry_idx = program
        .files
        .iter()
        .position(|file| file.file_name == entry_file)
        .expect("entry file should exist");
    let augmentations = MergedAugmentations::from_program(&program);
    let all_arenas = Arc::new(
        program
            .files
            .iter()
            .map(|file| Arc::clone(&file.arena))
            .collect::<Vec<_>>(),
    );
    let all_binders = Arc::new(
        program
            .files
            .iter()
            .enumerate()
            .map(|(file_idx, file)| {
                Arc::new(create_binder_from_bound_file_with_augmentations(
                    file,
                    &program,
                    file_idx,
                    &augmentations,
                ))
            })
            .collect::<Vec<_>>(),
    );
    let file_names = program
        .files
        .iter()
        .map(|file| file.file_name.clone())
        .collect::<Vec<_>>();
    let (resolved_module_paths, resolved_modules) =
        tsz::checker::module_resolution::build_module_resolution_maps(&file_names);
    let opts = tsz_common::checker_options::CheckerOptions {
        jsx_mode: tsz_common::checker_options::JsxMode::React,
        no_unused_locals: true,
        no_lib: true,
        module: ModuleKind::CommonJS,
        ..Default::default()
    };
    let interner = tsz_solver::construction::TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &interner,
        file_names[entry_idx].clone(),
        opts,
    );
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(program.files[entry_idx].source_file);
    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code != 2318)
        .cloned()
        .collect()
}

#[test]
fn plain_class_needs_no_helpers() {
    assert!(helper_names("class C { method() {} }").is_empty());
}

#[test]
fn jsx_fragment_factory_scope_ignores_external_module_globals() {
    let diagnostics = check_merged_program_file(
        &[
            (
                "/renderer.d.ts",
                r#"
declare global {
    namespace JSX {
        interface IntrinsicElements { [e: string]: any; }
        interface Element {}
    }
}
export function h(): void;
export function Fragment(): void;
"#,
            ),
            (
                "/entry.tsx",
                r#"/** @jsx h
 * @jsxFrag Fragment
 */
import { Fragment } from "./renderer";
const _frag = <></>;
"#,
            ),
        ],
        "/entry.tsx",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 2874 && diag.message_text.contains("'h'")),
        "Expected TS2874 for missing JSX factory `h`, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == 2879 && diag.message_text.contains("Fragment")),
        "Expected imported fragment factory to remain in scope, got: {diagnostics:#?}"
    );
}

/// TS1101 ('with' statements not allowed in strict mode) is a grammar
/// check, not a structural parse failure. The parser produces a valid AST
/// for the with-statement; tsc still emits semantic errors like TS2410
/// alongside it. Including TS1101 in `is_real_syntax_error` would cause
/// the CLI's `program_has_real_syntax_errors` filter to drop every
/// semantic diagnostic in any module file containing a `with` statement.
#[test]
fn ts1101_is_not_treated_as_real_syntax_error() {
    assert!(
        !is_real_syntax_error(1101),
        "TS1101 must NOT be classified as a real syntax error \
             — it is a strict-mode grammar check that does not malform the AST"
    );
}

/// Sanity-check that genuinely structural parse failures remain classified
/// as real syntax errors so the regression of TS1101's removal does not
/// accidentally weaken the broader filter.
#[test]
fn structural_parse_failures_remain_real_syntax_errors() {
    for code in [1005u32, 1109, 1128] {
        assert!(
            is_real_syntax_error(code),
            "TS{code} should still be classified as a real syntax error"
        );
    }
}

#[test]
fn ts_directive_scan_ignores_jsdoc_example_mentions() {
    let source = r#"/**
Example:
```
// @ts-expect-error
foo.bar;
```
*/
const value = 1;
"#;
    assert!(
        find_ts_directives(source).is_empty(),
        "directives embedded in documentation examples must not target source lines"
    );
}

#[test]
fn ts_directive_scan_keeps_real_line_directives() {
    let directives = find_ts_directives("// @ts-expect-error: intentional\nconst x: string = 1;");
    assert_eq!(directives.len(), 1);
    assert!(directives[0].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 1);
}

#[test]
fn ts_directive_scan_accepts_form_feed_before_directive() {
    let directives = find_ts_directives("//\x0C@ts-ignore\nconst x: string = 1;");
    assert_eq!(directives.len(), 1);
    assert!(!directives[0].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 1);
}

#[test]
fn ts_directive_scan_accepts_vertical_tab_before_directive() {
    let directives = find_ts_directives("//\x0B@ts-ignore\nconst x: string = 1;");
    assert_eq!(directives.len(), 1);
    assert!(!directives[0].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 1);
}

#[test]
fn ts_directive_suppresses_next_line_with_form_feed_spacing() {
    let source = "//\x0C@ts-ignore\nlet x: string = 1;\n";
    let mut diagnostics = vec![Diagnostic::error(
        "repro.ts".to_string(),
        21,
        1,
        "Type 'number' is not assignable to type 'string'.".to_string(),
        2322,
    )];

    apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);

    assert!(
        diagnostics.is_empty(),
        "Expected form-feed @ts-ignore to suppress the next-line diagnostic, got: {diagnostics:?}"
    );
}

#[test]
fn ts_directive_scan_keeps_template_substitution_directives() {
    let directives =
        find_ts_directives("const value = `${/* @ts-ignore */ 0}`;\nconst x: string = 1;");
    assert_eq!(directives.len(), 1);
    assert!(!directives[0].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 1);

    let directives =
        find_ts_directives("const value = `${/* @ts-expect-error */ 0}`;\nconst x: string = 1;");
    assert_eq!(directives.len(), 1);
    assert!(directives[0].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 1);
}

#[test]
fn ts_directive_suppresses_next_line_from_template_substitution() {
    let source = "const value = `${/* @ts-ignore */ 0}`;\nconst x: string = 1;\n";
    let mut diagnostics = vec![Diagnostic::error(
        "repro.ts".to_string(),
        45,
        1,
        "Type 'number' is not assignable to type 'string'.".to_string(),
        2322,
    )];

    apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);

    assert!(
            diagnostics.is_empty(),
            "Expected template-substitution @ts-ignore to suppress the next-line diagnostic, got: {diagnostics:?}"
        );
}

#[test]
fn ts_directive_scan_ignores_plain_template_text() {
    assert!(
        find_ts_directives("const value = `// @ts-ignore`;\nconst x: string = 1;").is_empty(),
        "directives in template text must not target source lines"
    );
}

#[test]
fn ts_directive_line_starts_treat_cr_as_line_break() {
    assert_eq!(
        build_line_starts("// @ts-ignore\rlet x: string = 1;\r"),
        vec![0, 14, 33],
    );
    assert_eq!(
        build_line_starts("// @ts-ignore\r\nlet x: string = 1;\n"),
        vec![0, 15, 34],
    );
}

#[test]
fn ts_ignore_suppresses_jsdoc_at_type_ts2304_in_declaration_emit() {
    // Issue #3996: a `// @ts-ignore` followed by a JSDoc `@type` annotation
    // referencing a missing name was incorrectly preserved during checked-JS
    // declaration emit because of a `line_text.contains("@type {")`
    // carve-out. tsc 6.0.3 suppresses the diagnostic regardless of which
    // checking surface (source-file vs declaration-emit) raised it.
    let source = "// @ts-ignore\n/** @type {Missing} */\nexport const x = 1;\n";
    let mut diagnostics = vec![Diagnostic::error(
        "repro.js".to_string(),
        22,
        7,
        "Cannot find name 'Missing'.".to_string(),
        2304,
    )];
    apply_ts_directive_suppression("repro.js", source, &mut diagnostics, true);
    assert!(
            diagnostics.is_empty(),
            "Expected @ts-ignore to suppress JSDoc @type TS2304 even during declaration emit, got: {diagnostics:?}"
        );
}

#[test]
fn ts_ignore_suppresses_next_line_with_cr_only_line_endings() {
    let source = "// @ts-ignore\rlet x: string = 1;\r";
    let mut diagnostics = vec![Diagnostic::error(
        "repro.ts".to_string(),
        18,
        1,
        "Type 'number' is not assignable to type 'string'.".to_string(),
        2322,
    )];

    apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);

    assert!(
        diagnostics.is_empty(),
        "Expected CR-only @ts-ignore to suppress the next-line diagnostic, got: {diagnostics:?}"
    );
}

#[test]
fn ts_expect_error_uses_next_line_with_cr_only_line_endings() {
    let source = "// @ts-expect-error\rlet x: string = 1;\r";
    let mut diagnostics = vec![Diagnostic::error(
        "repro.ts".to_string(),
        24,
        1,
        "Type 'number' is not assignable to type 'string'.".to_string(),
        2322,
    )];

    apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);

    assert!(
            diagnostics.is_empty(),
            "Expected CR-only @ts-expect-error to suppress the next-line diagnostic, got: {diagnostics:?}"
        );
}

/// Anchor regression for TS2578.
///
/// tsc 6.0.3 emits TS2578 at the comment range — the `/` of the `//`
/// or `/*` opener — not at the enclosing line start. For an indented
/// `  // @ts-expect-error` that means the diagnostic span starts at
/// the comment's first character (here byte 2, column 3), not at
/// column 1.
///
/// Source: type-challenges 00004-easy-pick (issue #4902).
#[test]
fn unused_expect_error_anchors_at_indented_comment_start() {
    let source = "const a = 1;\n  // @ts-expect-error\nconst x = 1;\n";
    let mut diagnostics = Vec::new();
    apply_ts_directive_suppression("anchor.ts", source, &mut diagnostics, false);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, 2578);
    // The `//` of the indented comment starts at byte offset
    // `"const a = 1;\n  ".len() == 15`. tsc anchors at this position
    // (column 3 on the comment line), not at the line start (column 1).
    assert_eq!(diagnostics[0].start, 15);
    // Span covers the entire comment, including the `//` opener.
    assert_eq!(diagnostics[0].length, "// @ts-expect-error".len() as u32);
}

/// Same rule for block comments: anchor at `/*`, span the whole comment.
/// Anti-hardcoding cover: a different comment opener and a different
/// indent — the fix must key on the structural comment range, not on
/// `//` specifically or on a fixed offset.
#[test]
fn unused_expect_error_anchors_at_indented_block_comment_start() {
    let source = "    /* @ts-expect-error */\nconst y = 1;\n";
    let mut diagnostics = Vec::new();
    apply_ts_directive_suppression("anchor-block.ts", source, &mut diagnostics, false);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, 2578);
    // 4 spaces of indent, then `/*` starts at byte 4 (column 5).
    assert_eq!(diagnostics[0].start, 4);
    assert_eq!(diagnostics[0].length, "/* @ts-expect-error */".len() as u32);
}

#[test]
fn unused_expect_error_in_multiline_block_anchors_at_directive_line_start() {
    let source = "    /*\n   @ts-expect-error */\nconst y = 1;\n";
    let mut diagnostics = Vec::new();
    apply_ts_directive_suppression("anchor-multiline-block.ts", source, &mut diagnostics, false);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, 2578);
    assert_eq!(diagnostics[0].start, "    /*\n".len() as u32);
    assert_eq!(diagnostics[0].length, "   @ts-expect-error */".len() as u32);
}

#[test]
fn raw_ts_nocheck_text_does_not_suppress_unused_expect_error() {
    let source = r#"const marker = "@ts-nocheck";

// @ts-expect-error
const stringValue = 1;

marker;
stringValue;
"#;
    let mut diagnostics = Vec::new();

    apply_ts_directive_suppression("string.ts", source, &mut diagnostics, false);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, 2578);
}

#[test]
fn late_ts_nocheck_comment_does_not_suppress_unused_expect_error() {
    let source = r#"const before = 0;

// @ts-nocheck
// @ts-expect-error
const lateValue = 1;

before;
lateValue;
"#;
    let mut diagnostics = Vec::new();

    apply_ts_directive_suppression("late-comment.ts", source, &mut diagnostics, false);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, 2578);
}

#[test]
fn leading_ts_nocheck_suppresses_unused_expect_error() {
    let source = r#"// @ts-nocheck

// @ts-expect-error
const unchecked = 1;

unchecked;
"#;
    let mut diagnostics = Vec::new();

    apply_ts_directive_suppression("actual-nocheck-control.ts", source, &mut diagnostics, false);

    assert!(diagnostics.is_empty());
}

#[test]
fn ts_directive_scan_keeps_triple_slash_directives() {
    let directives = find_ts_directives("/// @ts-ignore\nx();");
    assert_eq!(directives.len(), 1);
    assert!(!directives[0].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 1);
}

#[test]
fn build_line_starts_handles_cr_and_crlf() {
    // \n only
    assert_eq!(build_line_starts("a\nb\nc"), vec![0, 2, 4]);
    // \r only (classic Mac line endings)
    assert_eq!(build_line_starts("a\rb\rc"), vec![0, 2, 4]);
    // \r\n (Windows): one line break, not two
    assert_eq!(build_line_starts("a\r\nb\r\nc"), vec![0, 3, 6]);
    // Mixed
    assert_eq!(build_line_starts("a\nb\rc\r\nd"), vec![0, 2, 4, 7]);
    // U+2028 LINE SEPARATOR
    assert_eq!(build_line_starts("a\u{2028}b"), vec![0, 4]);
    // U+2029 PARAGRAPH SEPARATOR
    assert_eq!(build_line_starts("a\u{2029}b"), vec![0, 4]);
}

#[test]
fn ts_directive_scan_handles_cr_only_line_endings() {
    // CR-only file: directive on line 0 must suppress line 1, and the
    // single-line comment must not swallow the rest of the file.
    let directives = find_ts_directives("// @ts-ignore\rlet x: string = 1;\r");
    assert_eq!(directives.len(), 1);
    assert!(!directives[0].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 1);
    // Comment span must stop at the CR, not run to end-of-file.
    assert_eq!(
        directives[0].unused_diagnostic_length,
        "// @ts-ignore".len() as u32
    );
}

#[test]
fn ts_directive_scan_handles_crlf_line_endings() {
    let directives = find_ts_directives("// @ts-expect-error\r\nconst x: string = 1;\r\n");
    assert_eq!(directives.len(), 1);
    assert!(directives[0].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 1);
    // The CR must be excluded from the comment span (matches the
    // existing behaviour of the LF-only path).
    assert_eq!(
        directives[0].unused_diagnostic_length,
        "// @ts-expect-error".len() as u32
    );
}

#[test]
fn ts_directive_suppresses_diagnostic_with_cr_line_endings() {
    let source = "// @ts-ignore\rlet x: string = 1;\r";
    // The bad assignment is on line 1 (0-based) at the byte offset of
    // the literal `1` after the CR.
    let bad_offset = source.find('1').unwrap() as u32;
    let mut diagnostics = vec![Diagnostic::error(
        "repro.ts".to_string(),
        bad_offset,
        1,
        "Type 'number' is not assignable to type 'string'.".to_string(),
        2322,
    )];
    apply_ts_directive_suppression("repro.ts", source, &mut diagnostics, false);
    assert!(
        diagnostics.is_empty(),
        "@ts-ignore must suppress the next-line diagnostic with CR-only endings: {diagnostics:?}"
    );
}

#[test]
fn ts_directive_scan_keeps_block_comment_directives() {
    let directives = find_ts_directives(
        r#"/**
 @ts-expect-error */
texts.push(100);

{/*@ts-ignore*/}
<MyComponent foo={100} />"#,
    );
    assert_eq!(directives.len(), 2);
    assert!(directives[0].is_expect_error);
    assert!(!directives[1].is_expect_error);
    assert_eq!(directives[0].suppressed_line, 2);
    assert_eq!(directives[1].suppressed_line, 5);
}

#[test]
fn private_field_emits_class_private_field_set() {
    let helpers = helper_names("class C { #foo = 1; }");
    assert_eq!(helpers, vec!["__classPrivateFieldSet"]);
}

#[test]
fn default_reexport_requires_import_default_helper_when_interop_enabled() {
    let file = bound_file("export { default } from \"./a\";");
    let helpers: Vec<_> =
        required_helpers(&file, tsz_common::ScriptTarget::ES2017, true, false, false)
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
    assert_eq!(helpers, vec!["__importDefault"]);
}

#[test]
fn default_named_import_requires_import_default_helper_without_interop() {
    let file = bound_file("import { default as b } from \"./a\";\nvoid b;");
    let helpers: Vec<_> =
        required_helpers(&file, tsz_common::ScriptTarget::ES2017, false, false, false)
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
    assert_eq!(helpers, vec!["__importDefault"]);
}

#[test]
fn virtual_program_missing_tslib_reports_ts2354() {
    let program = merged_program(&[
        ("__virtual__/a.ts", "export default class { }"),
        ("__virtual__/b.ts", "export { default } from \"./a\";"),
    ]);
    let mut options = ResolvedCompilerOptions {
        import_helpers: true,
        es_module_interop: true,
        ..Default::default()
    };
    options.checker.target = tsz_common::ScriptTarget::ES2017;

    let diagnostics = detect_missing_tslib_helper_diagnostics(
        &program,
        &options,
        Path::new("/__virtual__"),
        &rustc_hash::FxHashMap::default(),
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2354
                && diag.file == "__virtual__/b.ts"
                && diag.message_text
                    == "This syntax requires an imported helper but module 'tslib' cannot be found."
        }),
        "Expected TS2354 for virtual program without tslib. Got: {diagnostics:#?}"
    );
}

#[test]
fn declaration_extension_variants_do_not_require_imported_tslib_helpers() {
    let program = merged_program(&[
        (
            "__virtual__/index.d.mts",
            "declare class Base {}\ndeclare class Derived extends Base {}",
        ),
        (
            "__virtual__/index.d.cts",
            "declare class CjsBase {}\ndeclare class CjsDerived extends CjsBase {}",
        ),
    ]);
    let mut options = ResolvedCompilerOptions {
        import_helpers: true,
        ..Default::default()
    };
    options.checker.target = tsz_common::ScriptTarget::ES5;

    let diagnostics = detect_missing_tslib_helper_diagnostics(
        &program,
        &options,
        Path::new("/__virtual__"),
        &rustc_hash::FxHashMap::default(),
    );

    assert!(
        diagnostics.iter().all(|diag| diag.code != 2354),
        "Did not expect TS2354 for declaration-file variants. Got: {diagnostics:#?}"
    );
}

#[test]
fn in_program_tslib_index_helpers_satisfy_legacy_decorator_requirements() {
    let program = merged_program(&[
            (
                "/app/a.ts",
                "declare var dec: any;\n@dec export class A {}\n",
            ),
            (
                "/app/node_modules/tslib/index.d.ts",
                "export declare function __decorate(decorators: Function[], target: any, key?: string | symbol, desc?: any): any;\n",
            ),
        ]);
    let options = ResolvedCompilerOptions {
        import_helpers: true,
        checker: tsz_common::checker_options::CheckerOptions {
            target: tsz_common::ScriptTarget::ES2015,
            experimental_decorators: true,
            ..Default::default()
        },
        ..ResolvedCompilerOptions::default()
    };

    let diagnostics = detect_missing_tslib_helper_diagnostics(
        &program,
        &options,
        Path::new("/app"),
        &rustc_hash::FxHashMap::default(),
    );

    assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.code == 2343 || diag.code == 2354),
            "Did not expect tslib helper diagnostics when index.d.ts declares __decorate. Got: {diagnostics:#?}"
        );
}

#[test]
fn ambient_tslib_helper_comments_do_not_satisfy_missing_helpers() {
    let program = merged_program(&[
            (
                "main.ts",
                "export async function load(): Promise<number> {\n    await Promise.resolve();\n    return 1;\n}\n",
            ),
            (
                "node_modules/tslib/tslib.d.ts",
                r#"declare module "tslib" {
  // Mentioning __importStar in a comment should not provide any helper export.
  // export declare function __awaiter(thisArg: any, _arguments: any, P: any, generator: any): any;
  export {};
}
"#,
            ),
        ]);
    let mut options = ResolvedCompilerOptions {
        import_helpers: true,
        ..Default::default()
    };
    options.checker.target = tsz_common::ScriptTarget::ES5;

    let diagnostics = detect_missing_tslib_helper_diagnostics(
        &program,
        &options,
        Path::new("/"),
        &rustc_hash::FxHashMap::default(),
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2343 && diag.file == "main.ts" && diag.message_text.contains("__awaiter")
        }),
        "Expected TS2343 for missing __awaiter. Got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2343 && diag.file == "main.ts" && diag.message_text.contains("__generator")
        }),
        "Expected TS2343 for missing __generator. Got: {diagnostics:#?}"
    );
}

#[test]
fn ambient_tslib_helper_declarations_satisfy_async_helpers() {
    let program = merged_program(&[
            (
                "main.ts",
                "export async function load(): Promise<number> {\n    await Promise.resolve();\n    return 1;\n}\n",
            ),
            (
                "node_modules/tslib/tslib.d.ts",
                r#"declare module "tslib" {
  export declare function __awaiter(thisArg: any, _arguments: any, P: any, generator: any): any;
  export declare function __generator(thisArg: any, body: any): any;
}
"#,
            ),
        ]);
    let mut options = ResolvedCompilerOptions {
        import_helpers: true,
        ..Default::default()
    };
    options.checker.target = tsz_common::ScriptTarget::ES5;

    let diagnostics = detect_missing_tslib_helper_diagnostics(
        &program,
        &options,
        Path::new("/"),
        &rustc_hash::FxHashMap::default(),
    );

    assert!(
            !diagnostics.iter().any(|diag| diag.code == 2343),
            "Did not expect missing-helper diagnostics when ambient tslib declares async helpers. Got: {diagnostics:#?}"
        );
}

#[test]
fn no_types_and_symbols_still_honors_project_local_tslib() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let tslib_dir = temp_dir.path().join("node_modules").join("tslib");
    std::fs::create_dir_all(&tslib_dir).unwrap();
    std::fs::write(
            tslib_dir.join("index.d.ts"),
            "export declare function __decorate(decorators: Function[], target: any, key?: string | symbol, desc?: any): any;\n",
        )
        .unwrap();

    let program = merged_program(&[(
        "/app/a.ts",
        "declare var dec: any, __decorate: any;\n@dec export class A {}\n",
    )]);
    let options = ResolvedCompilerOptions {
        import_helpers: true,
        checker: tsz_common::checker_options::CheckerOptions {
            target: tsz_common::ScriptTarget::ES2015,
            experimental_decorators: true,
            no_types_and_symbols: true,
            ..Default::default()
        },
        ..ResolvedCompilerOptions::default()
    };

    let diagnostics = detect_missing_tslib_helper_diagnostics(
        &program,
        &options,
        temp_dir.path(),
        &rustc_hash::FxHashMap::default(),
    );

    assert!(
            !diagnostics
                .iter()
                .any(|diag| diag.code == 2343 || diag.code == 2354),
            "Did not expect tslib helper diagnostics when a project-local tslib exists. Got: {diagnostics:#?}"
        );
}

#[test]
fn old_tslib_private_instance_helpers_report_ts2807_for_get_and_set() {
    let program = merged_program(&[
        (
            "main.ts",
            r#"
export class C {
    #a = 1;
    #b() { this.#c = 42; }
    set #c(v: number) { this.#a += v; }
}
"#,
        ),
        (
            "node_modules/tslib/index.d.ts",
            r#"
export declare function __classPrivateFieldGet<T extends object, V>(receiver: T, state: any): V;
export declare function __classPrivateFieldSet<T extends object, V>(receiver: T, state: any, value: V): V;
"#,
        ),
    ]);
    let mut options = ResolvedCompilerOptions {
        import_helpers: true,
        ..Default::default()
    };
    options.checker.target = tsz_common::ScriptTarget::ES2015;

    let diagnostics = detect_missing_tslib_helper_diagnostics(
        &program,
        &options,
        Path::new("/"),
        &rustc_hash::FxHashMap::default(),
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2807
                && diag.file == "main.ts"
                && diag.message_text.contains("__classPrivateFieldSet")
                && diag.message_text.contains("5 parameters")
        }),
        "Expected TS2807 for old __classPrivateFieldSet helper. Got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2807
                && diag.file == "main.ts"
                && diag.message_text.contains("__classPrivateFieldGet")
                && diag.message_text.contains("4 parameters")
        }),
        "Expected TS2807 for old __classPrivateFieldGet helper. Got: {diagnostics:#?}"
    );
}

#[test]
fn old_tslib_private_static_helpers_report_ts2807_for_get_and_set() {
    let program = merged_program(&[
        (
            "main.ts",
            r#"
export class S {
    static #a = 1;
    static #b() { this.#a = 42; }
    static get #c() { return S.#b(); }
}
"#,
        ),
        (
            "node_modules/tslib/index.d.ts",
            r#"
export declare function __classPrivateFieldGet<T extends object, V>(receiver: T, state: any): V;
export declare function __classPrivateFieldSet<T extends object, V>(receiver: T, state: any, value: V): V;
"#,
        ),
    ]);
    let mut options = ResolvedCompilerOptions {
        import_helpers: true,
        ..Default::default()
    };
    options.checker.target = tsz_common::ScriptTarget::ES2015;

    let diagnostics = detect_missing_tslib_helper_diagnostics(
        &program,
        &options,
        Path::new("/"),
        &rustc_hash::FxHashMap::default(),
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2807
                && diag.file == "main.ts"
                && diag.message_text.contains("__classPrivateFieldSet")
                && diag.message_text.contains("5 parameters")
        }),
        "Expected TS2807 for old static __classPrivateFieldSet helper. Got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2807
                && diag.file == "main.ts"
                && diag.message_text.contains("__classPrivateFieldGet")
                && diag.message_text.contains("4 parameters")
        }),
        "Expected TS2807 for old static __classPrivateFieldGet helper. Got: {diagnostics:#?}"
    );
}

#[test]
fn decorated_class_emits_es_decorate_and_run_initializers() {
    let helpers = helper_names("declare var dec: any;\n@dec class C { method() {} }");
    assert!(helpers.contains(&"__esDecorate"), "got: {helpers:?}");
    assert!(helpers.contains(&"__runInitializers"), "got: {helpers:?}");
    // Named non-default class should not need __setFunctionName
    assert!(!helpers.contains(&"__setFunctionName"), "got: {helpers:?}");
}

#[test]
fn decorated_class_with_private_method_emits_set_function_name() {
    let helpers = helper_names("declare var dec: any;\n@dec class C { #privateMethod() {} }");
    assert!(helpers.contains(&"__esDecorate"), "got: {helpers:?}");
    assert!(helpers.contains(&"__runInitializers"), "got: {helpers:?}");
    assert!(
        helpers.contains(&"__setFunctionName"),
        "private method should trigger __setFunctionName, got: {helpers:?}"
    );
}

#[test]
fn decorator_takes_priority_over_private_field() {
    let helpers = helper_names("declare var dec: any;\n@dec class C { #foo = 1; }");
    // ES decorators handle private fields internally
    assert!(helpers.contains(&"__esDecorate"), "got: {helpers:?}");
    assert!(
        !helpers.contains(&"__classPrivateFieldSet"),
        "decorator should take priority, got: {helpers:?}"
    );
}

#[test]
fn class_with_extends_emits_extends_helper() {
    let helpers = helper_names("class Base {} class Derived extends Base {}");
    assert_eq!(helpers, vec!["__extends"]);
}

#[test]
fn class_with_extends_no_helper_at_es2015() {
    // At ES2015+, class syntax is native — __extends is not needed
    let helpers = helper_names_at(
        "class Base {} class Derived extends Base {}",
        tsz_common::ScriptTarget::ES2015,
    );
    assert!(
        !helpers.contains(&"__extends"),
        "ES2015 target should not need __extends, got: {helpers:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_suppresses_await_ts1359_when_ts1109_present() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 100,
            length: 5,
            message: "Identifier expected. 'await' is a reserved word that cannot be used here."
                .to_string(),
            code: 1359,
        },
        ParseDiagnostic {
            start: 200,
            length: 1,
            message: "Expression expected.".to_string(),
            code: 1109,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&1359),
        "TS1359 for 'await' should be suppressed when TS1109 is present, got: {codes:?}"
    );
    assert!(
        codes.contains(&1109),
        "TS1109 should still be present, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_await_ts1359_with_unrelated_parse_errors() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![
        ParseDiagnostic {
            start: 100,
            length: 5,
            message: "Identifier expected. 'await' is a reserved word that cannot be used here."
                .to_string(),
            code: 1359,
        },
        ParseDiagnostic {
            start: 10,
            length: 6,
            message: "A module cannot have multiple default exports.".to_string(),
            code: 2528,
        },
    ];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1359),
        "TS1359 for 'await' should survive unrelated parse diagnostics, got: {codes:?}"
    );
}

#[test]
fn filtered_parse_diagnostics_keeps_await_ts1359_when_alone() {
    use tsz::parser::ParseDiagnostic;

    let diagnostics = vec![ParseDiagnostic {
        start: 100,
        length: 5,
        message: "Identifier expected. 'await' is a reserved word that cannot be used here."
            .to_string(),
        code: 1359,
    }];

    let filtered = filtered_parse_diagnostics(&diagnostics, false);
    let codes: Vec<u32> = filtered.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&1359),
        "TS1359 for 'await' should be kept when it's the only diagnostic, got: {codes:?}"
    );
}

#[test]
fn js_parse_allowlist_keeps_plain_js_binder_strict_codes() {
    for code in [1214, 18012] {
        assert!(
            is_ts1xxx_allowed_in_js(code),
            "plain JS binder parse diagnostic TS{code} should be reported in JavaScript files"
        );
    }
}

#[test]
fn js_parse_allowlist_keeps_ts2657() {
    assert!(
        is_ts1xxx_allowed_in_js(2657),
        "TS2657 should be preserved for JS JSX recovery diagnostics"
    );
}

#[test]
fn js_parse_allowlist_keeps_ts17002() {
    assert!(
        is_ts1xxx_allowed_in_js(17002),
        "TS17002 should be preserved for JS JSX closing-tag mismatch diagnostics"
    );
}

#[test]
fn js_parse_allowlist_keeps_ts17014() {
    assert!(
        is_ts1xxx_allowed_in_js(17014),
        "TS17014 should be preserved for JS JSX fragment recovery diagnostics"
    );
}

#[test]
fn js_parse_allowlist_keeps_ts1163() {
    assert!(
        is_ts1xxx_allowed_in_js(1163),
        "TS1163 should be preserved for JS yield-outside-generator diagnostics"
    );
}

// ---------------------------------------------------------------
// Export signature tests: CLI path via build_export_signature_input
// ---------------------------------------------------------------

/// Helper: compute export signature from source via the CLI pipeline
/// (`parse_and_bind_single` → merge → `build_export_signature_input` → `from_input`).
fn cli_export_signature(source: &str) -> tsz_lsp::export_signature::ExportSignature {
    let bind_result = parallel::parse_and_bind_single("test.ts".to_string(), source.to_string());
    let program = parallel::merge_bind_results(vec![bind_result]);
    let file = &program.files[0];
    compute_export_signature(&program, file, 0)
}

/// Helper: compute CLI export signature input (for structural inspection).
fn cli_export_input(source: &str) -> tsz_lsp::export_signature::ExportSignatureInput {
    let bind_result = parallel::parse_and_bind_single("test.ts".to_string(), source.to_string());
    let program = parallel::merge_bind_results(vec![bind_result]);
    let file = &program.files[0];
    build_export_signature_input(&program, file, 0)
}

#[test]
fn body_only_edit_preserves_signature() {
    let before = "export function foo() { return 1; }";
    let after = "export function foo() { return 42; }";
    assert_eq!(
        cli_export_signature(before),
        cli_export_signature(after),
        "body-only edit must not change export signature"
    );
}

#[test]
fn comment_only_edit_preserves_signature() {
    let before = "// original comment\nexport const x = 1;";
    let after = "// modified comment with extra words\nexport const x = 1;";
    assert_eq!(
        cli_export_signature(before),
        cli_export_signature(after),
        "comment-only edit must not change export signature"
    );
}

#[test]
fn private_symbol_edit_preserves_signature() {
    let before = "const priv = 1;\nexport const pub_val = priv;";
    let after = "const priv = 999;\nconst priv2 = 2;\nexport const pub_val = priv;";
    assert_eq!(
        cli_export_signature(before),
        cli_export_signature(after),
        "private symbol additions/edits must not change export signature"
    );
}

#[test]
fn adding_export_changes_signature() {
    let before = "export const x = 1;";
    let after = "export const x = 1;\nexport const y = 2;";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "adding a new export must change the signature"
    );
}

#[test]
fn removing_export_changes_signature() {
    let before = "export const x = 1;\nexport const y = 2;";
    let after = "export const x = 1;";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "removing an export must change the signature"
    );
}

#[test]
fn re_export_edit_changes_signature() {
    let before = "export { foo } from './other';";
    let after = "export { foo, bar } from './other';";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "adding a named re-export must change the signature"
    );
}

#[test]
fn wildcard_re_export_changes_signature() {
    let before = "export const x = 1;";
    let after = "export const x = 1;\nexport * from './other';";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "adding a wildcard re-export must change the signature"
    );
}

#[test]
fn augmentation_edit_changes_signature() {
    let before = "export const x = 1;";
    let after = "export const x = 1;\ndeclare global { interface Window { foo: string; } }";
    assert_ne!(
        cli_export_signature(before),
        cli_export_signature(after),
        "adding a global augmentation must change the signature"
    );
}

#[test]
fn export_input_captures_exports() {
    let input = cli_export_input("export const x = 1;\nexport function foo() {}");
    let names: Vec<&str> = input.exports.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(names.contains(&"x"), "should contain x export: {names:?}");
    assert!(
        names.contains(&"foo"),
        "should contain foo export: {names:?}"
    );
}

#[test]
fn export_input_captures_re_exports() {
    let input = cli_export_input("export { bar } from './other';");
    let re_names: Vec<&str> = input
        .named_reexports
        .iter()
        .map(|(n, _, _)| n.as_str())
        .collect();
    assert!(
        re_names.contains(&"bar"),
        "should contain bar re-export: {re_names:?}"
    );
}

#[test]
fn export_input_captures_wildcard_re_exports() {
    let input = cli_export_input("export * from './other';");
    assert_eq!(
        input.wildcard_reexports.len(),
        1,
        "should have one wildcard re-export"
    );
    assert_eq!(input.wildcard_reexports[0].0, "./other");
}

#[test]
fn export_input_ignores_private_symbols() {
    let input = cli_export_input("const priv = 1;\nexport const pub_val = priv;");
    let names: Vec<&str> = input.exports.iter().map(|(n, _, _)| n.as_str()).collect();
    assert!(
        !names.contains(&"priv"),
        "private symbols must not appear in export input"
    );
    assert!(names.contains(&"pub_val"));
}

#[test]
fn regex_flag_errors_do_not_suppress_semantic_diagnostics() {
    // TS1499 (unknown regex flag) should not set has_syntax_parse_errors,
    // so TS2339 (property does not exist) should still be emitted.
    assert!(
        is_non_suppressing_parse_error(1499),
        "TS1499 (Unknown regex flag) should be non-suppressing"
    );
    assert!(
        is_non_suppressing_parse_error(1500),
        "TS1500 (Duplicate regex flag) should be non-suppressing"
    );
    assert!(
        is_non_suppressing_parse_error(1502),
        "TS1502 (Incompatible u/v flags) should be non-suppressing"
    );
}

/// Helper: parse a single file and collect noCheck path diagnostics.
fn collect_no_check_diags(file_name: &str, source: &str) -> Vec<Diagnostic> {
    let mut parse_results =
        parallel::parse_files_parallel(vec![(file_name.to_string(), source.to_string())]);
    let result = parse_results.remove(0);
    let options = ResolvedCompilerOptions::default();
    let program_has_real_syntax_errors = result
        .parse_diagnostics
        .iter()
        .any(|d| is_real_syntax_error(d.code));
    collect_no_check_parse_diagnostics_for_file(
        &result.file_name,
        &result.arena,
        result.source_file,
        &result.parse_diagnostics,
        &options,
        program_has_real_syntax_errors,
    )
}

#[test]
fn no_check_path_emits_ts8010_for_js_parameter_type_annotation() {
    // Issue #3692: `--noCheck` previously skipped TS8xxx grammar
    // diagnostics that tsc reports from its parser. Confirm that a
    // type-annotated JS parameter still produces TS8010 here.
    let diagnostics = collect_no_check_diags("a.js", "function f(x: number) {}\n");
    assert!(
        diagnostics.iter().any(|d| d.code == 8010),
        "expected TS8010 in JS noCheck output, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_path_emits_ts8010_for_js_variable_type_annotation() {
    // Variable declarations with TS-only type annotations also surface.
    let diagnostics = collect_no_check_diags("a.js", "let x: number;\n");
    assert!(
        diagnostics.iter().any(|d| d.code == 8010),
        "expected TS8010 in JS noCheck output for `let x: number`, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_path_does_not_emit_ts8010_for_typescript_files() {
    // The grammar walker must not fire on TypeScript files.
    let diagnostics = collect_no_check_diags("a.ts", "function f(x: number) {}\n");
    assert!(
        !diagnostics.iter().any(|d| d.code == 8010),
        "TS8010 must not fire on TypeScript files, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_ts_expect_error_does_not_suppress_parse_error() {
    // Under `--noCheck`, `@ts-expect-error` must not suppress parse errors
    // (TS1109 "Expression expected"). tsc reports parse diagnostics from
    // `getSyntacticDiagnostics` which bypasses directive suppression.
    let source = "// @ts-expect-error\nconst broken = ;\n";
    let diagnostics = collect_no_check_diags("a.ts", source);
    assert!(
        diagnostics.iter().any(|d| d.code == 1109),
        "TS1109 must not be suppressed by @ts-expect-error in --noCheck, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted under --noCheck, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_ts_ignore_does_not_suppress_parse_error() {
    // `@ts-ignore` must also not suppress parse errors under `--noCheck`.
    let source = "// @ts-ignore\nconst broken = ;\n";
    let diagnostics = collect_no_check_diags("a.ts", source);
    assert!(
        diagnostics.iter().any(|d| d.code == 1109),
        "TS1109 must survive @ts-ignore in --noCheck, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_ts_expect_error_does_not_suppress_js_grammar_error() {
    // Under `--noCheck`, `@ts-expect-error` must not suppress JS grammar
    // errors (TS8010 "Type annotations can only be used in TypeScript files").
    let source = "// @ts-expect-error\nlet x: number;\n";
    let diagnostics = collect_no_check_diags("a.js", source);
    assert!(
        diagnostics.iter().any(|d| d.code == 8010),
        "TS8010 must not be suppressed by @ts-expect-error in --noCheck JS, got: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted under --noCheck, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_ts_expect_error_on_clean_line_does_not_emit_ts2578() {
    // Under `--noCheck`, an @ts-expect-error directive above a line with
    // no diagnostics must not produce TS2578 ("Unused '@ts-expect-error'").
    // tsc does not run type-checking in --noCheck mode so every directive
    // is effectively unreachable; none should be penalized.
    let source = "// @ts-expect-error\nconst x = 5;\n";
    let diagnostics = collect_no_check_diags("a.ts", source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted under --noCheck for unused directive, got: {diagnostics:#?}"
    );
}

#[test]
fn no_check_multiple_expect_error_directives_do_not_emit_ts2578() {
    // Multiple @ts-expect-error directives under --noCheck must all be
    // silently ignored rather than producing a wave of TS2578 reports.
    let source = concat!(
        "// @ts-expect-error\nconst a = 1;\n",
        "// @ts-expect-error\nconst b = 2;\n",
    );
    let diagnostics = collect_no_check_diags("a.ts", source);
    assert!(
            !diagnostics.iter().any(|d| d.code == 2578),
            "TS2578 must not fire for multiple unused directives under --noCheck, got: {diagnostics:#?}"
        );
}

fn check_directive_suppression(source: &str, codes_in: &[u32]) -> Vec<Diagnostic> {
    let line_starts = build_line_starts(source);
    let line1_start = line_starts.get(1).copied().unwrap_or(0);
    let mut diagnostics: Vec<Diagnostic> = codes_in
        .iter()
        .map(|&code| {
            Diagnostic::error(
                "test.ts".to_string(),
                line1_start,
                1,
                format!("diag {code}"),
                code,
            )
        })
        .collect();
    apply_ts_directive_suppression("test.ts", source, &mut diagnostics, false);
    diagnostics
}

#[test]
fn apply_suppression_never_suppresses_real_syntax_errors() {
    // TS1109 (Expression expected) is a real syntax error and must survive
    // directive suppression even in the full-check path. It still marks
    // @ts-expect-error as used, matching tsc's TS2578 behavior.
    let source = "// @ts-expect-error\nconst broken = ;\n";
    let remaining = check_directive_suppression(source, &[1109]);
    assert!(
        remaining.iter().any(|d| d.code == 1109),
        "TS1109 must not be suppressed, got: {remaining:#?}"
    );
    assert!(
        !remaining.iter().any(|d| d.code == 2578),
        "TS2578 must not be emitted when directive targets a parse error, got: {remaining:#?}"
    );
}

#[test]
fn apply_suppression_never_suppresses_js_only_syntactic_errors() {
    let source = "// @ts-expect-error\nlet x: number;\n";
    let remaining = check_directive_suppression(source, &[8010]);
    assert!(
        remaining.iter().any(|d| d.code == 8010),
        "TS8010 must not be suppressed, got: {remaining:#?}"
    );
    assert!(
            !remaining.iter().any(|d| d.code == 2578),
            "TS2578 must not be emitted when directive targets a JS syntactic diagnostic, got: {remaining:#?}"
        );
}

#[test]
fn apply_suppression_suppresses_semantic_error_but_not_parse_error_on_same_line() {
    // When a parse error (TS1109) and a semantic error (TS2322) both exist
    // on the target line, the semantic error is suppressed and the parse
    // error survives. The directive is marked as used, so no TS2578.
    let source = "// @ts-expect-error\nconst x: string = ;\n";
    let remaining = check_directive_suppression(source, &[1109, 2322]);
    assert!(
        remaining.iter().any(|d| d.code == 1109),
        "TS1109 must survive directive suppression, got: {remaining:#?}"
    );
    assert!(
        !remaining.iter().any(|d| d.code == 2322),
        "TS2322 must be suppressed by @ts-expect-error, got: {remaining:#?}"
    );
    assert!(
        !remaining.iter().any(|d| d.code == 2578),
        "TS2578 must not fire when directive suppressed a semantic error, got: {remaining:#?}"
    );
}

#[test]
fn apply_suppression_real_syntax_error_codes_are_never_suppressed() {
    // Verify several codes from is_real_syntax_error are all immune.
    let real_syntax_codes: &[u32] = &[1002, 1003, 1005, 1006, 1007, 1109, 1110, 1126, 1127];
    let source = "// @ts-expect-error\ncode_on_line_2;\n";
    for &code in real_syntax_codes {
        let remaining = check_directive_suppression(source, &[code]);
        assert!(
            remaining.iter().any(|d| d.code == code),
            "TS{code} must not be suppressed by @ts-expect-error, got: {remaining:#?}"
        );
    }
}
