#[test]
fn test_parse_batch_output_retains_bare_no_pos_diagnostics() {
    let root = std::path::Path::new("/tmp/tsz-test");
    let output = "error TS2468: Cannot find global value 'Promise'.";

    let result = parse_batch_output(output, root, HashMap::new());

    assert!(
        result.error_codes.is_empty(),
        "bare program-level diagnostics are compared as fingerprints, not code-list entries",
    );
    assert_eq!(result.diagnostic_fingerprints.len(), 1);
    let fp = &result.diagnostic_fingerprints[0];
    assert_eq!(fp.code, 2468);
    assert_eq!(fp.file, "");
    assert_eq!(fp.line, 0);
    assert_eq!(fp.column, 0);
    assert_eq!(fp.message_key, "Cannot find global value 'Promise'.");
}

#[test]
fn test_atypes_package_in_extracts_simple_package() {
    assert_eq!(
        atypes_package_in("/some/path/node_modules/@types/node/index.d.ts"),
        Some("node".to_string())
    );
    assert_eq!(
        atypes_package_in("node_modules/@types/node/index.d.ts"),
        Some("node".to_string())
    );
}

#[test]
fn test_atypes_package_in_extracts_scoped_package() {
    // tsc de-mangles `@scope/pkg` to `@types/scope__pkg` on disk.
    assert_eq!(
        atypes_package_in("/x/node_modules/@types/scope__pkg/index.d.ts"),
        Some("@scope/pkg".to_string())
    );
}

#[test]
fn test_atypes_package_in_returns_none_for_non_atypes_path() {
    assert_eq!(atypes_package_in("/foo/bar/baz.ts"), None);
    assert_eq!(atypes_package_in("node_modules/foo/index.d.ts"), None);
    assert_eq!(atypes_package_in(""), None);
}

#[test]
fn test_atypes_package_in_handles_subdir_paths() {
    // Sub-paths inside the @types package still resolve to the package name.
    assert_eq!(
        atypes_package_in("/p/node_modules/@types/node/fs/promises.d.ts"),
        Some("node".to_string())
    );
}

#[test]
fn test_normalize_file_not_found_message_key_handles_windows_backslashes() {
    // Triple-slash reference with Windows-style backslashes should normalize
    // to a forward-slash relative path.
    let msg = r"File '..\..\..\src\harness\external\mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_strips_macos_var_folders() {
    // Paths stored in the tsc cache on macOS include machine-specific
    // /var/folders/XX/ prefixes that should be stripped.
    // macOS CI temp dirs sit at /var/folders/XX/YYYY/T/test-ZZZ/. A reference
    // path with 3x ../ lands at /var/folders/XX/ (one hash component above the
    // meaningful path). The cache stores the resolved path with that one prefix.
    let msg = "File '/var/folders/6z/src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_strips_private_var_folders() {
    // macOS resolves /var/... to /private/var/... via symlink.
    let msg = "File '/private/var/folders/6z/src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_strips_leading_slash_on_linux() {
    // On Linux, an escaped temp path produces an absolute path at the filesystem
    // root like /src/harness/... (when temp dir is only 1-2 levels deep).
    let msg = "File '/src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_strips_leading_dotdot() {
    // Relative paths with leading ../ should have those stripped.
    let msg = "File '../../../src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(msg),
        "File 'src/harness/external/mocha.d.ts' not found."
    );
}

#[test]
fn test_normalize_file_not_found_message_key_preserves_simple_relative_path() {
    // A simple relative path (no escaping) should be left unchanged.
    let msg = "File 'lib.d.ts' not found.";
    assert_eq!(normalize_file_not_found_message_key(msg), msg);
}

#[test]
fn test_normalize_file_not_found_message_key_preserves_project_relative_path() {
    // A relative path within the project should be left unchanged.
    let msg = "File 'src/utils.ts' not found.";
    assert_eq!(normalize_file_not_found_message_key(msg), msg);
}

#[test]
fn test_normalize_file_not_found_message_key_does_not_alter_non_file_not_found_messages() {
    // Only "File 'X' not found." patterns should be normalized; other messages untouched.
    let msg = "Cannot find name 'foo'.";
    assert_eq!(normalize_file_not_found_message_key(msg), msg);
}

#[test]
fn test_normalize_file_not_found_message_key_both_sides_converge() {
    // The Linux actual output and macOS-cache expected output should normalize
    // to the same canonical form, making fingerprint comparison succeed.
    // linux_actual: resolved from /tmp/xxx/ going 3 levels up → /src/harness/...
    // macos_cache:  as stored in the tsc CI cache (one hash component after /var/folders/)
    // backslash_actual: tsz output before the directive.rs backslash-normalization fix
    let linux_actual = "File '/src/harness/external/mocha.d.ts' not found.";
    let macos_cache = "File '/var/folders/6z/src/harness/external/mocha.d.ts' not found.";
    let backslash_actual = r"File '..\..\..\src\harness\external\mocha.d.ts' not found.";

    let canonical = "File 'src/harness/external/mocha.d.ts' not found.";
    assert_eq!(
        normalize_file_not_found_message_key(linux_actual),
        canonical
    );
    assert_eq!(normalize_file_not_found_message_key(macos_cache), canonical);
    assert_eq!(
        normalize_file_not_found_message_key(backslash_actual),
        canonical
    );
}

#[test]
fn tsz_wrapper_has_no_ad_hoc_extra_fingerprint_helpers() {
    // Match the function name regardless of visibility (`fn`, `pub fn`,
    // `pub(crate) fn`, `pub(super) fn`, ...). The pattern is intentionally
    // permissive so visibility renames or attribute-prefixed forms still
    // trip the guard.
    let source = include_str!("../src/tsz_wrapper.rs");
    let needle = "fn is_extra_";
    let mut ad_hoc = Vec::new();
    for (start, _) in source.match_indices(needle) {
        // Require the preceding character to be whitespace or a visibility
        // marker so we don't match `is_extra_*` inside a doc string.
        let preceded_by_decl_boundary = start == 0
            || source[..start]
                .chars()
                .last()
                .is_some_and(|c| c.is_whitespace() || c == ')');
        if !preceded_by_decl_boundary {
            continue;
        }
        let name = source[start + needle.len()..]
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        ad_hoc.push(format!("is_extra_{name}"));
    }
    assert!(
        ad_hoc.is_empty(),
        "ad-hoc parity suppressor helpers found in crates/conformance/src/tsz_wrapper.rs: {:?}\n\
         Fix the underlying structured diagnostic rule instead of filtering a rendered fingerprint.",
        ad_hoc,
    );
}
