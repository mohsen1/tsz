//! Fail-closed access to the shared pinned TypeScript 7 native oracle.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedOracle {
    pub binary_path: PathBuf,
    pub provenance: serde_json::Value,
}

const PROVENANCE_KEYS: &[&str] = &[
    "schemaVersion",
    "packageName",
    "platformPackageName",
    "version",
    "gitHead",
    "wrapperIntegrity",
    "platformIntegrity",
    "wrapperPackageJsonSha256",
    "wrapperBinSha256",
    "platformPackageJsonSha256",
    "platformPackageTreeSha256",
    "binarySha256",
    "binaryPath",
    "fingerprint",
];

fn value_str<'a>(value: &'a serde_json::Value, key: &str) -> anyhow::Result<&'a str> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("oracle provenance {key} must be a string"))
}

/// Stable, platform-neutral envelope around the exact native oracle that
/// generated the cache. The manifest hash lets any supported platform verify
/// a cache generated on another supported platform.
pub fn evidence(repo_root: &Path, oracle: &VerifiedOracle) -> anyhow::Result<serde_json::Value> {
    let manifest_path = repo_root.join("scripts/emit/oracle-manifest.json");
    let manifest_sha256 = crate::integrity::sha256_bytes(
        &std::fs::read(&manifest_path)
            .with_context(|| format!("cannot read oracle manifest {}", manifest_path.display()))?,
    );
    let value = serde_json::json!({
        "schemaVersion": 1,
        "manifestSha256": manifest_sha256,
        "generator": oracle.provenance,
    });
    validate_evidence(repo_root, &value)?;
    Ok(value)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintBase<'a> {
    schema_version: u64,
    package_name: &'a str,
    platform_package_name: &'a str,
    version: &'a str,
    git_head: &'a str,
    wrapper_integrity: &'a str,
    platform_integrity: &'a str,
    wrapper_package_json_sha256: &'a str,
    wrapper_bin_sha256: &'a str,
    platform_package_json_sha256: &'a str,
    platform_package_tree_sha256: &'a str,
    binary_sha256: &'a str,
    binary_path: &'a str,
}

/// Validate recorded generator provenance against the checked-in manifest,
/// including the generator platform package, tree, native executable, package
/// integrities, version, and wrapper hashes.
pub fn validate_evidence(repo_root: &Path, evidence: &serde_json::Value) -> anyhow::Result<()> {
    let object = evidence
        .as_object()
        .context("oracle evidence must be an object")?;
    let keys = object
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if keys
        != ["generator", "manifestSha256", "schemaVersion"]
            .into_iter()
            .collect()
    {
        anyhow::bail!("oracle evidence has an unexpected schema");
    }
    if evidence
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        anyhow::bail!("oracle evidence schemaVersion must be 1");
    }

    let manifest_path = repo_root.join("scripts/emit/oracle-manifest.json");
    let manifest_bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("cannot read oracle manifest {}", manifest_path.display()))?;
    let manifest_hash = crate::integrity::sha256_bytes(&manifest_bytes);
    if value_str(evidence, "manifestSha256")? != manifest_hash {
        anyhow::bail!("oracle evidence manifest hash does not match the checked-in manifest");
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).context("oracle manifest is invalid JSON")?;
    let generator = evidence
        .get("generator")
        .context("oracle evidence has no generator provenance")?;
    let generator_object = generator
        .as_object()
        .context("oracle generator provenance must be an object")?;
    let actual_keys = generator_object
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if actual_keys != PROVENANCE_KEYS.iter().copied().collect() {
        anyhow::bail!("oracle generator provenance has an unexpected schema");
    }
    if generator
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        anyhow::bail!("oracle generator provenance schemaVersion must be 1");
    }

    for (provenance_key, manifest_key) in [
        ("packageName", "packageName"),
        ("version", "version"),
        ("gitHead", "gitHead"),
        ("wrapperIntegrity", "wrapperIntegrity"),
        ("wrapperPackageJsonSha256", "wrapperPackageJsonSha256"),
        ("wrapperBinSha256", "wrapperBinSha256"),
    ] {
        if generator.get(provenance_key) != manifest.get(manifest_key) {
            anyhow::bail!("oracle provenance {provenance_key} disagrees with manifest");
        }
    }

    let platform_package_name = value_str(generator, "platformPackageName")?;
    let prefix = value_str(&manifest, "platformPackagePrefix")?;
    let suffix = platform_package_name
        .strip_prefix(prefix)
        .context("oracle platform package does not use pinned prefix")?;
    let platform = manifest
        .get("platforms")
        .and_then(|platforms| platforms.get(suffix))
        .with_context(|| format!("oracle generator platform {suffix} is not pinned"))?;
    for (provenance_key, manifest_key) in [
        ("platformIntegrity", "packageIntegrity"),
        ("platformPackageJsonSha256", "packageJsonSha256"),
        ("platformPackageTreeSha256", "packageTreeSha256"),
        ("binarySha256", "binarySha256"),
    ] {
        if generator.get(provenance_key) != platform.get(manifest_key) {
            anyhow::bail!("oracle provenance {provenance_key} disagrees with manifest");
        }
    }
    for key in [
        "wrapperPackageJsonSha256",
        "wrapperBinSha256",
        "platformPackageJsonSha256",
        "platformPackageTreeSha256",
        "binarySha256",
    ] {
        if !crate::integrity::is_lower_hex(value_str(generator, key)?, 64) {
            anyhow::bail!("oracle provenance {key} must be 64 lowercase hex bytes");
        }
    }
    let executable = if suffix.starts_with("win32-") {
        "tsc.exe"
    } else {
        "tsc"
    };
    let expected_path = format!("scripts/node_modules/{platform_package_name}/lib/{executable}");
    if value_str(generator, "binaryPath")? != expected_path {
        anyhow::bail!("oracle binary path is not the pinned platform package executable");
    }

    let base = FingerprintBase {
        schema_version: 1,
        package_name: value_str(generator, "packageName")?,
        platform_package_name,
        version: value_str(generator, "version")?,
        git_head: value_str(generator, "gitHead")?,
        wrapper_integrity: value_str(generator, "wrapperIntegrity")?,
        platform_integrity: value_str(generator, "platformIntegrity")?,
        wrapper_package_json_sha256: value_str(generator, "wrapperPackageJsonSha256")?,
        wrapper_bin_sha256: value_str(generator, "wrapperBinSha256")?,
        platform_package_json_sha256: value_str(generator, "platformPackageJsonSha256")?,
        platform_package_tree_sha256: value_str(generator, "platformPackageTreeSha256")?,
        binary_sha256: value_str(generator, "binarySha256")?,
        binary_path: value_str(generator, "binaryPath")?,
    };
    let expected_fingerprint = format!(
        "sha256:{}",
        crate::integrity::sha256_bytes(&serde_json::to_vec(&base)?)
    );
    if value_str(generator, "fingerprint")? != expected_fingerprint {
        anyhow::bail!("oracle provenance fingerprint is invalid");
    }
    Ok(())
}

impl VerifiedOracle {
    pub fn version(&self) -> anyhow::Result<&str> {
        self.provenance
            .get("version")
            .and_then(serde_json::Value::as_str)
            .context("verified oracle provenance has no version")
    }
}

fn bind_resolved_binary_path(repo_root: &Path, oracle: &VerifiedOracle) -> anyhow::Result<PathBuf> {
    let recorded_relative = value_str(&oracle.provenance, "binaryPath")?;
    let expected = repo_root
        .join(recorded_relative)
        .canonicalize()
        .context("recorded pinned oracle executable does not exist")?;
    let returned = oracle
        .binary_path
        .canonicalize()
        .context("returned pinned oracle executable does not exist")?;
    if returned != expected || !returned.is_file() {
        anyhow::bail!("resolver executable path differs from its recorded provenance");
    }
    let returned_hash = crate::integrity::sha256_bytes(
        &std::fs::read(&returned).context("cannot hash returned pinned oracle executable")?,
    );
    if returned_hash != value_str(&oracle.provenance, "binarySha256")? {
        anyhow::bail!("resolver executable bytes differ from pinned provenance");
    }
    Ok(returned)
}

fn verify_resolved_oracle(repo_root: &Path, oracle: &VerifiedOracle) -> anyhow::Result<PathBuf> {
    let binary_path = bind_resolved_binary_path(repo_root, oracle)?;
    let platform_dir = binary_path
        .parent()
        .and_then(Path::parent)
        .context("pinned oracle executable is not inside a platform package lib directory")?;
    let platform_package_json = platform_dir.join("package.json");
    let platform_package_json_hash = crate::integrity::sha256_bytes(
        &std::fs::read(&platform_package_json).with_context(|| {
            format!(
                "cannot hash platform package metadata {}",
                platform_package_json.display()
            )
        })?,
    );
    if platform_package_json_hash != value_str(&oracle.provenance, "platformPackageJsonSha256")? {
        anyhow::bail!("platform package metadata differs from pinned provenance");
    }
    if crate::integrity::sha256_directory(platform_dir)?
        != value_str(&oracle.provenance, "platformPackageTreeSha256")?
    {
        anyhow::bail!("platform package tree differs from pinned provenance");
    }

    let wrapper_dir = repo_root.join("scripts/node_modules/typescript");
    let wrapper_package_json = wrapper_dir.join("package.json");
    let wrapper_bin = wrapper_dir
        .join("bin")
        .join(if cfg!(windows) { "tsc.cmd" } else { "tsc" });
    for (path, key) in [
        (&wrapper_package_json, "wrapperPackageJsonSha256"),
        (&wrapper_bin, "wrapperBinSha256"),
    ] {
        let observed = crate::integrity::sha256_bytes(
            &std::fs::read(path)
                .with_context(|| format!("cannot hash oracle wrapper input {}", path.display()))?,
        );
        if observed != value_str(&oracle.provenance, key)? {
            anyhow::bail!("oracle wrapper input {key} differs from pinned provenance");
        }
    }

    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(
        repo_root.join("scripts/emit/oracle-manifest.json"),
    )?)?;
    let expected_version_output = value_str(&manifest, "versionOutput")?;
    let version_output = std::process::Command::new(&binary_path)
        .arg("--version")
        .output()
        .context("failed to execute pinned native oracle version probe")?;
    if !version_output.status.success()
        || String::from_utf8(version_output.stdout)
            .context("pinned oracle version output was not UTF-8")?
            .trim()
            != expected_version_output
        || !version_output.stderr.is_empty()
    {
        anyhow::bail!("pinned native oracle version probe disagrees with manifest");
    }
    Ok(binary_path)
}

pub fn resolve_verified_oracle(repo_root: &Path) -> anyhow::Result<VerifiedOracle> {
    let resolver = repo_root.join("scripts/emit/resolve-oracle.mjs");
    let output = std::process::Command::new("node")
        .arg("--experimental-strip-types")
        .arg(&resolver)
        .arg("--root")
        .arg(repo_root)
        .output()
        .with_context(|| {
            format!(
                "failed to invoke pinned oracle resolver {}",
                resolver.display()
            )
        })?;
    if !output.status.success() {
        anyhow::bail!(
            "pinned oracle verification failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let oracle: VerifiedOracle = serde_json::from_slice(&output.stdout)
        .context("pinned oracle resolver returned invalid JSON")?;
    if !oracle.binary_path.is_absolute() || !oracle.binary_path.is_file() {
        anyhow::bail!(
            "pinned oracle resolver returned a non-file path: {}",
            oracle.binary_path.display()
        );
    }
    let _ = oracle.version()?;
    let evidence = evidence(repo_root, &oracle)?;
    validate_evidence(repo_root, &evidence)?;
    let binary_path = verify_resolved_oracle(repo_root, &oracle)?;
    Ok(VerifiedOracle {
        binary_path,
        provenance: oracle.provenance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf()
    }

    fn linux_evidence(root: &Path) -> serde_json::Value {
        let manifest_bytes =
            std::fs::read(root.join("scripts/emit/oracle-manifest.json")).expect("manifest");
        let manifest: serde_json::Value =
            serde_json::from_slice(&manifest_bytes).expect("manifest JSON");
        let platform = &manifest["platforms"]["linux-x64"];
        let base = FingerprintBase {
            schema_version: 1,
            package_name: manifest["packageName"].as_str().unwrap(),
            platform_package_name: "@typescript/typescript-linux-x64",
            version: manifest["version"].as_str().unwrap(),
            git_head: manifest["gitHead"].as_str().unwrap(),
            wrapper_integrity: manifest["wrapperIntegrity"].as_str().unwrap(),
            platform_integrity: platform["packageIntegrity"].as_str().unwrap(),
            wrapper_package_json_sha256: manifest["wrapperPackageJsonSha256"].as_str().unwrap(),
            wrapper_bin_sha256: manifest["wrapperBinSha256"].as_str().unwrap(),
            platform_package_json_sha256: platform["packageJsonSha256"].as_str().unwrap(),
            platform_package_tree_sha256: platform["packageTreeSha256"].as_str().unwrap(),
            binary_sha256: platform["binarySha256"].as_str().unwrap(),
            binary_path: "scripts/node_modules/@typescript/typescript-linux-x64/lib/tsc",
        };
        let fingerprint = format!(
            "sha256:{}",
            crate::integrity::sha256_bytes(&serde_json::to_vec(&base).unwrap())
        );
        serde_json::json!({
            "schemaVersion": 1,
            "manifestSha256": crate::integrity::sha256_bytes(&manifest_bytes),
            "generator": {
                "schemaVersion": 1,
                "packageName": base.package_name,
                "platformPackageName": base.platform_package_name,
                "version": base.version,
                "gitHead": base.git_head,
                "wrapperIntegrity": base.wrapper_integrity,
                "platformIntegrity": base.platform_integrity,
                "wrapperPackageJsonSha256": base.wrapper_package_json_sha256,
                "wrapperBinSha256": base.wrapper_bin_sha256,
                "platformPackageJsonSha256": base.platform_package_json_sha256,
                "platformPackageTreeSha256": base.platform_package_tree_sha256,
                "binarySha256": base.binary_sha256,
                "binaryPath": base.binary_path,
                "fingerprint": fingerprint,
            }
        })
    }

    #[test]
    fn recorded_platform_native_binary_is_bound_to_manifest() {
        let root = repo_root();
        let mut evidence = linux_evidence(&root);
        validate_evidence(&root, &evidence).expect("pinned linux evidence");
        evidence["generator"]["binarySha256"] = serde_json::Value::String("0".repeat(64));
        assert!(validate_evidence(&root, &evidence).is_err());
    }

    #[test]
    fn resolver_path_and_bytes_cannot_disagree_with_provenance() {
        let temp = tempfile::tempdir().expect("tempdir");
        let pinned = temp.path().join("pinned-tsc");
        let substitute = temp.path().join("substitute-tsc");
        std::fs::write(&pinned, b"pinned bytes").expect("pinned");
        std::fs::write(&substitute, b"substitute bytes").expect("substitute");
        let mut oracle = VerifiedOracle {
            binary_path: pinned.clone(),
            provenance: serde_json::json!({
                "binaryPath": "pinned-tsc",
                "binarySha256": crate::integrity::sha256_bytes(b"pinned bytes"),
            }),
        };
        bind_resolved_binary_path(temp.path(), &oracle).expect("exact path and bytes");

        oracle.binary_path = substitute;
        assert!(bind_resolved_binary_path(temp.path(), &oracle).is_err());
        oracle.binary_path = pinned.clone();
        std::fs::write(pinned, b"mutated bytes").expect("mutation");
        assert!(bind_resolved_binary_path(temp.path(), &oracle).is_err());
    }
}
