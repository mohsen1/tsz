//! Regression: narrowing a union whose members are unresolved cross-file
//! `Lazy(DefId)` type-alias references must filter on each member's resolved
//! structural form, not on the raw `Lazy` ref.
//!
//! An unresolved `Lazy(DefId)` is permissively "assignable" to any target, so
//! the union-member filter in `narrow_to_type` kept every constituent. In a
//! real cross-file program the type-guard narrowing of a type parameter
//! `T extends Dict | List | Bag | Sack` by `isBag(value)` therefore produced
//! `T & (Dict | List | Bag | Sack)` instead of `T & Bag`, and the surviving
//! `T & Sack` arm failed the generic `wrapBag_<T extends Bag>` parameter — a
//! false `TS2345`. `tsc` accepts.
//!
//! The fixture under `fixtures/narrow_union_lazy_alias/` is a renamed,
//! de-branded reduction of the shape that first exhibited this (immer's
//! `createProxy`). Binder names are deliberately unrelated to that origin so
//! the fix cannot be a name-scoped path (anti-hardcoding gate). The bug is
//! emergent from the full cross-file type graph — trimming the module bodies
//! collapses the deferred-resolution timing that triggers it — so the fixture
//! keeps the reducing module set intact and asserts the whole project checks
//! with zero diagnostics.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const FILES: &[(&str, &str)] = &[
    (
        "src/internal.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/internal.ts"),
    ),
    (
        "src/globals.d.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/globals.d.ts"),
    ),
    (
        "src/types/types-internal.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/types/types-internal.ts"),
    ),
    (
        "src/types/types-external.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/types/types-external.ts"),
    ),
    (
        "src/utils/env.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/utils/env.ts"),
    ),
    (
        "src/utils/errors.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/utils/errors.ts"),
    ),
    (
        "src/utils/common.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/utils/common.ts"),
    ),
    (
        "src/utils/plugins.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/utils/plugins.ts"),
    ),
    (
        "src/core/scope.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/core/scope.ts"),
    ),
    (
        "src/core/finalize.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/core/finalize.ts"),
    ),
    (
        "src/core/proxy.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/core/proxy.ts"),
    ),
    (
        "src/core/current.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/core/current.ts"),
    ),
    (
        "src/core/forgeClass.ts",
        include_str!("fixtures/narrow_union_lazy_alias/src/core/forgeClass.ts"),
    ),
];

const TSCONFIG: &str = include_str!("fixtures/narrow_union_lazy_alias/tsconfig.json");

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

fn stage_project(name: &str) -> TempDir {
    let temp = TempDir::new(name).expect("temp dir");
    for (rel, contents) in FILES {
        let path = temp.path.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent dir");
        }
        std::fs::write(path, contents).expect("write fixture file");
    }
    std::fs::write(temp.path.join("tsconfig.json"), TSCONFIG).expect("write tsconfig");
    temp
}

fn run_tsz(tsz_bin: &Path, project_dir: &Path) -> (Option<i32>, String) {
    let output = Command::new(tsz_bin)
        .args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(project_dir)
        .output()
        .expect("run tsz on narrow-union fixture");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code(), combined)
}

#[test]
fn cross_file_union_constrained_type_guard_does_not_false_ts2345() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = stage_project("narrow_union_lazy_alias");
    let (status, out) = run_tsz(&tsz_bin, &temp.path);

    assert!(
        !out.contains("error TS"),
        "narrowing a union-constrained type parameter by a cross-file type \
         guard must keep only the matching constituent (no false TS2345); \
         diagnostics:\n{out}"
    );
    assert_eq!(
        status,
        Some(0),
        "project must compile cleanly; output:\n{out}"
    );
}
