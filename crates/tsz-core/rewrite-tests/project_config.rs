use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tempfile::TempDir;
use tsz::config::{CompilerOptionPatch, ProjectRequest, ProjectSelection, resolve_project};
use tsz::host::{HostEntry, ProgramHost, SystemHost};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

struct CountingHost {
    inner: SystemHost,
    counted_name: &'static str,
    reads: AtomicUsize,
}

impl CountingHost {
    fn new(root: &Path, counted_name: &'static str) -> Self {
        Self {
            inner: SystemHost::new(root),
            counted_name,
            reads: AtomicUsize::new(0),
        }
    }
}

impl ProgramHost for CountingHost {
    fn current_directory(&self) -> &Path {
        self.inner.current_directory()
    }

    fn use_case_sensitive_file_names(&self) -> bool {
        self.inner.use_case_sensitive_file_names()
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.inner.file_exists(path)
    }

    fn directory_exists(&self, path: &Path) -> bool {
        self.inner.directory_exists(path)
    }

    fn read_file(&self, path: &Path) -> io::Result<String> {
        if path
            .file_name()
            .is_some_and(|name| name == self.counted_name)
        {
            self.reads.fetch_add(1, Ordering::Relaxed);
        }
        self.inner.read_file(path)
    }

    fn read_directory(&self, path: &Path) -> io::Result<Vec<HostEntry>> {
        self.inner.read_directory(path)
    }

    fn realpath(&self, path: &Path) -> PathBuf {
        self.inner.realpath(path)
    }
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("test path has parent")).expect("create parent");
    fs::write(path, text).expect("write fixture");
}

fn relative_paths(root: &Path, paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

fn codes(project: &tsz::config::ResolvedProject) -> Vec<u32> {
    project
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn output_codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn compiler_defaults_follow_the_pinned_typescript_7_frontend() {
    let options = tsz::CompilerOptions::default();
    assert!(options.strict);
    assert!(options.effective_no_implicit_any());
    assert_eq!(options.module, "preserve");

    let output = Compiler::new().compile(
        vec![tsz::SourceInput::new(
            "implicit.ts",
            "function identity(value) { return value; }",
        )],
        &tsz::CompilerOptions {
            no_emit: true,
            ..options
        },
    );
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [7006]
    );

    let opted_out = Compiler::new().compile(
        vec![tsz::SourceInput::new(
            "implicit.ts",
            "function identity(value) { return value; }",
        )],
        &tsz::CompilerOptions {
            no_emit: true,
            no_implicit_any: Some(false),
            ..tsz::CompilerOptions::default()
        },
    );
    assert!(
        opted_out.diagnostics.is_empty(),
        "{:?}",
        opted_out.diagnostics
    );
}

#[test]
fn config_preserves_explicit_strict_suboption_false() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
          "compilerOptions": { "strict": true, "noImplicitAny": false, "noEmit": true },
          "files": ["implicit.ts"]
        }"#,
    );
    write(
        root,
        "implicit.ts",
        "function identity(value) { return value; }\n",
    );
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    assert!(resolved.options.strict);
    assert_eq!(resolved.options.no_implicit_any, Some(false));
    assert!(!resolved.options.effective_no_implicit_any());
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn unmodeled_unused_diagnostic_options_are_typed_nonclaims() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
          "compilerOptions": { "noUnusedLocals": true, "noUnusedParameters": true, "noEmit": true },
          "files": ["unused.ts"]
        }"#,
    );
    write(
        root,
        "unused.ts",
        "const holder = function renamed(parameter: number) {\n\
         \tconst nested = function changed(inner: number) { return 1; };\n\
         \treturn nested;\n\
         };\n\
         const independent: MissingIndependent = 1;\n",
    );
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    assert!(resolved.options.no_unused_locals);
    assert!(resolved.options.no_unused_parameters);
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2304]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);

    let control = Compiler::new().compile(
        vec![SourceInput::new(
            "unused.ts",
            "const holder = function renamed(parameter: number) { return 1; };\n\
             const independent: MissingIndependent = 1;\n",
        )],
        &CompilerOptions {
            no_emit: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(
        control
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2304]
    );
    assert_eq!(control.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn jsonc_default_discovery_is_deterministic_and_skips_package_directories() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        "\u{feff}{\n  // JSONC comments and trailing commas are accepted\n  \"compilerOptions\": { \"strict\": true, },\n}\n",
    );
    write(root, "z.ts", "export const z = 1;\n");
    write(root, "a.ts", "export const a = 1;\n");
    write(root, "nested/b.ts", "export const b = 1;\n");
    write(root, "nested/a.tsx", "export const a = <div />;\n");
    write(
        root,
        "node_modules/pkg/index.ts",
        "export const hidden = 1;\n",
    );
    write(root, ".generated/hidden.ts", "export const hidden = 1;\n");
    write(root, ".hidden.ts", "export const hidden = 1;\n");
    write(root, "UPPER.TS", "export const upper = 1;\n");
    write(root, "ignored.js", "exports.value = 1;\n");

    let host = SystemHost::new(root);
    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
    assert!(project.options.strict);
    assert_eq!(
        relative_paths(root, &project.root_files),
        ["a.ts", "z.ts", "nested/a.tsx", "nested/b.ts"]
    );
}

#[test]
fn jsonc_preserves_unicode_selector_text() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        "{\n  // non-ASCII strings remain UTF-8\n  \"files\": [\"grüße.ts\",],\n}\n",
    );
    write(root, "grüße.ts", "export const grüße = 1;\n");
    let host = SystemHost::new(root);
    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
    assert_eq!(relative_paths(root, &project.root_files), ["grüße.ts"]);
}

#[test]
fn question_wildcard_matches_one_unicode_scalar() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "tsconfig.json", r#"{"include":["?.ts"]}"#);
    write(root, "é.ts", "export {};\n");
    write(root, "ab.ts", "export {};\n");
    let host = SystemHost::new(root);
    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
    assert_eq!(relative_paths(root, &project.root_files), ["é.ts"]);
}

#[test]
fn explicit_include_can_enter_implicitly_excluded_directories() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{"include":["**/node_modules/pkg/*.ts","**/.generated/*.ts"]}"#,
    );
    write(root, "node_modules/pkg/index.ts", "export {};\n");
    write(root, ".generated/output.ts", "export {};\n");
    write(root, "node_modules/other/ignored.ts", "export {};\n");
    write(root, ".other/ignored.ts", "export {};\n");
    let host = SystemHost::new(root);
    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
    assert_eq!(
        relative_paths(root, &project.root_files),
        ["node_modules/pkg/index.ts", ".generated/output.ts"]
    );
}

#[test]
fn files_and_include_form_a_union_and_exclude_only_filters_wildcards() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
          "files": ["generated/keep.ts", "literal.ts"],
          "include": ["second/**/*.ts", "first/**/*.ts", "generated/**/*.ts"],
          "exclude": ["generated"]
        }"#,
    );
    write(root, "generated/keep.ts", "export const keep = 1;\n");
    write(root, "generated/drop.ts", "export const drop = 1;\n");
    write(root, "literal.ts", "export const literal = 1;\n");
    write(root, "first/z.ts", "export const z = 1;\n");
    write(root, "first/a.ts", "export const a = 1;\n");
    write(root, "second/b.ts", "export const b = 1;\n");

    let host = SystemHost::new(root);
    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
    assert_eq!(
        relative_paths(root, &project.root_files),
        [
            "generated/keep.ts",
            "literal.ts",
            "second/b.ts",
            "first/a.ts",
            "first/z.ts",
        ]
    );
}

#[test]
fn empty_selector_diagnostics_follow_typescript_precedence() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "empty-files.json", r#"{"files": []}"#);
    write(
        root,
        "empty-files-with-include.json",
        r#"{"files": [], "include": ["present.ts"]}"#,
    );
    write(root, "present.ts", "export {};\n");
    write(root, "empty-include.json", r#"{"include": []}"#);
    write(root, "base.json", r#"{"files": []}"#);
    write(root, "extended.json", r#"{"extends": "./base.json"}"#);

    let host = SystemHost::new(root);
    let files = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("empty-files.json"))),
    );
    assert_eq!(codes(&files), [18002]);
    assert!(
        files.diagnostics[0]
            .message_text
            .contains("The 'files' list in config file")
    );

    let files_with_include = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(
            root.join("empty-files-with-include.json"),
        )),
    );
    assert_eq!(codes(&files_with_include), [18002]);
    assert_eq!(
        relative_paths(root, &files_with_include.root_files),
        ["present.ts"]
    );

    let include = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("empty-include.json"))),
    );
    assert_eq!(codes(&include), [18003]);
    assert!(
        include.diagnostics[0]
            .message_text
            .contains("Specified 'include' paths were '[]'")
    );

    let extended = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("extended.json"))),
    );
    assert!(
        extended.diagnostics.is_empty(),
        "{:?}",
        extended.diagnostics
    );
    assert_eq!(extended.project_config_count, 2);
}

#[test]
fn extends_arrays_merge_left_to_right_and_keep_selector_origin() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "bases/one/base.json",
        r#"{
          "compilerOptions": { "strict": true, "target": "es2020" },
          "include": ["src/**/*.ts"]
        }"#,
    );
    write(
        root,
        "bases/two/base.json",
        r#"{
          "compilerOptions": { "strict": false, "module": "esnext" },
          "files": ["owned/literal.ts"],
          "include": ["owned/**/*.ts"],
          "exclude": ["owned/excluded"]
        }"#,
    );
    write(
        root,
        "app/tsconfig.json",
        r#"{
          "extends": ["../bases/one/base.json", "../bases/two/base.json"],
          "compilerOptions": { "target": "es2024" }
        }"#,
    );
    write(root, "bases/one/src/not-selected.ts", "export {};\n");
    write(root, "bases/two/owned/literal.ts", "export {};\n");
    write(root, "bases/two/owned/selected.ts", "export {};\n");
    write(root, "bases/two/owned/excluded/drop.ts", "export {};\n");
    write(root, "app/owned/not-selected.ts", "export {};\n");

    let host = SystemHost::new(root.join("app"));
    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(PathBuf::from("."))),
    );

    assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
    assert!(!project.options.strict);
    assert_eq!(project.options.target, "es2024");
    assert_eq!(project.options.module, "esnext");
    assert_eq!(project.project_config_count, 3);
    assert_eq!(
        relative_paths(root, &project.root_files),
        ["bases/two/owned/literal.ts", "bases/two/owned/selected.ts"]
    );
}

#[test]
fn extends_cycles_and_missing_bases_are_normal_core_diagnostics() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "a.json", r#"{"extends":"./b.json","files":[]}"#);
    write(root, "b.json", r#"{"extends":"./a.json"}"#);
    write(
        root,
        "missing.json",
        r#"{"extends":"./does-not-exist","files":[]}"#,
    );
    write(
        root,
        "missing-with-default.json",
        r#"{"extends":"./does-not-exist"}"#,
    );
    let host = SystemHost::new(root);

    let cycle = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("a.json"))),
    );
    assert!(codes(&cycle).contains(&18000), "{:?}", cycle.diagnostics);
    assert!(cycle.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == 18000 && diagnostic.message_text.contains("a.json ->")
    }));

    let missing = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("missing.json"))),
    );
    assert_eq!(codes(&missing), [6053]);
    assert_eq!(
        missing.diagnostics[0].message_text,
        "File './does-not-exist' not found."
    );

    let missing_with_default = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(
            root.join("missing-with-default.json"),
        )),
    );
    assert_eq!(codes(&missing_with_default), [6053, 18003]);
}

#[test]
fn cyclic_partial_configs_never_enter_the_definitive_cache() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "a.json",
        r#"{"extends":["./b.json","./b.json"],"files":[]}"#,
    );
    write(
        root,
        "b.json",
        r#"{"extends":"./a.json","compilerOptions":{"strict":false}}"#,
    );
    let host = CountingHost::new(root, "b.json");

    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("a.json"))),
    );

    assert!(
        codes(&project).contains(&18000),
        "{:?}",
        project.diagnostics
    );
    assert!(!project.options.strict);
    assert_eq!(
        host.reads.load(Ordering::Relaxed),
        2,
        "an incomplete cycle result was reused as a complete config"
    );
}

#[test]
fn project_directory_file_and_ancestor_search_share_core_resolution() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "project/tsconfig.json", r#"{"files":["entry.ts"]}"#);
    write(root, "project/entry.ts", "export const entry = 1;\n");
    fs::create_dir_all(root.join("project/deep/child")).expect("nested directory");
    fs::create_dir_all(root.join("without-config")).expect("empty directory");
    let host = SystemHost::new(root);

    let directory = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("project"))),
    );
    let file = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(
            root.join("project/tsconfig.json"),
        )),
    );
    let search = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Search(root.join("project/deep/child"))),
    );
    assert_eq!(directory.root_files, file.root_files);
    assert_eq!(file.root_files, search.root_files);
    let options = directory.options.clone();
    let output = Compiler::new().compile_resolved(directory, &options);
    assert_eq!(
        output.program.files[0].source.path,
        Path::new("project/entry.ts")
    );
    assert_eq!(
        output.program.files[0].source.host_path,
        root.join("project/entry.ts")
    );

    let no_directory_config = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("without-config"))),
    );
    assert_eq!(codes(&no_directory_config), [5081]);
    let no_file = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("absent.json"))),
    );
    assert_eq!(codes(&no_file), [5058]);
}

#[cfg(unix)]
#[test]
fn transport_alias_realpaths_only_normalize_source_root_identity() {
    let fixture = TempDir::new().expect("tempdir");
    let fixture_root = fixture.path();
    write(
        fixture_root,
        "real/tsconfig.json",
        r#"{"compilerOptions":{"noEmit":true,"strict":true},"files":["test.ts"]}"#,
    );
    write(fixture_root, "real/test.ts", "function bodyless():void;\n");
    std::os::unix::fs::symlink(
        fixture_root.join("real"),
        fixture_root.join("transport-alias"),
    )
    .expect("transport alias");
    let canonical_root = fixture_root
        .join("real")
        .canonicalize()
        .expect("canonical project root");
    let host = SystemHost::new(canonical_root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(
            fixture_root.join("transport-alias/tsconfig.json"),
        )),
    );
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);

    assert_eq!(output.program.files[0].source.path, Path::new("test.ts"));
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [2391]
    );
    assert_eq!(output.diagnostics[0].file, "test.ts");
}

#[cfg(unix)]
#[test]
fn symlink_spelled_config_preserves_diagnostic_and_output_layout() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "real/project/tsconfig.json",
        r#"{
            "compilerOptions":{"rootDir":".","outDir":"dist","declaration":true},
            "files":["entry.ts"]
        }"#,
    );
    write(
        root,
        "real/project/entry.ts",
        "export const value:number=1;\n",
    );
    std::os::unix::fs::symlink(root.join("real/project"), root.join("alias"))
        .expect("project symlink");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("alias/tsconfig.json"))),
    );
    assert_eq!(resolved.inputs[0].path, Path::new("alias/entry.ts"));
    assert_eq!(resolved.options.out_dir, Some(root.join("alias/dist")));
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    // CompileOutput is an in-memory product and retains the authored config
    // spelling. A filesystem adapter writes these paths to the same physical
    // targets as `real/project/dist/*` through the symlink.
    assert_eq!(
        relative_paths(
            root,
            &output
                .emitted_files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
        ),
        ["alias/dist/entry.d.ts", "alias/dist/entry.js"]
    );
}

#[test]
fn malformed_selected_config_remains_a_counted_project_attempt() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "tsconfig.json", "{\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    assert_eq!(codes(&resolved), [5083]);
    assert_eq!(resolved.project_config_count, 1);
    assert!(resolved.entry_config.is_some());
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert_eq!(output.stats.project_configs, 1);
    assert_eq!(output.stats.root_files, 0);
}

#[test]
fn explicit_root_order_is_stable_before_internal_program_canonicalization() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "a.ts", "export const a = 1;\n");
    write(root, "b.ts", "export const b = 1;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Files(vec![
            PathBuf::from("b.ts"),
            PathBuf::from("a.ts"),
            PathBuf::from("b.ts"),
        ])),
    );

    assert_eq!(relative_paths(root, &resolved.root_files), ["b.ts", "a.ts"]);
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert_eq!(
        output
            .program
            .files
            .iter()
            .map(|file| file.source.path.to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        ["a.ts", "b.ts"]
    );
    assert_eq!(
        output
            .program
            .source_order
            .iter()
            .map(|id| {
                output.program.files[id.0 as usize]
                    .source
                    .path
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>(),
        ["b.ts", "a.ts"]
    );
    assert_eq!(output.program.files[0].source.host_path, root.join("a.ts"));
    assert_eq!(output.stats.root_files, 2);
    assert_eq!(output.stats.source_files, 2);
    assert_eq!(
        output.stats.root_file_paths,
        [
            root.join("b.ts").to_string_lossy().replace('\\', "/"),
            root.join("a.ts").to_string_lossy().replace('\\', "/"),
        ]
    );
    assert_eq!(output.stats.root_file_paths, output.stats.source_file_paths);
}

#[test]
fn references_are_entry_metadata_and_do_not_union_referenced_roots() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "solution/tsconfig.json",
        r#"{"references":[{"path":"../library"}]}"#,
    );
    write(root, "library/tsconfig.json", r#"{"files":["library.ts"]}"#);
    write(root, "library/library.ts", "export const library = 1;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("solution"))),
    );

    assert!(
        resolved.diagnostics.is_empty(),
        "{:?}",
        resolved.diagnostics
    );
    assert!(resolved.root_files.is_empty());
    assert_eq!(resolved.project_config_count, 1);
    assert_eq!(resolved.project_reference_count, 1);
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.stats.root_files, 0);
    assert_eq!(output.stats.source_files, 0);
    assert_eq!(output.stats.project_configs, 1);
    assert_eq!(output.stats.project_references, 1);
}

#[test]
fn references_property_suppresses_no_input_and_missing_edges_report_6053() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "empty.json", r#"{"references":[]}"#);
    fs::create_dir(root.join("dependency")).expect("empty reference directory");
    write(
        root,
        "missing.json",
        r#"{"references":[{"path":"./dependency"}]}"#,
    );
    let host = SystemHost::new(root);

    let empty = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("empty.json"))),
    );
    assert!(empty.diagnostics.is_empty(), "{:?}", empty.diagnostics);
    assert_eq!(empty.root_files.len(), 0);

    let missing = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.join("missing.json"))),
    );
    assert_eq!(codes(&missing), [6053]);
    assert_eq!(missing.project_reference_count, 1);
    assert!(
        missing.diagnostics[0]
            .message_text
            .contains(&root.join("dependency").to_string_lossy().to_string())
    );
}

#[test]
fn reference_targets_follow_json_config_resolution_and_retain_jsonc_spans() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "plain.txt", "not a project configuration\n");
    write(root, "named.json", r#"{"files":[]}"#);
    fs::create_dir(root.join("without-config")).expect("reference directory");
    write(
        root,
        "tsconfig.json",
        r#"{
  "files": [],
  "references": [
    { "path": "./without-config" },
    { "path": "./plain.txt" },
    { "path": "./named.json" },
  ],
}"#,
    );
    let host = SystemHost::new(root);
    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    assert_eq!(codes(&project), [6053, 6053]);
    assert_eq!(project.project_reference_count, 3);
    assert_eq!(project.diagnostics[0].file, "tsconfig.json");
    assert_eq!(project.diagnostics[1].file, "tsconfig.json");
    assert_eq!(
        project.diagnostics[0].render(None),
        format!(
            "tsconfig.json(4,5): error TS6053: File '{}' not found.",
            root.join("without-config")
                .to_string_lossy()
                .replace('\\', "/")
        )
    );
    assert_eq!(
        project.diagnostics[1].render(None),
        format!(
            "tsconfig.json(5,5): error TS6053: File '{}' not found.",
            root.join("plain.txt").to_string_lossy().replace('\\', "/")
        )
    );
    assert!(
        project
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.length > 0)
    );
}

#[test]
fn missing_literal_roots_report_6053_and_stats_survive_config_errors() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"noLib":true},"files":["missing.ts"]}"#,
    );
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    assert_eq!(codes(&resolved), [6053]);
    let options = resolved.options.clone();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [6053]
    );
    assert_eq!(output.stats.root_files, 1);
    assert_eq!(output.stats.source_files, 0);
    assert_eq!(output.stats.project_configs, 1);
}

#[test]
fn literal_root_extensions_are_validated_before_source_parsing() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "main.js", "const value = 1;\n");
    write(root, "main.txt", "this is not TypeScript syntax @@@\n");
    write(root, "kept.ts", "export {};\n");
    write(
        root,
        "tsconfig.json",
        r#"{"files":["main.js","main.txt","missing.ts"]}"#,
    );
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    assert_eq!(codes(&resolved), [6053, 6054, 6504]);
    assert_eq!(resolved.root_files.len(), 3);
    assert_eq!(resolved.inputs.len(), 0);
    for diagnostic in &resolved.diagnostics {
        assert_eq!(diagnostic.related_information.len(), 2);
        assert_eq!(diagnostic.related_information[0].code, 1430);
        assert_eq!(diagnostic.related_information[0].depth, 1);
        assert_eq!(diagnostic.related_information[1].code, 1409);
        assert_eq!(diagnostic.related_information[1].depth, 2);
    }
    let unsupported = resolved
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 6054)
        .expect("unsupported-extension diagnostic");
    assert_eq!(
        unsupported.message_text,
        format!(
            "File '{}' has an unsupported extension. The only supported extensions are '.ts', '.tsx', '.d.ts', '.cts', '.d.cts', '.mts', '.d.mts'.",
            root.join("main.txt").to_string_lossy().replace('\\', "/")
        )
    );

    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"allowJs":true},"files":["main.js"]}"#,
    );
    let allowed = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    assert!(allowed.diagnostics.is_empty(), "{:?}", allowed.diagnostics);
    assert_eq!(allowed.inputs.len(), 1);

    write(
        root,
        "tsconfig.json",
        r#"{"include":["**/*"],"exclude":["ignored"]}"#,
    );
    let wildcard = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    assert!(
        wildcard.diagnostics.is_empty(),
        "{:?}",
        wildcard.diagnostics
    );
    assert_eq!(relative_paths(root, &wildcard.root_files), ["kept.ts"]);
}

#[test]
fn direct_roots_use_command_line_reason_chains() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "main.js", "const value = 1;\n");
    let host = SystemHost::new(root);
    let unsupported = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Files(vec![PathBuf::from("main.js")])),
    );

    assert_eq!(codes(&unsupported), [6504, 6504]);
    assert_eq!(unsupported.inputs.len(), 0);
    assert!(unsupported.diagnostics.iter().all(|diagnostic| {
        diagnostic.related_information.len() == 2
            && diagnostic.related_information[1].code == 1427
            && diagnostic.related_information[1].depth == 2
    }));

    let missing = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Files(vec![PathBuf::from("missing.ts")])),
    );
    assert_eq!(codes(&missing), [6053]);
    assert_eq!(
        missing.diagnostics[0].message_text,
        "File 'missing.ts' not found."
    );
    assert_eq!(missing.diagnostics[0].related_information[1].code, 1427);
}

#[test]
fn default_output_excludes_and_allow_js_affect_only_discovery() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"outDir":"generated","allowJs":true}}"#,
    );
    write(root, "source.ts", "export {};\n");
    write(root, "source.js", "exports.value = 1;\n");
    write(root, "lone.js", "exports.value = 1;\n");
    write(root, "generated/output.ts", "export {};\n");
    let host = SystemHost::new(root);

    let configured = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    assert_eq!(
        relative_paths(root, &configured.root_files),
        ["lone.js", "source.ts"]
    );

    let mut request = ProjectRequest::new(ProjectSelection::Project(root.to_path_buf()));
    request.overrides.allow_js = Some(false);
    let overridden = resolve_project(&host, &request);
    assert_eq!(relative_paths(root, &overridden.root_files), ["source.ts"]);
}

#[test]
fn command_line_output_directories_affect_default_discovery() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "tsconfig.json", r#"{}"#);
    write(root, "source.ts", "export {};\n");
    write(root, "dist/old.ts", "export {};\n");
    write(root, "types/old.ts", "export {};\n");
    let host = SystemHost::new(root);
    let mut request = ProjectRequest::new(ProjectSelection::Project(root.to_path_buf()));
    request.overrides.out_dir = Some("dist".into());
    request.overrides.declaration_dir = Some("types".into());
    let project = resolve_project(&host, &request);

    assert!(project.diagnostics.is_empty(), "{:?}", project.diagnostics);
    assert_eq!(relative_paths(root, &project.root_files), ["source.ts"]);
}

#[test]
fn extension_priority_respects_host_path_case_semantics() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"allowJs":true}}"#,
    );
    write(root, "X.ts", "export {};\n");
    write(root, "x.js", "exports.value = 1;\n");
    let host = SystemHost::new(root);
    let project = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );

    let paths = relative_paths(root, &project.root_files);
    if host.use_case_sensitive_file_names() {
        assert_eq!(paths, ["X.ts", "x.js"]);
    } else {
        assert_eq!(paths, ["X.ts"]);
    }
}

#[test]
fn project_emit_preserves_nested_paths_in_both_output_directories() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
            "compilerOptions": {
                "rootDir": "src",
                "outDir": "dist",
                "declaration": true,
                "declarationDir": "types"
            },
            "files": ["src/one/index.ts", "src/two/index.ts"]
        }"#,
    );
    write(root, "src/one/index.ts", "export const one: number = 1;\n");
    write(root, "src/two/index.ts", "export const two: number = 2;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();

    let output = Compiler::new().compile_resolved(resolved, &options);
    let paths: Vec<_> = output
        .emitted_files
        .iter()
        .map(|file| {
            file.path
                .strip_prefix(root)
                .expect("project output belongs to fixture")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();

    assert_eq!(
        paths,
        [
            "dist/one/index.js",
            "dist/two/index.js",
            "types/one/index.d.ts",
            "types/two/index.d.ts",
        ]
    );
    assert!(output.emitted_files[0].text.contains("const one = 1;"));
    assert!(output.emitted_files[1].text.contains("const two = 2;"));
}

#[test]
fn inferred_project_emit_root_reports_ts5011_but_keeps_config_relative_layout() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
            "compilerOptions": { "outDir": "dist" },
            "files": ["src/value.ts"]
        }"#,
    );
    write(root, "src/value.ts", "export const value = 1;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();

    let output = Compiler::new().compile_resolved(resolved, &options);

    let diagnostic = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 5011)
        .expect("TS5011");
    assert_eq!(
        diagnostic.message_text,
        concat!(
            "The common source directory of 'tsconfig.json' is './src'. ",
            "The 'rootDir' setting must be explicitly set to this or another path to ",
            "adjust your output's file layout.\n  Visit https://aka.ms/ts6 for migration ",
            "information."
        )
    );
    assert_eq!(
        relative_paths(
            root,
            &output
                .emitted_files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
        ),
        ["dist/src/value.js"]
    );
}

#[test]
fn no_emit_suppresses_output_layout_diagnostics_and_no_emit_on_error_blocks_writes() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "src/value.ts", "export const value = 1;\n");
    let host = SystemHost::new(root);

    write(
        root,
        "tsconfig.json",
        r#"{
            "compilerOptions": { "outDir": "dist", "noEmit": true },
            "files": ["src/value.ts"]
        }"#,
    );
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();
    let no_emit = Compiler::new().compile_resolved(resolved, &options);
    assert!(no_emit.diagnostics.is_empty(), "{:?}", no_emit.diagnostics);
    assert!(no_emit.emitted_files.is_empty());

    write(
        root,
        "tsconfig.json",
        r#"{
            "compilerOptions": { "outDir": "dist", "noEmitOnError": true },
            "files": ["src/value.ts"]
        }"#,
    );
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();
    let no_emit_on_error = Compiler::new().compile_resolved(resolved, &options);
    assert_eq!(
        no_emit_on_error
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [5011]
    );
    assert!(no_emit_on_error.emitted_files.is_empty());
}

#[test]
fn emit_preflight_blocks_only_the_product_that_would_overwrite_an_input() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
            "compilerOptions": { "allowJs": true, "declaration": true },
            "files": ["input.js"]
        }"#,
    );
    write(root, "input.js", "const value = 1;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();

    let output = Compiler::new().compile_resolved(resolved, &options);

    let diagnostic = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 5055)
        .expect("TS5055");
    assert_eq!(
        diagnostic.message_text,
        format!(
            "Cannot write file '{}' because it would overwrite input file.",
            root.join("input.js").to_string_lossy()
        )
    );
    assert_eq!(
        relative_paths(
            root,
            &output
                .emitted_files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
        ),
        ["input.d.ts"]
    );
    assert_eq!(
        output.exit_status,
        tsz::CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn syntax_aggregates_with_overwrite_preflight_when_emit_is_demanded() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"allowJs":true},"files":["syntax.ts","input.js"]}"#,
    );
    write(root, "syntax.ts", "const broken = ;");
    write(root, "input.js", "const value = 1;\n");
    let host = SystemHost::new(root);
    let compile = |no_emit, no_emit_on_error| {
        let resolved = resolve_project(
            &host,
            &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
        );
        let mut options = resolved.options.clone();
        options.no_emit = no_emit;
        options.no_emit_on_error = no_emit_on_error;
        Compiler::new().compile_resolved(resolved, &options)
    };

    for (no_emit, no_emit_on_error, expected) in [
        (false, false, &[5055, 1109][..]),
        (false, true, &[5055, 1109][..]),
        (true, false, &[1109][..]),
    ] {
        let output = compile(no_emit, no_emit_on_error);
        assert_eq!(
            output_codes(&output),
            expected,
            "no_emit={no_emit}, no_emit_on_error={no_emit_on_error}",
        );
        if no_emit || no_emit_on_error {
            assert!(output.emitted_files.is_empty());
        }
    }
    write(root, "syntax.ts", "const missing: number;");
    assert_eq!(output_codes(&compile(false, false)), [5055, 1155]);
}

#[test]
fn syntax_aggregates_with_collision_and_root_dir_program_diagnostics() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "syntax.ts", "const broken = ;");
    write(root, "same.ts", "export const from_ts = 1;\n");
    write(root, "same.tsx", "export const from_tsx = 2;\n");
    let host = SystemHost::new(root);
    let compile = || {
        let resolved = resolve_project(
            &host,
            &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
        );
        let options = resolved.options.clone();
        Compiler::new().compile_resolved(resolved, &options)
    };

    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"rootDir":".","outDir":"dist"},"files":["syntax.ts","same.ts","same.tsx"]}"#,
    );
    let collision = compile();
    assert_eq!(output_codes(&collision), [5056, 1109]);
    assert!(
        collision
            .emitted_files
            .iter()
            .all(|file| file.path != root.join("dist/same.js"))
    );

    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"rootDir":"src","outDir":"dist"},"files":["syntax.ts"]}"#,
    );
    let root_dir = compile();
    assert_eq!(output_codes(&root_dir), [6059, 1109]);
}

#[test]
fn unsupported_map_options_withhold_products_before_collision_preflight() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
            "compilerOptions": {
                "declaration": true,
                "declarationMap": true,
                "sourceMap": true,
                "jsx": "react"
            },
            "files": ["same.ts", "same.tsx", "other.ts"]
        }"#,
    );
    write(root, "same.ts", "export const fromTs = 1;\n");
    write(root, "same.tsx", "export const fromTsx = 2;\n");
    write(root, "other.ts", "export const other = 3;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();

    let output = Compiler::new().compile_resolved(resolved, &options);

    let collision_messages: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 5056)
        .map(|diagnostic| diagnostic.message_text.clone())
        .collect();
    assert!(collision_messages.is_empty(), "{collision_messages:?}");
    assert_eq!(
        relative_paths(
            root,
            &output
                .emitted_files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
        ),
        Vec::<String>::new()
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn explicit_root_dir_reports_ts6059_and_preserves_partial_emit() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
            "compilerOptions": { "rootDir": "src", "outDir": "dist" },
            "files": ["src/a.ts", "outside/b.ts"]
        }"#,
    );
    write(root, "src/a.ts", "export const a = 1;\n");
    write(root, "outside/b.ts", "export const b = 2;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();

    let output = Compiler::new().compile_resolved(resolved, &options);

    let diagnostic = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 6059)
        .expect("TS6059");
    assert_eq!(
        diagnostic.message_text,
        format!(
            "File '{}' is not under 'rootDir' '{}'. 'rootDir' is expected to contain all source files.",
            root.join("outside/b.ts").to_string_lossy(),
            root.join("src").to_string_lossy()
        )
    );
    assert_eq!(
        diagnostic
            .related_information
            .iter()
            .map(|related| (related.code, related.depth, related.message_text.as_str()))
            .collect::<Vec<_>>(),
        [
            (1411, 1, "The file is in the program because:"),
            (1409, 2, "Part of 'files' list in tsconfig.json"),
        ]
    );
    assert_eq!(
        relative_paths(
            root,
            &output
                .emitted_files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
        ),
        ["dist/a.js", "outside/b.js"]
    );
    assert_eq!(
        output.exit_status,
        tsz::CompileExitStatus::DiagnosticsPresentOutputsGenerated
    );
}

#[test]
fn explicit_root_dir_is_checked_under_no_emit_and_no_emit_on_error() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(
        root,
        "tsconfig.json",
        r#"{
            "compilerOptions": { "rootDir": "src", "outDir": "dist" },
            "files": ["outside.ts"]
        }"#,
    );
    write(root, "outside.ts", "export const outside = 1;\n");
    let host = SystemHost::new(root);

    for option in ["noEmit", "noEmitOnError"] {
        let resolved = resolve_project(
            &host,
            &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
        );
        let mut options = resolved.options.clone();
        if option == "noEmit" {
            options.no_emit = true;
        } else {
            options.no_emit_on_error = true;
        }
        let output = Compiler::new().compile_resolved(resolved, &options);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            [6059]
        );
        assert!(output.emitted_files.is_empty());
        assert_eq!(
            output.exit_status,
            tsz::CompileExitStatus::DiagnosticsPresentOutputsSkipped
        );
    }
}

#[test]
fn config_option_origins_locate_ts5011_and_invalid_target() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    let config = concat!(
        "{\n",
        "  \"files\": [\"src/value.ts\"],\n",
        "  \"compilerOptions\": {\n",
        "    \"outDir\": \"dist\",\n",
        "    \"target\": \"wat\"\n",
        "  }\n",
        "}\n",
    );
    write(root, "tsconfig.json", config);
    write(root, "src/value.ts", "export const value = 1;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();

    let output = Compiler::new().compile_resolved(resolved, &options);

    let target = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 6046)
        .expect("TS6046");
    assert_eq!(target.file, "tsconfig.json");
    assert_eq!(target.start, config.find("\"wat\"").unwrap() as u32);
    assert_eq!(target.length, 5);
    assert!(
        target
            .render(None)
            .starts_with("tsconfig.json(5,15): error TS6046:"),
        "{}",
        target.render(None)
    );

    let layout = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 5011)
        .expect("TS5011");
    assert_eq!(layout.file, "tsconfig.json");
    assert_eq!(layout.start, config.find("\"outDir\"").unwrap() as u32);
    assert_eq!(layout.length, 8);
    assert!(
        layout
            .render(None)
            .starts_with("tsconfig.json(4,5): error TS5011:"),
        "{}",
        layout.render(None)
    );
    assert_eq!(
        relative_paths(
            root,
            &output
                .emitted_files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
        ),
        ["dist/src/value.js"]
    );
    assert_eq!(
        output.exit_status,
        tsz::CompileExitStatus::DiagnosticsPresentOutputsGenerated
    );
}

#[test]
fn program_option_diagnostics_use_entry_config_syntax() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "case.ts", "const missing: number;\n");
    let compile = |patch: CompilerOptionPatch| {
        let host = SystemHost::new(root);
        let mut resolved = resolve_project(
            &host,
            &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
        );
        let options = resolved.apply_option_patch(&patch);
        Compiler::new().compile_resolved(resolved, &options)
    };
    let location = |output: &tsz::CompileOutput| {
        let diagnostic = output
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == 5108)
            .expect("TS5108");
        (diagnostic.file.clone(), diagnostic.start, diagnostic.length)
    };

    let own = r#"{"compilerOptions":{"target":"es5","noEmitOnError":false},"files":["case.ts"]}"#;
    write(root, "tsconfig.json", own);
    let output = compile(CompilerOptionPatch::default());
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [5108]
    );
    assert_eq!(output.stats.types, 0);
    assert!(output.emitted_files.is_empty());
    assert_eq!(
        location(&output),
        (
            "tsconfig.json".to_string(),
            own.find("\"es5\"").unwrap() as u32,
            5,
        )
    );

    let overridden = r#"{"compilerOptions":{"target":"es2022","noEmit":true},"files":["case.ts"]}"#;
    write(root, "tsconfig.json", overridden);
    let output = compile(CompilerOptionPatch {
        target: Some("es5".to_string()),
        ..CompilerOptionPatch::default()
    });
    assert_eq!(
        location(&output),
        (
            "tsconfig.json".to_string(),
            overridden.find("\"es2022\"").unwrap() as u32,
            8,
        )
    );

    let fallback = r#"{"compilerOptions":{"noEmit":true},"files":["case.ts"]}"#;
    write(root, "tsconfig.json", fallback);
    let output = compile(CompilerOptionPatch {
        target: Some("es5".to_string()),
        ..CompilerOptionPatch::default()
    });
    assert_eq!(
        location(&output),
        (
            "tsconfig.json".to_string(),
            fallback.find("\"compilerOptions\"").unwrap() as u32,
            17,
        )
    );

    let non_string = r#"{"compilerOptions":{"target":123,"noEmit":true},"files":["case.ts"]}"#;
    write(root, "tsconfig.json", non_string);
    let output = compile(CompilerOptionPatch {
        target: Some("es5".to_string()),
        ..CompilerOptionPatch::default()
    });
    assert_eq!(
        location(&output),
        (
            "tsconfig.json".to_string(),
            non_string.find("123").unwrap() as u32,
            3,
        )
    );

    let non_object = r#"{"compilerOptions":null,"files":["case.ts"]}"#;
    write(root, "tsconfig.json", non_object);
    let output = compile(CompilerOptionPatch {
        target: Some("es5".to_string()),
        ..CompilerOptionPatch::default()
    });
    assert_eq!(
        location(&output),
        (
            "tsconfig.json".to_string(),
            non_object.find("\"compilerOptions\"").unwrap() as u32,
            17,
        )
    );

    write(root, "base.json", r#"{"compilerOptions":{"target":"es5"}}"#);
    let inherited =
        r#"{"extends":"./base.json","compilerOptions":{"noEmit":true},"files":["case.ts"]}"#;
    write(root, "tsconfig.json", inherited);
    let output = compile(CompilerOptionPatch::default());
    assert_eq!(
        location(&output),
        (
            "tsconfig.json".to_string(),
            inherited.find("\"compilerOptions\"").unwrap() as u32,
            17,
        )
    );

    let valid = r#"{"compilerOptions":{"target":"es2022","noEmit":true},"files":["case.ts"]}"#;
    write(root, "tsconfig.json", valid);
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let mut options = resolved.options.clone();
    options.target = "renamed-invalid".to_string();
    let output = Compiler::new().compile_resolved(resolved, &options);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [6046]
    );
    assert!(output.diagnostics[0].file.is_empty());
}

#[test]
fn config_target_parse_diagnostics_preserve_duplicate_occurrences() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    write(root, "case.ts", "const missing: number;\n");
    let compile = |config: &str| {
        write(root, "tsconfig.json", config);
        let host = SystemHost::new(root);
        let resolved = resolve_project(
            &host,
            &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
        );
        let options = resolved.options.clone();
        Compiler::new().compile_resolved(resolved, &options)
    };
    let starts = |output: &tsz::CompileOutput| {
        output
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == 6046)
            .map(|diagnostic| diagnostic.start)
            .collect::<Vec<_>>()
    };

    let invalid_then_valid = r#"{"compilerOptions":{"target":"wat","target":"es2022","noEmit":true},"files":["case.ts"]}"#;
    assert_eq!(
        starts(&compile(invalid_then_valid)),
        [invalid_then_valid.find("\"wat\"").unwrap() as u32]
    );

    let valid_then_invalid = r#"{"compilerOptions":{"target":"es2022","target":"wat","noEmit":true},"files":["case.ts"]}"#;
    assert_eq!(
        starts(&compile(valid_then_invalid)),
        [valid_then_invalid.find("\"wat\"").unwrap() as u32]
    );

    let two_invalid = r#"{"compilerOptions":{"target":"wat","target":"future","noEmit":true},"files":["case.ts"]}"#;
    assert_eq!(
        starts(&compile(two_invalid)),
        [
            two_invalid.find("\"wat\"").unwrap() as u32,
            two_invalid.find("\"future\"").unwrap() as u32,
        ]
    );
}

#[test]
fn inherited_option_origins_follow_ts7_diagnostic_ownership() {
    let fixture = TempDir::new().expect("tempdir");
    let root = fixture.path();
    let base = concat!(
        "{\n",
        "  \"compilerOptions\": {\n",
        "    \"outDir\": \"dist\",\n",
        "    \"target\": \"wat\"\n",
        "  }\n",
        "}\n",
    );
    write(root, "base.json", base);
    write(
        root,
        "tsconfig.json",
        r#"{ "extends": "./base.json", "files": ["src/value.ts"] }"#,
    );
    write(root, "src/value.ts", "export const value = 1;\n");
    let host = SystemHost::new(root);
    let resolved = resolve_project(
        &host,
        &ProjectRequest::new(ProjectSelection::Project(root.to_path_buf())),
    );
    let options = resolved.options.clone();

    let output = Compiler::new().compile_resolved(resolved, &options);

    let target = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 6046)
        .expect("TS6046");
    assert_eq!(target.file, "base.json");
    assert_eq!(target.start, base.find("\"wat\"").unwrap() as u32);
    assert_eq!(target.length, 5);
    assert!(
        target
            .render(None)
            .starts_with("base.json(4,15): error TS6046:")
    );

    let layout = output
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 5011)
        .expect("TS5011");
    assert!(layout.file.is_empty());
    assert!(layout.render(None).starts_with("error TS5011:"));
}

#[test]
fn commonjs_overloads_wait_for_hoisted_export_product_ownership() {
    for source in [
        "export function select(value:string):string;export function select(value:any):any{return value}",
        "export class Vessel{method(value:string):string;method(value:any):any{return value}}",
        "export class Vessel{method():void;}",
        "export class Vessel{constructor();}",
        "class Vessel{method():void;}export{Vessel};",
        "export{Vessel};class Vessel{method():void;}",
        "class Vessel{method():void;}export{Vessel as Ship};",
    ] {
        for (path, module) in [
            ("overloads.ts", "commonjs"),
            ("overloads.cts", "node16"),
            ("overloads.cts", "nodenext"),
        ] {
            for no_check in [false, true] {
                let output = Compiler::new().compile(
                    vec![SourceInput::new(path, Arc::<str>::from(source))],
                    &CompilerOptions {
                        declaration: true,
                        no_check,
                        target: "esnext".to_string(),
                        module: module.to_string(),
                        ..CompilerOptions::default()
                    },
                );
                assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
                assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
                assert!(
                    output.emitted_files.is_empty(),
                    "{source}/{module}/{no_check}"
                );
            }
        }
    }
}
