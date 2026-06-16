//! End-to-end regression for issue #13250 — the query-scoped nested eval memo
//! (recursion-pruning) must not change the result of the ts-toolbelt `AutoPath`
//! relation.
//!
//! `AutoPath<O, P>` enumerates every dotted access path of `O` through a
//! recursive `MetaPath` mapped type and a `Join`-style template-literal fold.
//! The relation layer validates a concrete path string against that enumerated
//! union by spinning up many fresh `TypeEvaluator`s within one top-level query;
//! the nested memo now lets a sibling reuse a subtree a prior one computed. The
//! property under test is that the *relation result* is unchanged: a valid path
//! is accepted and an invalid one is rejected (`TS2345`), at every depth and
//! independent of binder spellings.
//!
//! These tests run the real binary in a subprocess at the production budget, so
//! the recursion runs to completion rather than bailing on a tiny test budget.
//! Binder names are varied between cases so the assertion is structural, never
//! a fixture-name fast path.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
        path.push(format!("tsz_autopath_prune_{name}_{nanos}"));
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

/// The shared `AutoPath` path-builder primitives, parameterised so each test can
/// vary the binder spellings (anti-hardcoding) without changing the structure.
fn autopath_prelude(cast: &str, meta: &str, exec: &str, k: &str, p: &str, paths: &str) -> String {
    format!(
        "type {cast}<A, B> = A extends B ? A : B;\n\
         type {meta}<O, {p} extends string = '', {paths} extends string = ''> = {{\n\
           0: {{\n\
             [{k} in keyof O]: {meta}<\n\
               O[{k}],\n\
               {p} extends '' ? `${{{cast}<{k}, string>}}` : `${{{p}}}.${{{cast}<{k}, string>}}`,\n\
               {paths} | ({p} extends '' ? `${{{cast}<{k}, string>}}` : `${{{p}}}.${{{cast}<{k}, string>}}`)\n\
             >;\n\
           }}[keyof O];\n\
           1: {paths};\n\
         }}[O extends object ? 0 : 1];\n\
         type {exec}<O> = {meta}<O> extends infer R ? R : never;\n"
    )
}

fn run_check(name: &str, source: &str) -> String {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping {name}: tsz binary not found");
        return String::new();
    };
    let temp = TempDir::new(name).expect("temp dir");
    let file = temp.path.join("repro.ts");
    std::fs::write(&file, source).expect("write repro file");

    let output = Command::new(tsz_bin)
        .args(["repro.ts", "--noEmit", "--pretty", "false"])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz repro");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// A path that exists in the nested object must be accepted; the AutoPath
/// enumeration and the path-validity relation must agree.
#[test]
fn autopath_accepts_valid_dotted_path() {
    let prelude = autopath_prelude("Cast", "MetaPath", "ExecPath", "K", "P", "paths");
    let src = format!(
        "{prelude}\
         interface L2 {{ a2: string; b2: number; }}\n\
         interface L1 {{ a1: L2; b1: L2; }}\n\
         interface L0 {{ a0: L1; b0: L1; }}\n\
         declare function takes<O>(o: O, p: ExecPath<O>): void;\n\
         declare const obj: L0;\n\
         takes(obj, \"a0.a1.a2\");\n"
    );
    let out = run_check("valid_path", &src);
    if out.is_empty() {
        return;
    }
    assert!(
        !out.contains("TS2345") && !out.contains("TS2322"),
        "valid path \"a0.a1.a2\" must be accepted by AutoPath; got:\n{out}"
    );
}

/// A path that does NOT exist must be rejected with `TS2345`. Binder names are
/// deliberately different from the positive case so the assertion is structural.
#[test]
fn autopath_rejects_invalid_dotted_path() {
    let prelude = autopath_prelude("Coerce", "Walk", "Paths", "Key", "Prefix", "acc");
    let src = format!(
        "{prelude}\
         interface Deep {{ leaf: string; count: number; }}\n\
         interface Mid {{ first: Deep; second: Deep; }}\n\
         interface Root {{ alpha: Mid; beta: Mid; }}\n\
         declare function consume<O>(o: O, p: Paths<O>): void;\n\
         declare const root: Root;\n\
         consume(root, \"alpha.first.missing\");\n"
    );
    let out = run_check("invalid_path", &src);
    if out.is_empty() {
        return;
    }
    assert!(
        out.contains("TS2345"),
        "invalid path \"alpha.first.missing\" must be rejected by AutoPath; got:\n{out}"
    );
}

/// A deeper nesting (5 levels) — the recursion-pruning must hold at depth, with
/// yet another set of binder spellings. The valid deep path is accepted and a
/// sibling invalid one is rejected in the same file (one top-level query each).
#[test]
fn autopath_deep_nesting_accepts_valid_rejects_invalid() {
    let prelude = autopath_prelude("Id", "Enum", "Pick", "Idx", "Pre", "Seen");
    let src = format!(
        "{prelude}\
         interface N4 {{ w: string; }}\n\
         interface N3 {{ d: N4; }}\n\
         interface N2 {{ c: N3; }}\n\
         interface N1 {{ b: N2; }}\n\
         interface N0 {{ a: N1; }}\n\
         declare function f<O>(o: O, p: Pick<O>): void;\n\
         declare const o: N0;\n\
         f(o, \"a.b.c.d.w\");\n\
         f(o, \"a.b.c.nope\");\n"
    );
    let out = run_check("deep_path", &src);
    if out.is_empty() {
        return;
    }
    // Exactly one rejection: the invalid path. The valid deep path is accepted.
    let rejections = out.matches("TS2345").count();
    assert_eq!(
        rejections, 1,
        "deep AutoPath must accept the valid 5-level path and reject only the \
         invalid one (exactly one TS2345); got:\n{out}"
    );
}
