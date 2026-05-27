//! DTS emit for recursive generic const-arrow functions (issue #8683).
//!
//! When the inferred return type of a generic const-arrow contains a recursive
//! application `App(Lazy(f), args)` of its own value symbol `f`, declaration emit
//! must expand it by instantiating `f`'s return type with `args` and recursing up
//! to the depth limit (then `/*elided*/ any`), regardless of whether `f`'s return
//! type is a function or an object literal. It must never print `f<args>` — a value
//! symbol used in type position, which `tsc --noEmitOnError` rejects.
//!
//! PR #9383 fixed the function-returning half (a block-scoped const-arrow whose
//! def is recorded in `def_types`). The object-returning half regressed to the
//! named-reference path because a top-level const-arrow's resolved type lives in
//! `symbol_types`, not `def_types`; the printer now falls back through
//! `def_to_symbol` so both shapes unroll identically.

use std::path::PathBuf;
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
        path.push(format!("tsz_recursive_const_arrow_dts_{name}_{nanos}"));
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

/// Compile `source` with declaration emit and return the generated `.d.ts` text.
/// Returns `None` when the tsz binary is unavailable (lets the test self-skip).
fn emit_dts(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let output = Command::new(tsz_bin)
        .args([
            "repro.ts",
            "--declaration",
            "--emitDeclarationOnly",
            "--target",
            "es2015",
            "--lib",
            "es6",
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz declaration emit");

    let dts = std::fs::read_to_string(temp.path.join("repro.d.ts")).unwrap_or_else(|_| {
        panic!(
            "expected repro.d.ts to be emitted.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    Some(dts)
}

fn assert_unrolls_without_value_in_type_position(dts: &str, value_name: &str) {
    // The recursive application must never surface as `value<...>` (a value used
    // in type position). That is the exact illegal output `tsc` rejects.
    assert!(
        !dts.contains(&format!("{value_name}<")),
        "recursive application leaked the value symbol `{value_name}` into type \
         position instead of unrolling:\n{dts}"
    );
    // tsc caps the unroll at the depth limit and bottoms out with `/*elided*/ any`.
    assert!(
        dts.contains("/*elided*/ any"),
        "expected depth-limited recursion to bottom out at `/*elided*/ any`:\n{dts}"
    );
}

/// Reported repro from `declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`:
/// a top-level generic const-arrow returning an object literal whose method recurses
/// on the const itself. The object return type must unroll, not print `testRecFun<...>`.
#[test]
fn object_returning_recursive_const_arrow_unrolls_in_dts() {
    let Some(dts) = emit_dts(
        "object_return",
        r#"
export const testRecFun = <T extends Object>(parent: T) => {
    return {
        result: parent,
        deeper: <U extends Object>(child: U) =>
            testRecFun<T & U>({ ...parent, ...child })
    };
};
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_unrolls_without_value_in_type_position(&dts, "testRecFun");
    // First inner level keeps the source name; shadowed re-introductions are renamed.
    assert!(
        dts.contains("deeper: <U extends Object>"),
        "expected first level to keep the source type-parameter name:\n{dts}"
    );
    assert!(
        dts.contains("deeper: <U_1 extends Object>")
            && dts.contains("deeper: <U_2 extends Object>"),
        "expected shadowed re-introductions to be uniquely renamed (U_1, U_2):\n{dts}"
    );
    // The accumulated intersection grows one member per level.
    assert!(
        dts.contains("result: T & U & U_1"),
        "expected the result intersection to accumulate across levels:\n{dts}"
    );
}

/// The same rule, with different type-parameter and property names, proves the
/// fix is structural and not keyed to `T`/`U`/`deeper`/`testRecFun` spellings.
#[test]
fn object_returning_recursion_dts_unroll_is_not_hardcoded_to_names() {
    let Some(dts) = emit_dts(
        "renamed",
        r#"
export const build = <P extends Object>(node: P) => {
    return {
        value: node,
        nest: <C extends Object>(extra: C) =>
            build<P & C>({ ...node, ...extra })
    };
};
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_unrolls_without_value_in_type_position(&dts, "build");
    assert!(
        dts.contains("nest: <C extends Object>")
            && dts.contains("nest: <C_1 extends Object>")
            && dts.contains("nest: <C_2 extends Object>"),
        "expected renamed iteration variable to unroll as C, C_1, C_2:\n{dts}"
    );
    assert!(
        dts.contains("value: P & C & C_1"),
        "expected the accumulated intersection to use the renamed parameters:\n{dts}"
    );
}

/// Regression guard for PR #9383: a const-arrow whose recursion returns a *function*
/// (via a block-scoped helper) must keep unrolling its quantifier chain in DTS.
#[test]
fn function_returning_recursive_const_arrow_still_unrolls_in_dts() {
    let Some(dts) = emit_dts(
        "function_return",
        r#"
export const make = <T>(t: T) => {
    const step = <U>(u: U) => {
        return Object.assign(
            <K extends keyof U>(key: K) => step<U[K]>(u[key]),
            { get: () => u });
    };
    return step<T>(t);
};
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_unrolls_without_value_in_type_position(&dts, "make");
    assert_unrolls_without_value_in_type_position(&dts, "step");
    assert!(
        dts.contains("<K_1 extends keyof") && dts.contains("<K_10 extends keyof"),
        "expected the function-returning recursion to unroll its quantifier chain:\n{dts}"
    );
}

/// Negative case: a non-recursive generic const-arrow returning an object must emit
/// the plain object shape with no depth-limit sentinel. This proves the expansion
/// fires only for genuinely recursive applications, not for every generic const.
#[test]
fn non_recursive_generic_const_arrow_does_not_emit_elided_sentinel() {
    let Some(dts) = emit_dts(
        "non_recursive",
        r#"
export const wrap = <T>(t: T) => ({ value: t, again: t });
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        !dts.contains("/*elided*/ any"),
        "non-recursive const-arrow must not trigger depth-limited expansion:\n{dts}"
    );
    assert!(
        dts.contains("value: T") && dts.contains("again: T"),
        "expected the plain object return shape to be emitted:\n{dts}"
    );
}
