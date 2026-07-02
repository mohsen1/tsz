//! `#15317` repo-wide flag-registry ratchet, as a pure source-text scan (no
//! crate internals), following the `arch_source_scans` pattern.
//!
//! `every_tsz_env_flag_is_documented_in_the_ledger`: every `TSZ_*` env var
//! read anywhere in `crates/*/src` must appear in
//! `docs/plan/campaign-flag-ledger.md` (either in the campaign tables or the
//! full-registry appendix). Landing a new flag without documenting its owner /
//! default / flip gate is the exact debt #15317 ratchets away. The
//! campaign-list ↔ gauge ↔ derivation sync is separately machine-checked by
//! the unit tests in `def/core/campaign_channels.rs`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/tsz-solver has a workspace root two levels up")
        .to_path_buf()
}

fn rust_sources_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Extract `TSZ_[A-Z0-9_]+` tokens from lines that read the environment.
fn env_flag_reads(source: &str) -> BTreeSet<String> {
    let mut flags = BTreeSet::new();
    for line in source.lines() {
        if !line.contains("env::var") {
            continue;
        }
        let mut rest = line;
        while let Some(pos) = rest.find("TSZ_") {
            let tail = &rest[pos..];
            let end = tail
                .char_indices()
                .find(|&(_, c)| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
                .map_or(tail.len(), |(i, _)| i);
            // A bare "TSZ_" prefix (e.g. from formatting) is not a flag name.
            if end > "TSZ_".len() {
                flags.insert(tail[..end].to_string());
            }
            rest = &tail[end..];
        }
    }
    flags
}

fn all_env_flags_in_crates() -> BTreeSet<String> {
    let crates_dir = workspace_root().join("crates");
    let mut sources = Vec::new();
    let Ok(entries) = std::fs::read_dir(&crates_dir) else {
        panic!(
            "workspace crates/ directory not found at {}",
            crates_dir.display()
        );
    };
    for entry in entries.flatten() {
        let src = entry.path().join("src");
        if src.is_dir() {
            rust_sources_under(&src, &mut sources);
        }
    }
    assert!(
        sources.len() > 100,
        "flag scan walked implausibly few sources ({}); scan roots moved?",
        sources.len()
    );
    let mut flags = BTreeSet::new();
    for path in sources {
        if let Ok(content) = std::fs::read_to_string(&path) {
            flags.extend(env_flag_reads(&content));
        }
    }
    flags
}

#[test]
fn every_tsz_env_flag_is_documented_in_the_ledger() {
    let ledger_path = workspace_root().join("docs/plan/campaign-flag-ledger.md");
    let ledger = std::fs::read_to_string(&ledger_path)
        .unwrap_or_else(|e| panic!("flag ledger missing at {ledger_path:?}: {e}"));
    let undocumented: Vec<String> = all_env_flags_in_crates()
        .into_iter()
        .filter(|flag| !ledger.contains(flag.as_str()))
        .collect();
    assert!(
        undocumented.is_empty(),
        "TSZ_* env flags read in crates/*/src but missing from \
         docs/plan/campaign-flag-ledger.md: {undocumented:?}. \
         Add a campaign-table row or a full-registry appendix entry — \
         see #15317; do not land env flags without ledger coverage."
    );
}
