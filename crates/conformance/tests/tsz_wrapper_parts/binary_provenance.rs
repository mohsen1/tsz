use super::*;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

const BUILD_MANIFEST_ENV: &str = "TSZ_CONFORMANCE_BUILD_MANIFEST";

#[derive(Deserialize)]
struct BuildManifest {
    schema_version: u8,
    binaries: BTreeMap<String, BuildBinary>,
}

#[derive(Deserialize)]
struct BuildBinary {
    path: String,
    sha256: String,
    size: u64,
}

pub(super) fn find_tsz_binary(workspace_root: &Path) -> Option<String> {
    let cargo_binary = std::env::var_os("CARGO_BIN_EXE_tsz")
        .or_else(|| option_env!("CARGO_BIN_EXE_tsz").map(OsString::from));
    let manifest = std::env::var_os(BUILD_MANIFEST_ENV);
    resolve_tsz_binary(
        workspace_root,
        cargo_binary,
        manifest,
        verify_build_manifest,
    )
    .map(|path| path.to_string_lossy().into_owned())
}

fn resolve_tsz_binary(
    workspace_root: &Path,
    cargo_binary: Option<OsString>,
    manifest: Option<OsString>,
    verifier: impl FnOnce(&Path, &Path, &[(String, PathBuf)]) -> bool,
) -> Option<PathBuf> {
    if let Some(path) = cargo_binary {
        return executable_regular_file(Path::new(&path));
    }
    let manifest = PathBuf::from(manifest?);
    verified_manifest_binary(workspace_root, &manifest, verifier)
}

fn verified_manifest_binary(
    workspace_root: &Path,
    manifest_path: &Path,
    verifier: impl FnOnce(&Path, &Path, &[(String, PathBuf)]) -> bool,
) -> Option<PathBuf> {
    let manifest: BuildManifest =
        serde_json::from_slice(&std::fs::read(manifest_path).ok()?).ok()?;
    if manifest.schema_version != 1 || manifest.binaries.is_empty() {
        return None;
    }

    let root = workspace_root.canonicalize().ok()?;
    let mut binaries = Vec::with_capacity(manifest.binaries.len());
    let mut tsz_binary = None;
    for (name, record) in &manifest.binaries {
        if name.is_empty()
            || !crate::integrity::is_lower_hex(&record.sha256, 64)
            || Path::new(&record.path)
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return None;
        }
        let configured = root.join(&record.path);
        if configured.symlink_metadata().ok()?.file_type().is_symlink() {
            return None;
        }
        let binary = executable_regular_file(&configured)?;
        if binary.strip_prefix(&root).is_err() {
            return None;
        }
        let bytes = std::fs::read(&binary).ok()?;
        if bytes.len() as u64 != record.size
            || crate::integrity::sha256_bytes(&bytes) != record.sha256
        {
            return None;
        }
        if name == "tsz" {
            tsz_binary = Some(binary.clone());
        }
        binaries.push((name.clone(), binary));
    }
    let tsz_binary = tsz_binary?;
    verifier(&root, manifest_path, &binaries).then_some(tsz_binary)
}

fn executable_regular_file(path: &Path) -> Option<PathBuf> {
    let path = path.canonicalize().ok()?;
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(path)
}

fn verify_build_manifest(
    workspace_root: &Path,
    manifest_path: &Path,
    binaries: &[(String, PathBuf)],
) -> bool {
    let script = workspace_root.join("scripts/conformance/build-manifest.py");
    let mut command = std::process::Command::new("python3");
    command
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg(script)
        .arg("verify")
        .arg("--repo")
        .arg(workspace_root)
        .arg("--manifest")
        .arg(manifest_path);
    for (name, path) in binaries {
        command
            .arg("--binary")
            .arg(format!("{name}={}", path.display()));
    }
    command.status().is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_executable(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn write_manifest(root: &Path, binary_bytes: &[u8], recorded_bytes: &[u8]) -> PathBuf {
        let binary = root.join("bin/tsz");
        write_executable(&binary, binary_bytes);
        let manifest = root.join("manifest.json");
        std::fs::write(
            &manifest,
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "binaries": {
                    "tsz": {
                        "path": "bin/tsz",
                        "sha256": crate::integrity::sha256_bytes(recorded_bytes),
                        "size": recorded_bytes.len(),
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        manifest
    }

    #[test]
    fn cargo_owned_binary_wins_without_manifest_lookup() {
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("cargo/tsz");
        write_executable(&binary, b"cargo binary");

        let resolved = resolve_tsz_binary(
            temp.path(),
            Some(binary.clone().into_os_string()),
            None,
            |_, _, _| panic!("Cargo-owned binary must not consult a manifest"),
        );

        assert_eq!(resolved, Some(binary.canonicalize().unwrap()));
    }

    #[test]
    fn missing_manifest_binary_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("manifest.json");
        std::fs::write(
            &manifest,
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "binaries": {
                    "tsz": {
                        "path": "bin/missing-tsz",
                        "sha256": "00".repeat(32),
                        "size": 0,
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(resolve_tsz_binary(
            temp.path(),
            None,
            Some(manifest.into_os_string()),
            |_, _, _| panic!("missing binary must fail before manifest verification"),
        )
        .is_none());
    }

    #[test]
    fn stale_binary_hash_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(temp.path(), b"new binary", b"old binary");

        assert!(resolve_tsz_binary(
            temp.path(),
            None,
            Some(manifest.into_os_string()),
            |_, _, _| panic!("stale binary must fail before source verification"),
        )
        .is_none());
    }

    #[test]
    fn stale_source_manifest_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(temp.path(), b"exact binary", b"exact binary");

        assert!(resolve_tsz_binary(
            temp.path(),
            None,
            Some(manifest.into_os_string()),
            |_, _, _| false,
        )
        .is_none());
    }

    #[test]
    fn exact_binary_and_source_manifest_are_accepted() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = write_manifest(temp.path(), b"exact binary", b"exact binary");
        let expected = temp.path().join("bin/tsz").canonicalize().unwrap();

        let resolved = resolve_tsz_binary(
            temp.path(),
            None,
            Some(manifest.clone().into_os_string()),
            |root, observed_manifest, binaries| {
                assert_eq!(root, temp.path().canonicalize().unwrap());
                assert_eq!(observed_manifest, manifest);
                assert_eq!(binaries, &[("tsz".to_string(), expected.clone())]);
                true
            },
        );

        assert_eq!(resolved, Some(expected));
    }

    fn process_output(code: i32, stdout: &[u8]) -> std::process::Output {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt;

        std::process::Output {
            status: {
                #[cfg(unix)]
                {
                    std::process::ExitStatus::from_raw(code << 8)
                }
                #[cfg(windows)]
                {
                    std::process::ExitStatus::from_raw(code as u32)
                }
            },
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn compile_wrapper_preserves_ordinary_and_incomplete_exits() {
        let temp = tempfile::tempdir().unwrap();
        let diagnostic =
            b"test.ts(1,1): error TS2322: Type 'string' is not assignable to type 'number'.\n";

        for (exit, output, completion, ordinary) in [
            (0, &b""[..], SemanticCompletion::Complete, vec![0]),
            (1, &diagnostic[..], SemanticCompletion::Complete, vec![1]),
            (2, &diagnostic[..], SemanticCompletion::Complete, vec![2]),
            (3, &b""[..], SemanticCompletion::Incomplete, vec![]),
        ] {
            let result =
                classify_compile_output(process_output(exit, output), temp.path(), HashMap::new());
            assert!(!result.crashed, "exit {exit}");
            assert_eq!(result.semantic_completion, completion, "exit {exit}");
            assert_eq!(result.ordinary_exit_statuses, ordinary, "exit {exit}");
        }
    }
}
