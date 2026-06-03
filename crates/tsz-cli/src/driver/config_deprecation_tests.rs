use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tempfile::TempDir;

fn config_deprecation_dependency_fixture() -> (TempDir, CliArgs) {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::create_dir_all(dir.path().join("node_modules/pkg")).expect("create package dir");
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "module": "es2022",
    "moduleResolution": "node",
    "allowJs": true,
    "strict": true
  },
  "files": ["index.js"]
}"#,
    )
    .expect("write tsconfig");
    fs::write(dir.path().join("index.js"), "import \"pkg\";\n").expect("write index");
    fs::write(
        dir.path().join("node_modules/pkg/package.json"),
        r#"{ "name": "pkg", "version": "1.0.0", "types": "index.d.ts" }"#,
    )
    .expect("write package json");
    fs::write(
        dir.path().join("node_modules/pkg/index.d.ts"),
        "declare module Legacy.Namespace { export interface Item { value: string } }\n",
    )
    .expect("write package d.ts");

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("project args");
    (dir, args)
}

#[test]
fn normal_no_emit_config_deprecation_stays_on_full_program_path() {
    let (dir, args) = config_deprecation_dependency_fixture();
    let result = super::config_deprecation::with_try_tsz_worker_config_deprecation(false, || {
        compile(&args, dir.path())
    })
    .expect("compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(codes.contains(&5107), "expected TS5107, got: {codes:?}");
    assert!(
        result.phase_timings.load_libs_ms > 0.0,
        "normal CLI should keep the full-program path outside try-tsz worker mode, got timings: {:?}",
        result.phase_timings
    );
}

#[test]
fn try_tsz_worker_no_emit_config_deprecation_skips_dependency_semantic_diagnostics() {
    let (dir, args) = config_deprecation_dependency_fixture();
    let result = super::config_deprecation::with_try_tsz_worker_config_deprecation(true, || {
        compile(&args, dir.path())
    })
    .expect("compile succeeds");
    let codes: Vec<u32> = result.diagnostics.iter().map(|d| d.code).collect();

    assert!(codes.contains(&5107), "expected TS5107, got: {codes:?}");
    assert!(
        !codes.contains(&1540),
        "try-tsz worker deprecation path should skip dependency semantic diagnostics, got: {:?}",
        result.diagnostics
    );
}
