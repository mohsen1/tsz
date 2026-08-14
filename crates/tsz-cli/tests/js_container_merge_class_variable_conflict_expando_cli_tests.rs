//! `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration3.ts`
//! (`expected:[TS2339,TS2451]`): a `.d.ts` `declare class A {}` conflicting
//! with a `.js` `const A = {}` reports `TS2451` on both declarations (a
//! variable never declaration-merges with a class — see
//! `crates/tsz-checker/tests/cross_file_js_ts_class_merge_ts2451_suppression_tests.rs`)
//! *and* `TS2339` on a later `A.d = { }` expando-shaped write: tsc's value
//! resolution for the conflicting symbol always prefers the class
//! (`typeof A`, which declares no `d`), so the variable's own
//! empty-object-literal expando-host shape is never authoritative once a
//! conflicting class is also present.
//!
//! Requires the real project-mode driver (`-p tsconfig.json`): the fix
//! depends on the production `global_symbol_file_index` cross-arena merge
//! that unifies `A`'s `.d.ts`/`.js` declarations under one `SymbolId` so
//! `compute_type_of_symbol` sees the merged `CLASS | VARIABLE` flags: none
//! of the lightweight multi-file test harnesses in `tsz-checker` reproduce
//! that merge (confirmed by hand — every one resolves the write against the
//! variable's own local, unmerged symbol and never reproduces the false
//! negative this test guards).
//!
//! Fix: `crates/tsz-checker/src/types/property_access_helpers/expando.rs`,
//! `root_symbol_supports_js_expando_read` /
//! `root_symbol_supports_js_direct_expando_write` — a symbol whose flags
//! carry both `CLASS` and `VARIABLE` is provably a redeclaration conflict
//! (a variable can never legitimately declaration-merge with a class, unlike
//! `function`), so expando access through the variable's own shape is
//! rejected outright rather than falling through to the empty-literal check.

use std::path::PathBuf;
use std::process::Command;

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

const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "allowJs": true,
    "checkJs": true,
    "module": "commonjs",
    "noEmit": true
  }
}
"#;

/// Run `tsz -p tsconfig.json` over an in-memory project, returning
/// `(exit_success, combined_stdout+stderr)`. Returns `None` when the `tsz`
/// binary is unavailable, so each test can bail with a single `let else`
/// instead of repeating the guard.
fn run_project(files: &[(&str, &str)]) -> Option<(bool, String)> {
    let tsz_bin = find_tsz_binary()?;
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        std::fs::write(dir.path().join(name), source).expect("write file");
    }
    std::fs::write(dir.path().join("tsconfig.json"), TSCONFIG).expect("write tsconfig");
    let output = Command::new(tsz_bin)
        .args(["-p", "tsconfig.json", "--pretty", "false"])
        .current_dir(dir.path())
        .output()
        .expect("run tsz");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Some((output.status.success(), combined))
}

/// The core repro, oracle-verified byte-identical against pinned
/// `typescript@7.0.2`: `TS2451` on both declarations, plus `TS2339` on the
/// `A.d = { };` write in `b.js`.
#[test]
fn ts2339_fires_for_expando_write_against_class_variable_conflict() {
    let Some((ok, out)) = run_project(&[
        ("a.d.ts", "declare class A {}\n"),
        ("b.js", "const A = { };\nA.d = { };\n"),
    ]) else {
        return;
    };
    assert!(!ok, "expected diagnostics; output:\n{out}");
    assert_eq!(
        out.matches("TS2451").count(),
        2,
        "expected TS2451 on both the .d.ts and .js declarations; output:\n{out}"
    );
    assert_eq!(
        out.matches("TS2339").count(),
        1,
        "expected TS2339 for the undeclared 'd' member against 'typeof A'; output:\n{out}"
    );
    assert!(
        out.contains("Property 'd' does not exist on type 'typeof A'"),
        "expected the exact message tsc renders; output:\n{out}"
    );
}

/// Anti-hardcoding (§25): the rule is structural, not specific to the names
/// `A`/`d`. Re-run with different identifier and property choices.
#[test]
fn ts2339_fires_for_expando_write_against_class_variable_conflict_renamed() {
    for (class_name, prop) in [("Widget", "extra"), ("MyType", "count")] {
        let dts_src = format!("declare class {class_name} {{}}\n");
        let js_src = format!("const {class_name} = {{ }};\n{class_name}.{prop} = {{ }};\n");
        let Some((ok, out)) = run_project(&[("a.d.ts", &dts_src), ("b.js", &js_src)]) else {
            return;
        };
        assert!(
            !ok,
            "expected diagnostics for '{class_name}'; output:\n{out}"
        );
        assert_eq!(
            out.matches("TS2339").count(),
            1,
            "expected TS2339 for class '{class_name}' + expando '{prop}'; output:\n{out}"
        );
    }
}

/// Positive control: a genuine `function`+`class` merge (no redeclaration
/// conflict — `FUNCTION_EXCLUDES` omits `CLASS`) keeps its expando write
/// clean of `TS2339`/`TS2451`. Proves the CLASS+VARIABLE-specific rejection
/// above didn't overreach into the legitimate FUNCTION+CLASS merge path.
///
/// Only asserts the absence of `TS2339`/`TS2451`, not a clean exit: a
/// cross-file `function`-with-body/ambient-`class` merge currently also
/// trips an unrelated pre-existing false positive, `TS2814` ("Function with
/// bodies can only merge with classes that are ambient" — the check does
/// not recognize a *cross-file* ambient class as ambient; the pinned
/// `typescript@7.0.2` oracle reports nothing for this same input). That gap
/// is outside this fix's scope (expando/CLASS+VARIABLE conflict handling,
/// not the ambient-merge validity check) and is left for a separate PR.
#[test]
fn function_class_merge_expando_write_stays_clean() {
    let Some((_ok, out)) = run_project(&[
        ("a.d.ts", "declare class A { static x: number; }\n"),
        ("b.js", "function A() {}\nA.x = 1;\n"),
    ]) else {
        return;
    };
    assert!(
        !out.contains("TS2339") && !out.contains("TS2451"),
        "function/class merge must not report TS2339/TS2451; output:\n{out}"
    );
}
