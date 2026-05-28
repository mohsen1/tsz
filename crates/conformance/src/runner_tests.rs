use super::*;
use crate::tsc_results::{DiagnosticFingerprint, FileMetadata, TscResult};
use std::sync::{Mutex, OnceLock};

fn fp(code: u32, file: &str, msg: &str) -> DiagnosticFingerprint {
    DiagnosticFingerprint {
        code,
        file: file.to_string(),
        line: 1,
        column: 1,
        message_key: msg.to_string(),
    }
}

fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn appledouble_files_are_not_discoverable_tests() {
    assert!(is_appledouble_file(Path::new(
        "TypeScript/tests/cases/._foo.ts"
    )));
    assert!(is_appledouble_file(Path::new("._bar.js")));
    assert!(!is_appledouble_file(Path::new("foo.ts")));
    assert!(!is_appledouble_file(Path::new("dir/regular.js")));
}

#[test]
fn result_timings_override_stale_path_weights() {
    let file = tempfile::NamedTempFile::new().expect("weights file");
    std::fs::write(
        file.path(),
        serde_json::json!({
            "path_weights": {
                "TypeScript/tests/cases/compiler/foo.ts": 10_000.0
            },
            "results": [{
                "file": "TypeScript/tests/cases/compiler/foo.ts",
                "elapsed_ms": 25.0
            }]
        })
        .to_string(),
    )
    .expect("write weights");

    let weights = load_json_weights(file.path()).expect("weights should load");
    let path = Path::new("/repo/TypeScript/tests/cases/compiler/foo.ts");
    let test_dir = Path::new("/repo/TypeScript/tests/cases");

    assert_eq!(historical_path_weight(&weights, path, test_dir), Some(25.0));
}

fn with_temp_cwd<F, T>(create_fast_binary: bool, f: F) -> T
where
    F: FnOnce(&Path) -> T,
{
    let _guard = cwd_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let original = std::env::current_dir().expect("current dir should be readable");
    let temp = std::env::temp_dir().join(format!(
        "tsz_runner_helper_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp).expect("temp dir should be created");

    if create_fast_binary {
        let fast_binary = temp.join(".target/dist-fast/tsz");
        if let Some(parent) = fast_binary.parent() {
            std::fs::create_dir_all(parent).expect("parent dir should be created");
        }
        std::fs::write(&fast_binary, b"tsz").expect("fast binary should be created");
    }

    std::env::set_current_dir(&temp).expect("cwd should change");
    let result = f(&temp);
    std::env::set_current_dir(original).expect("cwd should be restored");
    let _ = std::fs::remove_dir_all(&temp);
    result
}

#[test]
fn use_fingerprint_compare_requires_both_sides_non_empty() {
    let tsc: std::collections::HashSet<DiagnosticFingerprint> =
        [fp(2322, "a.ts", "mismatch")].into_iter().collect();
    let tsz_empty: std::collections::HashSet<DiagnosticFingerprint> =
        std::collections::HashSet::new();
    let tsz_populated: std::collections::HashSet<DiagnosticFingerprint> =
        [fp(2322, "a.ts", "mismatch")].into_iter().collect();

    // Server mode: TSC has fingerprints, tsz doesn't — must NOT compare,
    // otherwise every test would spuriously fail with all tsc
    // fingerprints reported as missing.
    assert!(!use_fingerprint_compare(&tsc, &tsz_empty));
    // Symmetric: tsz has fingerprints, TSC doesn't (cache missed them) —
    // also fall back to code-only.
    assert!(!use_fingerprint_compare(
        &std::collections::HashSet::new(),
        &tsz_populated
    ));
    // Both empty: no fingerprint data anywhere, compare by codes only.
    assert!(!use_fingerprint_compare(
        &std::collections::HashSet::new(),
        &std::collections::HashSet::new()
    ));
    // CLI mode: both sides populated — enable fingerprint compare.
    assert!(use_fingerprint_compare(&tsc, &tsz_populated));
}

#[test]
fn config_diagnostic_filters_include_removed_compiler_options() {
    assert!(is_project_config_diagnostic_code(5102));
    assert!(is_compiler_option_config_diagnostic_code(5102));
    assert!(is_compiler_option_config_diagnostic_code(5101));
    assert!(is_compiler_option_config_diagnostic_code(5107));
    assert!(!is_compiler_option_config_diagnostic_code(2322));
}

#[test]
fn is_lib_diagnostic_detects_lib_files() {
    assert!(is_lib_diagnostic(&fp(
        2430,
        ".lib/react16.d.ts",
        "Interface 'X' incorrectly extends 'Y'."
    )));
    assert!(is_lib_diagnostic(&fp(
        6053,
        "test.tsx",
        "File '/.lib/react.d.ts' not found."
    )));
    assert!(!is_lib_diagnostic(&fp(
        2344,
        "scripts/node_modules/typescript/lib/lib.dom.d.ts",
        "Type 'HTMLElementTagNameMap[K]' does not satisfy the constraint 'Element'."
    )));
    assert!(!is_lib_diagnostic(&fp(
        2344,
        "TypeScript/lib/lib.dom.d.ts",
        "Type 'HTMLElementTagNameMap[K]' does not satisfy the constraint 'Element'."
    )));
    assert!(!is_lib_diagnostic(&fp(
        2322,
        "test.ts",
        "Type 'A' is not assignable to type 'B'."
    )));
}

#[test]
fn filter_tsz_removes_lib_only_codes() {
    let result = tsz_wrapper::CompilationResult {
        error_codes: vec![2430, 2322],
        diagnostic_fingerprints: vec![
            fp(2430, ".lib/react16.d.ts", "Interface error"),
            fp(2322, "test.ts", "Type mismatch"),
        ],
        crashed: false,
        options: Default::default(),
    };
    let filtered = filter_lib_diagnostics_tsz(result);
    assert_eq!(filtered.error_codes, vec![2322]);
    assert_eq!(filtered.diagnostic_fingerprints.len(), 1);
    assert_eq!(filtered.diagnostic_fingerprints[0].code, 2322);
}

#[test]
fn filter_tsz_preserves_builtin_lib_only_codes_for_later_comparison_filter() {
    let result = tsz_wrapper::CompilationResult {
        error_codes: vec![2344, 2304],
        diagnostic_fingerprints: vec![
            fp(
                2344,
                "TypeScript/lib/lib.dom.d.ts",
                "Type 'HTMLElementTagNameMap[K]' does not satisfy the constraint 'Element'.",
            ),
            fp(2304, "test.ts", "Cannot find name 'missing'."),
        ],
        crashed: false,
        options: Default::default(),
    };
    let filtered = filter_lib_diagnostics_tsz(result);
    assert_eq!(filtered.error_codes, vec![2344, 2304]);
    assert_eq!(filtered.diagnostic_fingerprints.len(), 2);
}

#[test]
fn filter_tsz_removes_extra_builtin_lib_only_codes() {
    let result = tsz_wrapper::CompilationResult {
        error_codes: vec![2344, 2304],
        diagnostic_fingerprints: vec![
            fp(
                2344,
                "TypeScript/lib/lib.dom.d.ts",
                "Type 'HTMLElementTagNameMap[K]' does not satisfy the constraint 'Element'.",
            ),
            fp(2304, "test.ts", "Cannot find name 'missing'."),
        ],
        crashed: false,
        options: Default::default(),
    };
    let filtered = filter_extra_typescript_builtin_lib_diagnostics_tsz(result, &[]);
    assert_eq!(filtered.error_codes, vec![2304]);
    assert_eq!(filtered.diagnostic_fingerprints.len(), 1);
    assert_eq!(filtered.diagnostic_fingerprints[0].code, 2304);
}

#[test]
fn filter_tsz_preserves_builtin_lib_code_expected_by_tsc() {
    let result = tsz_wrapper::CompilationResult {
        error_codes: vec![2344],
        diagnostic_fingerprints: vec![fp(
            2344,
            "TypeScript/lib/lib.dom.d.ts",
            "Type 'HTMLElementTagNameMap[K]' does not satisfy the constraint 'Element'.",
        )],
        crashed: false,
        options: Default::default(),
    };
    let tsc_fps = vec![fp(
        2344,
        "lib.dom.d.ts",
        "Type 'HTMLElementTagNameMap[K]' does not satisfy the constraint 'Element'.",
    )];
    let filtered = filter_extra_typescript_builtin_lib_diagnostics_tsz(result, &tsc_fps);
    assert_eq!(filtered.error_codes, vec![2344]);
    assert_eq!(filtered.diagnostic_fingerprints.len(), 1);
}

#[test]
fn filter_tsz_preserves_code_appearing_in_both_lib_and_non_lib() {
    let result = tsz_wrapper::CompilationResult {
        error_codes: vec![2430],
        diagnostic_fingerprints: vec![
            fp(2430, ".lib/react16.d.ts", "Interface error in lib"),
            fp(2430, "test.ts", "Interface error in user code"),
        ],
        crashed: false,
        options: Default::default(),
    };
    let filtered = filter_lib_diagnostics_tsz(result);
    assert_eq!(filtered.error_codes, vec![2430]);
    assert_eq!(filtered.diagnostic_fingerprints.len(), 1);
    assert_eq!(filtered.diagnostic_fingerprints[0].file, "test.ts");
}

#[test]
fn filter_tsz_noop_when_no_lib_diagnostics() {
    let result = tsz_wrapper::CompilationResult {
        error_codes: vec![2322, 2345],
        diagnostic_fingerprints: vec![
            fp(2322, "test.ts", "Type mismatch"),
            fp(2345, "test.ts", "Arg type error"),
        ],
        crashed: false,
        options: Default::default(),
    };
    let filtered = filter_lib_diagnostics_tsz(result);
    assert_eq!(filtered.error_codes, vec![2322, 2345]);
    assert_eq!(filtered.diagnostic_fingerprints.len(), 2);
}

#[test]
fn filter_tsc_removes_lib_6053() {
    let tsc_result = TscResult {
        metadata: FileMetadata {
            mtime_ms: 0,
            size: 0,
            typescript_version: None,
        },
        error_codes: vec![6053, 2322],
        diagnostic_fingerprints: vec![
            fp(6053, "test.tsx", "File '/.lib/react16.d.ts' not found."),
            fp(2322, "test.ts", "Type mismatch"),
        ],
    };
    let (codes, fps) = filter_lib_diagnostics_tsc(&tsc_result);
    assert_eq!(codes, vec![2322]);
    assert_eq!(fps.len(), 1);
    assert_eq!(fps[0].code, 2322);
}

#[test]
fn filter_tsc_preserves_6053_from_non_lib() {
    let tsc_result = TscResult {
        metadata: FileMetadata {
            mtime_ms: 0,
            size: 0,
            typescript_version: None,
        },
        error_codes: vec![6053],
        diagnostic_fingerprints: vec![
            fp(6053, "test.tsx", "File '/.lib/react16.d.ts' not found."),
            fp(6053, "test.ts", "File 'missing.d.ts' not found."),
        ],
    };
    let (codes, fps) = filter_lib_diagnostics_tsc(&tsc_result);
    assert_eq!(codes, vec![6053]);
    assert_eq!(fps.len(), 1);
    assert_eq!(fps[0].message_key, "File 'missing.d.ts' not found.");
}

#[test]
fn filter_tsz_removes_6053_with_lib_in_message() {
    let result = tsz_wrapper::CompilationResult {
        error_codes: vec![6053],
        diagnostic_fingerprints: vec![fp(6053, "test.tsx", "File '/.lib/react.d.ts' not found.")],
        crashed: false,
        options: Default::default(),
    };
    let filtered = filter_lib_diagnostics_tsz(result);
    assert!(filtered.error_codes.is_empty());
    assert!(filtered.diagnostic_fingerprints.is_empty());
}

#[test]
fn relative_display_returns_relative_path_when_possible() {
    let base = Path::new("/repo/project");
    let path = Path::new("/repo/project/tests/case.ts");
    assert_eq!(relative_display(path, base), "tests/case.ts");
}

#[test]
fn relative_display_falls_back_to_absolute_path_when_outside_base() {
    let base = Path::new("/repo/project");
    let path = Path::new("/other/place/case.ts");
    assert_eq!(relative_display(path, base), "/other/place/case.ts");
}

#[test]
fn sanitize_artifact_name_replaces_filesystem_special_characters() {
    let sanitized = sanitize_artifact_name(r#"a/b\c:d*e?f"g<h>i|j"#);
    assert_eq!(sanitized, "a_b_c_d_e_f_g_h_i_j");
}

#[test]
fn resolve_tsz_binary_prefers_local_fast_binary_when_present() {
    with_temp_cwd(true, |temp| {
        let resolved = Runner::resolve_tsz_binary("tsz");
        assert_eq!(
            resolved,
            std::fs::canonicalize(temp.join(".target/dist-fast/tsz"))
                .expect("fast binary path should canonicalize")
                .to_string_lossy()
                .to_string()
        );
        assert!(temp.join(".target/dist-fast/tsz").is_file());
    });
}

#[test]
fn resolve_tsz_binary_preserves_configured_binary_when_not_default() {
    with_temp_cwd(false, |_| {
        let resolved = Runner::resolve_tsz_binary("/usr/local/bin/tsz-custom");
        assert_eq!(resolved, "/usr/local/bin/tsz-custom");
    });
}

#[test]
fn resolve_tsz_binary_absolutizes_relative_configured_path() {
    with_temp_cwd(false, |temp| {
        let rel = Path::new("bin/tsz-custom");
        std::fs::create_dir_all(temp.join("bin")).expect("bin dir should exist");
        std::fs::write(temp.join(rel), b"").expect("binary placeholder should exist");

        let resolved = Runner::resolve_tsz_binary("bin/tsz-custom");
        assert_eq!(
            resolved,
            std::fs::canonicalize(temp.join(rel))
                .expect("configured binary path should canonicalize")
                .to_string_lossy()
                .to_string()
        );
    });
}

fn compilation(codes: &[u32], fps: Vec<DiagnosticFingerprint>) -> tsz_wrapper::CompilationResult {
    tsz_wrapper::CompilationResult {
        error_codes: codes.to_vec(),
        diagnostic_fingerprints: fps,
        crashed: false,
        options: HashMap::new(),
    }
}

fn assert_fail_codes(
    result: &TestResult,
    expected_codes: &[u32],
    actual_codes: &[u32],
    missing_codes: &[u32],
    extra_codes: &[u32],
) {
    match result {
        TestResult::Fail(fail) => {
            assert_eq!(&fail.expected, expected_codes, "expected codes mismatch");
            assert_eq!(&fail.actual, actual_codes, "actual codes mismatch");
            let mut m = fail.missing.clone();
            m.sort_unstable();
            let mut e = fail.extra.clone();
            e.sort_unstable();
            let mut want_m = missing_codes.to_vec();
            want_m.sort_unstable();
            let mut want_e = extra_codes.to_vec();
            want_e.sort_unstable();
            assert_eq!(m, want_m, "missing codes mismatch");
            assert_eq!(e, want_e, "extra codes mismatch");
        }
        other => panic!("expected TestResult::Fail, got {other:?}"),
    }
}

#[test]
fn compare_diagnostics_passes_on_exact_match() {
    let tsc_codes = vec![2304];
    let tsc_fps = vec![fp(2304, "a.ts", "Cannot find name 'foo'.")];
    let compile = compilation(&[2304], vec![fp(2304, "a.ts", "Cannot find name 'foo'.")]);

    let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
    assert_eq!(result, TestResult::Pass);
}

#[test]
fn compare_diagnostics_detects_missing_code() {
    let tsc_codes = vec![2304, 2322];
    let tsc_fps: Vec<DiagnosticFingerprint> = vec![];
    let compile = compilation(&[2304], vec![]);

    let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
    assert_fail_codes(&result, &[2304, 2322], &[2304], &[2322], &[]);
}

#[test]
fn compare_diagnostics_detects_extra_code() {
    let tsc_codes = vec![2304];
    let tsc_fps: Vec<DiagnosticFingerprint> = vec![];
    let compile = compilation(&[2304, 7027], vec![]);

    let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
    assert_fail_codes(&result, &[2304], &[2304, 7027], &[], &[7027]);
}

#[test]
fn compare_diagnostics_skips_fingerprints_when_tsc_has_none() {
    // When the tsc cache carries no fingerprints, code-level parity is the
    // only thing that matters. Extra tsz fingerprints must not fail the run.
    let tsc_codes = vec![2304];
    let compile = compilation(
        &[2304],
        vec![fp(2304, "a.ts", "Cannot find name 'foo' on line 1.")],
    );

    let result = compare_diagnostics(&compile, &tsc_codes, &[], HashMap::new());
    assert_eq!(result, TestResult::Pass);
}

#[test]
fn compare_diagnostics_detects_fingerprint_only_mismatch() {
    // Codes match but fingerprints disagree (e.g. wrong file or message).
    // This is the "fingerprint-only failure" case that dominates the
    // close-to-passing bucket in the conformance dashboard.
    let tsc_codes = vec![2304];
    let tsc_fps = vec![fp(2304, "expected.ts", "Cannot find name 'foo'.")];
    let compile = compilation(
        &[2304],
        vec![fp(2304, "actual.ts", "Cannot find name 'foo'.")],
    );

    let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
    match result {
        TestResult::Fail(fail) => {
            assert!(
                fail.missing.is_empty() && fail.extra.is_empty(),
                "codes should match exactly"
            );
            assert_eq!(fail.missing_fingerprints.len(), 1);
            assert_eq!(fail.missing_fingerprints[0].file, "expected.ts");
            assert_eq!(fail.extra_fingerprints.len(), 1);
            assert_eq!(fail.extra_fingerprints[0].file, "actual.ts");
        }
        other => panic!("expected Fail with fingerprint diff, got {other:?}"),
    }
}

#[test]
fn compare_diagnostics_sorts_expected_and_actual() {
    // Callers rely on sorted `expected` and `actual` for stable failure
    // rendering and snapshot stability.
    let tsc_codes = vec![2345, 2304];
    let tsc_fps: Vec<DiagnosticFingerprint> = vec![];
    let compile = compilation(&[7027, 2304], vec![]);

    let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
    match result {
        TestResult::Fail(fail) => {
            assert_eq!(fail.expected, vec![2304, 2345]);
            assert_eq!(fail.actual, vec![2304, 7027]);
        }
        other => panic!("expected Fail, got {other:?}"),
    }
}

#[test]
fn compare_diagnostics_sorts_fingerprint_diffs() {
    // Fingerprint lists must be deterministic so failure output is stable.
    // tsz must carry at least one fingerprint to activate the fingerprint
    // comparison path (server-mode parity guard in `use_fingerprint_compare`).
    let tsc_codes = vec![2322, 2304];
    let tsc_fps = vec![
        fp(2322, "b.ts", "Type mismatch."),
        fp(2304, "a.ts", "Cannot find."),
    ];
    let compile = compilation(&[], vec![fp(9999, "z.ts", "sentinel")]);

    let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
    match result {
        TestResult::Fail(fail) => {
            // Sort key is (code, file, line, column, message_key).
            assert_eq!(
                fail.missing_fingerprints
                    .iter()
                    .map(|f| (f.code, f.file.clone()))
                    .collect::<Vec<_>>(),
                vec![(2304, "a.ts".into()), (2322, "b.ts".into())],
            );
        }
        other => panic!("expected Fail, got {other:?}"),
    }
}

#[test]
fn compare_diagnostics_carries_full_fingerprint_sets_on_fail() {
    let tsc_codes = vec![2304, 2322];
    let tsc_fps = vec![
        fp(2322, "b.ts", "Type mismatch."),
        fp(2304, "a.ts", "Cannot find."),
    ];
    let compile = compilation(
        &[2304, 2322],
        vec![
            fp(2322, "actual.ts", "Type mismatch."),
            fp(2304, "a.ts", "Cannot find."),
        ],
    );

    let result = compare_diagnostics(&compile, &tsc_codes, &tsc_fps, HashMap::new());
    match result {
        TestResult::Fail(fail) => {
            assert_eq!(
                fail.expected_fingerprints
                    .iter()
                    .map(|f| (f.code, f.file.as_str()))
                    .collect::<Vec<_>>(),
                vec![(2304, "a.ts"), (2322, "b.ts")],
            );
            assert_eq!(
                fail.actual_fingerprints
                    .iter()
                    .map(|f| (f.code, f.file.as_str()))
                    .collect::<Vec<_>>(),
                vec![(2304, "a.ts"), (2322, "actual.ts")],
            );
        }
        other => panic!("expected Fail, got {other:?}"),
    }
}

#[test]
fn compare_diagnostics_threads_options_into_fail() {
    let mut options = HashMap::new();
    options.insert("target".to_string(), "es2020".to_string());
    let tsc_codes = vec![2304];
    let compile = compilation(&[], vec![]);

    let result = compare_diagnostics(&compile, &tsc_codes, &[], options.clone());
    match result {
        TestResult::Fail(fail) => assert_eq!(fail.options, options),
        other => panic!("expected Fail, got {other:?}"),
    }
}
