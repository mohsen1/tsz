//! Exact identity checks for the pinned TypeScript conformance corpus.

use anyhow::Context;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusIdentity {
    pub commit: String,
    pub tree: String,
}

const GIT_ROUTING_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_REPLACE_REF_BASE",
];

fn git_command() -> std::process::Command {
    let mut command = std::process::Command::new("git");
    for name in GIT_ROUTING_ENV {
        command.env_remove(name);
    }
    command.env("GIT_NO_REPLACE_OBJECTS", "1");
    command
}

fn git_output(corpus_root: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = git_command()
        .arg("-C")
        .arg(corpus_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to inspect corpus at {}", corpus_root.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {:?} failed for corpus {} ({}): {}",
            args,
            corpus_root.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("corpus git output was not UTF-8")
        .map(|text| text.trim().to_string())
}

/// Require the default corpus path, its pinned commit and Merkle tree, and a
/// pristine worktree. Candidate bytes are bound separately by the domain hash.
pub fn verify_pinned_corpus(repo_root: &Path, test_dir: &Path) -> anyhow::Result<CorpusIdentity> {
    let repo_root = repo_root.canonicalize().with_context(|| {
        format!(
            "cannot canonicalize repository root {}",
            repo_root.display()
        )
    })?;
    let expected_cases = repo_root.join("TypeScript/tests/cases");
    let actual_cases = test_dir
        .canonicalize()
        .with_context(|| format!("cannot canonicalize corpus cases {}", test_dir.display()))?;
    let expected_cases = expected_cases.canonicalize().with_context(|| {
        format!(
            "cannot canonicalize pinned corpus cases {}",
            expected_cases.display()
        )
    })?;
    if actual_cases != expected_cases {
        anyhow::bail!(
            "canonical conformance evidence requires pinned corpus {}, got {}",
            expected_cases.display(),
            actual_cases.display()
        );
    }

    let pin_path = repo_root.join("scripts/ci/typescript-submodule-ref");
    let pin = std::fs::read_to_string(&pin_path)
        .with_context(|| format!("cannot read corpus pin {}", pin_path.display()))?
        .trim()
        .to_string();
    if !crate::integrity::is_lower_hex(&pin, 40) {
        anyhow::bail!("corpus pin must be exactly 40 lowercase hex bytes");
    }

    let corpus_root = repo_root.join("TypeScript");
    let commit = git_output(&corpus_root, &["rev-parse", "HEAD"])?;
    if commit != pin {
        anyhow::bail!("pinned corpus commit mismatch: expected {pin}, got {commit}");
    }
    let tree = git_output(&corpus_root, &["rev-parse", "HEAD^{tree}"])?;
    if !crate::integrity::is_lower_hex(&tree, 40) {
        anyhow::bail!("corpus Merkle tree is not an exact 40-byte git object id");
    }
    let dirty = git_output(
        &corpus_root,
        &["status", "--porcelain", "--untracked-files=all"],
    )?;
    if !dirty.is_empty() {
        anyhow::bail!("pinned TypeScript corpus is dirty: {dirty}");
    }
    let ignored_candidates = git_output(
        &corpus_root,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored=matching",
            "--",
            "tests/cases",
            "tests/lib",
        ],
    )?;
    if !ignored_candidates.is_empty() {
        anyhow::bail!(
            "pinned TypeScript semantic inputs contain ignored candidates not owned by the corpus tree: {ignored_candidates}"
        );
    }

    Ok(CorpusIdentity { commit, tree })
}

pub fn repository_root_from_current_dir() -> anyhow::Result<PathBuf> {
    let current = std::env::current_dir().context("cannot resolve current directory")?;
    let output = git_command()
        .arg("-C")
        .arg(&current)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to resolve repository root")?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to resolve repository root ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let root = PathBuf::from(
        String::from_utf8(output.stdout)
            .context("repository root was not UTF-8")?
            .trim(),
    );
    root.canonicalize()
        .with_context(|| format!("cannot canonicalize repository root {}", root.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) -> String {
        let output = git_command()
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("git output")
            .trim()
            .to_string()
    }

    #[test]
    fn ignored_candidate_is_not_owned_by_the_pinned_corpus_tree() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        let corpus = root.join("TypeScript");
        let cases = corpus.join("tests/cases/compiler");
        let libraries = corpus.join("tests/lib");
        std::fs::create_dir_all(&cases).expect("cases");
        std::fs::create_dir_all(&libraries).expect("libraries");
        std::fs::create_dir_all(root.join("scripts/ci")).expect("pin directory");
        std::fs::write(cases.join("tracked.ts"), "let value = 1;\n").expect("source");
        std::fs::write(libraries.join("tracked.d.ts"), "declare const value: 1;\n")
            .expect("library");
        std::fs::write(
            corpus.join(".gitignore"),
            "tests/cases/generated.js\ntests/lib/generated.d.ts\n",
        )
        .expect("ignore");
        git(&corpus, &["init", "-q"]);
        git(&corpus, &["add", "."]);
        git(
            &corpus,
            &[
                "-c",
                "user.name=Conformance Test",
                "-c",
                "user.email=conformance@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        );
        let commit = git(&corpus, &["rev-parse", "HEAD"]);
        std::fs::write(root.join("scripts/ci/typescript-submodule-ref"), commit).expect("pin");

        verify_pinned_corpus(root, &root.join("TypeScript/tests/cases"))
            .expect("tracked pristine corpus");
        std::fs::write(corpus.join("tests/cases/generated.js"), "let hidden = 1;\n")
            .expect("ignored candidate");
        let error = verify_pinned_corpus(root, &root.join("TypeScript/tests/cases"))
            .expect_err("ignored candidate must fail closed");
        assert!(error.to_string().contains("ignored candidates"));

        std::fs::remove_file(corpus.join("tests/cases/generated.js")).expect("remove candidate");
        std::fs::write(
            corpus.join("tests/lib/generated.d.ts"),
            "declare const hidden: 1;\n",
        )
        .expect("ignored library");
        let error = verify_pinned_corpus(root, &root.join("TypeScript/tests/cases"))
            .expect_err("ignored referenced library must fail closed");
        assert!(error.to_string().contains("ignored candidates"));
    }
}
