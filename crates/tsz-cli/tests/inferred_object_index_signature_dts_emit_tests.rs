//! DTS emit for inferred object-literal index signatures and returned-local
//! multi-line object type annotations.
//!
//! Two structural rules are covered:
//!
//! Rule A: when declaration emit needs the inferred type of an object literal
//! whose members are all non-emittable computed keys (e.g. `["" + ""]`), the
//! checker collapses those dynamic members into index signatures on the solver
//! type. The emitter must serialize that structural type rather than dropping
//! to an empty `{}` via the source-text object-literal fallback.
//!
//! Rule B: when a function returns a local whose declared type annotation is a
//! multi-line object type literal (`{ ... }` spanning several lines), the
//! emitter must serialize the local's structural type rather than copying the
//! raw source slice, which preserves source indentation/member ordering and can
//! capture trailing tokens when the annotation has no terminator.

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
        path.push(format!("tsz_inferred_idx_dts_{name}_{nanos}"));
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

// =============================================================================
// Rule A: inferred object-literal index signatures from dynamic computed keys
// =============================================================================

/// Primary repro (computedPropertyNamesDeclarationEmit5): an object whose
/// members are all dynamic-string computed keys infers `{ [x: string]: any; }`.
#[test]
fn dynamic_computed_string_keys_infer_string_index_signature() {
    let Some(dts) = emit_dts(
        "dyn_string",
        r#"
var v = {
    ["" + ""]: 0,
    ["" + ""]() { },
    get ["" + ""]() { return 0; },
    set ["" + ""](x) { }
};
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("[x: string]: any;"),
        "dynamic computed keys must collapse to a string index signature:\n{dts}"
    );
    assert!(
        !dts.contains("declare var v: {}"),
        "the index signature must not be dropped to an empty object:\n{dts}"
    );
}

/// Adjacent case: a single dynamic-string computed property (different spelling
/// of the computed key) proves the rule is not tied to the `"" + ""` shape.
#[test]
fn single_dynamic_computed_string_key_infers_string_index_signature() {
    let Some(dts) = emit_dts(
        "single_dyn",
        r#"
declare const key: string;
var rec = {
    [key.toUpperCase()]: 123
};
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("[x: string]:"),
        "a single dynamic computed string key must infer a string index signature:\n{dts}"
    );
    assert!(
        !dts.contains("declare var rec: {}"),
        "the index signature must not be dropped:\n{dts}"
    );
}

/// Negative/fallback case: an object literal with only concrete (emittable)
/// properties keeps its named members and must NOT synthesize a spurious index
/// signature.
#[test]
fn concrete_only_object_literal_keeps_named_members() {
    let Some(dts) = emit_dts(
        "concrete_only",
        r#"
var p = {
    a: 1,
    b: "two"
};
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("a: number;") && dts.contains("b: string;"),
        "concrete members must be preserved:\n{dts}"
    );
    assert!(
        !dts.contains("[x: string]:") && !dts.contains("[x: number]:"),
        "no spurious index signature for a concrete-only object:\n{dts}"
    );
}

// =============================================================================
// Rule B: returned-local multi-line object type annotation
// =============================================================================

/// Primary repro (readonlyInDeclarationFile, `function g`): a returned local
/// with a multi-line object type literal annotation must be printed
/// structurally (readonly preserved, index signature first, normalized indent),
/// not copied from source text.
#[test]
fn returned_local_multiline_object_annotation_prints_structurally() {
    let Some(dts) = emit_dts(
        "returned_local",
        r#"
function g() {
    var x: {
        readonly a: string;
        readonly [x: string]: Object;
    }
    return x;
}
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("readonly [x: string]: Object;"),
        "readonly index signature must be preserved in the return type:\n{dts}"
    );
    assert!(
        dts.contains("readonly a: string;"),
        "readonly property must be preserved in the return type:\n{dts}"
    );
    assert!(
        !dts.contains("return;") && !dts.contains("return x"),
        "raw source text (trailing `return`) must not leak into the type:\n{dts}"
    );
    // Index signatures are ordered before named members by the printer.
    let idx_pos = dts.find("readonly [x: string]: Object;").unwrap();
    let prop_pos = dts.find("readonly a: string;").unwrap();
    assert!(
        idx_pos < prop_pos,
        "index signature should precede the named member:\n{dts}"
    );
}

/// Adjacent case: renamed local + different member names/types prove the rule is
/// not tied to the `x` / `a` spelling in the primary repro.
#[test]
fn returned_local_multiline_object_annotation_renamed_members() {
    let Some(dts) = emit_dts(
        "returned_local_renamed",
        r#"
function build() {
    var result: {
        readonly title: number;
        readonly [k: string]: Object;
    }
    return result;
}
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("readonly [k: string]: Object;"),
        "renamed index signature parameter is preserved:\n{dts}"
    );
    assert!(
        dts.contains("readonly title: number;"),
        "renamed readonly member is preserved:\n{dts}"
    );
    assert!(
        !dts.contains("return;") && !dts.contains("return result"),
        "raw source text must not leak into the renamed return type:\n{dts}"
    );
}

/// Adjacent case: a non-readonly multi-line object annotation (no index
/// signature) is still printed structurally without leaking raw source text.
#[test]
fn returned_local_multiline_plain_object_annotation_prints_structurally() {
    let Some(dts) = emit_dts(
        "returned_local_plain",
        r#"
function make() {
    var obj: {
        first: string;
        second: number;
    }
    return obj;
}
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("first: string;") && dts.contains("second: number;"),
        "plain multi-line object members are preserved structurally:\n{dts}"
    );
    assert!(
        !dts.contains("return;") && !dts.contains("return obj"),
        "raw source text must not leak into the plain return type:\n{dts}"
    );
}
