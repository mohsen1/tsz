//! `TypeScript/tests/cases/conformance/salsa/jsContainerMergeTsDeclaration3.ts`
//! (`expected:[TS2339,TS2451]`): a `.d.ts` `declare class A {}` conflicting
//! with a `.js` `const A = {}` reports `TS2451` on both declarations (a
//! variable never declaration-merges with a class — see
//! `crates/tsz-checker/tests/cross_file_js_ts_class_merge_ts2451_suppression_tests.rs`)
//! *and* `TS2339` on a later `A.d = { }` expando-shaped write — but only when
//! the `.d.ts` is processed before the `.js`. tsc's value resolution for the
//! conflicting symbol prefers whichever declaration's file bound FIRST
//! (`mergeSymbol` in `checker.ts` leaves the pre-existing target untouched on
//! a flag conflict): reversing the file order lets the variable's own
//! empty-object-literal type win instead, keeping `A.d = { }` a legitimate
//! expando write. Oracle-verified both orders against pinned
//! `typescript@7.0.2`.
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
//! carry both `CLASS` and `VARIABLE` is provably a redeclaration conflict (a
//! variable can never legitimately declaration-merge with a class, unlike
//! `function`), so expando access through the variable's own shape is
//! rejected — but only when
//! `crates/tsz-checker/src/state/type_analysis/computed/cross_file_variable_class_merge.rs`'s
//! `variable_is_shadowed_by_earlier_class` confirms the class's file was
//! processed first; a naive flags-only check would reject unconditionally
//! and false-positive `TS2339` in the reversed order.

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

/// Like `run_project`, but pins the binder's file-processing order via an
/// explicit `"files"` list instead of relying on default discovery order —
/// needed to exercise the reversed-order (variable's file bound first) half
/// of this fix.
fn run_project_ordered(files: &[(&str, &str)]) -> Option<(bool, String)> {
    let tsz_bin = find_tsz_binary()?;
    let dir = tempfile::tempdir().expect("temp dir");
    let mut file_list = Vec::new();
    for (name, source) in files {
        std::fs::write(dir.path().join(name), source).expect("write file");
        file_list.push(format!("\"{name}\""));
    }
    let tsconfig = format!(
        r#"{{
  "compilerOptions": {{
    "strict": true,
    "allowJs": true,
    "checkJs": true,
    "module": "commonjs",
    "noEmit": true
  }},
  "files": [{}]
}}
"#,
        file_list.join(", ")
    );
    std::fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write tsconfig");
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

/// The order-reversed mirror of `ts2339_fires_for_expando_write_against_class_variable_conflict`:
/// when the `.js` variable's own file is processed BEFORE the conflicting
/// `.d.ts` class, `TS2451` still fires on both declarations (still a
/// redeclaration conflict, unaffected by order), but `A.d = { };` stays a
/// legitimate expando write against the variable's own `{}` type — `TS2339`
/// must NOT fire. Oracle-verified (`typescript@7.0.2`, `files: [b.js, a.d.ts]`).
/// Guards the false positive a naive order-independent `CLASS && VARIABLE`
/// flag check would introduce.
#[test]
fn ts2339_absent_when_js_variable_file_processed_before_conflicting_class() {
    let Some((ok, out)) = run_project_ordered(&[
        ("b.js", "const A = { };\nA.d = { };\n"),
        ("a.d.ts", "declare class A {}\n"),
    ]) else {
        return;
    };
    assert!(!ok, "expected TS2451 diagnostics; output:\n{out}");
    assert_eq!(
        out.matches("TS2451").count(),
        2,
        "expected TS2451 on both declarations regardless of order; output:\n{out}"
    );
    assert_eq!(
        out.matches("TS2339").count(),
        0,
        "the variable's own file was processed first, so A.d = {{ }} must stay a legitimate expando write; output:\n{out}"
    );
}

/// Same repro as `ts2339_fires_for_expando_write_against_class_variable_conflict`,
/// but routed through the order-pinning helper (`files: [a.d.ts, b.js]`) to
/// prove the explicit-order harness reproduces the un-pinned default-order
/// test's result identically — the two tests bracket both file orders.
#[test]
fn ts2339_fires_when_class_file_processed_before_js_variable() {
    let Some((ok, out)) = run_project_ordered(&[
        ("a.d.ts", "declare class A {}\n"),
        ("b.js", "const A = { };\nA.d = { };\n"),
    ]) else {
        return;
    };
    assert!(!ok, "expected diagnostics; output:\n{out}");
    assert_eq!(
        out.matches("TS2451").count(),
        2,
        "expected TS2451 on both declarations; output:\n{out}"
    );
    assert_eq!(
        out.matches("TS2339").count(),
        1,
        "the class's file was processed first, so A.d = {{ }} must report TS2339; output:\n{out}"
    );
}
