//! DTS emit for `ParenthesizedType` nodes (issue #10556 / `declFileTypeAnnotationParenType`).
//!
//! When a `ParenthesizedType` appears in an annotation position (variable
//! type, parameter type, return type, property type), tsc strips the outer
//! parens.  tsz was emitting them verbatim, producing illegal output such as
//! `declare var x: (string)`.
//!
//! In structural positions (array element, union member, intersection arm,
//! conditional check/extends), the surrounding context re-adds parens only
//! when operator precedence requires it, and type-argument positions preserve
//! source-written parens verbatim to avoid `>>` ambiguity.

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
        path.push(format!("tsz_paren_dts_{name}_{nanos}"));
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

/// Primary repro: parenthesized primitive in a variable annotation must be stripped.
/// Adjacent case: different primitive types prove the rule is not spelling-dependent.
#[test]
fn parenthesized_primitive_annotation_is_stripped() {
    let Some(dts) = emit_dts(
        "primitive",
        r#"
export var x: (string);
export var count: (number);
export var flag: (boolean);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("x: string"),
        "expected (string) annotation parens stripped:\n{dts}"
    );
    assert!(
        dts.contains("count: number"),
        "expected (number) annotation parens stripped:\n{dts}"
    );
    assert!(
        dts.contains("flag: boolean"),
        "expected (boolean) annotation parens stripped:\n{dts}"
    );
    assert!(
        !dts.contains("x: (string)") && !dts.contains("count: (number)"),
        "no annotation parens should survive in output:\n{dts}"
    );
}

/// Union type in annotation position: parens stripped.
/// Adjacent case: different union shapes (2-member, 3-member, with null/undefined).
#[test]
fn parenthesized_union_annotation_is_stripped() {
    let Some(dts) = emit_dts(
        "union",
        r#"
export var a: (string | number);
export var b: (boolean | null | undefined);
export var c: (string | number | boolean);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("a: string | number"),
        "expected 2-member union parens stripped:\n{dts}"
    );
    assert!(
        dts.contains("b: boolean | null | undefined"),
        "expected 3-member union parens stripped:\n{dts}"
    );
    assert!(
        !dts.contains("a: (string | number)"),
        "no parenthesized union annotation should survive:\n{dts}"
    );
}

/// Function parameter and return type annotations: parens stripped in both positions.
/// Adjacent case: multiple parameters, each with parenthesized type.
#[test]
fn parenthesized_annotation_stripped_in_function_signature() {
    let Some(dts) = emit_dts(
        "function_sig",
        r#"
export declare function f(x: (string | number), y: (boolean)): (null | undefined);
export declare function g(a: (string)): (number);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("x: string | number"),
        "expected param parens stripped:\n{dts}"
    );
    assert!(
        dts.contains("y: boolean"),
        "expected param parens stripped:\n{dts}"
    );
    assert!(
        dts.contains("): null | undefined"),
        "expected return type parens stripped:\n{dts}"
    );
    assert!(
        dts.contains("a: string"),
        "expected second function param parens stripped:\n{dts}"
    );
}

/// Array element: parens are stripped from the element type but re-added when
/// the structural inner type requires them for precedence (union, function, conditional).
#[test]
fn parenthesized_array_element_gets_structural_parens() {
    let Some(dts) = emit_dts(
        "array_element",
        r#"
export var a: (string | number)[];
export var b: (string extends number ? true : false)[];
export var c: ((x: string) => void)[];
export var d: (string)[];
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    // Union and conditional need parens in array element position.
    assert!(
        dts.contains("a: (string | number)[]"),
        "union in array element keeps parens:\n{dts}"
    );
    assert!(
        dts.contains("b: (string extends number ? true : false)[]"),
        "conditional in array element keeps parens:\n{dts}"
    );
    assert!(
        dts.contains("c: ((x: string) => void)[]"),
        "function type in array element keeps parens:\n{dts}"
    );
    // Plain string needs no parens in array element.
    assert!(
        dts.contains("d: string[]"),
        "primitive type in array element has no parens:\n{dts}"
    );
    assert!(
        !dts.contains("((string | number))"),
        "no double parens on union array element:\n{dts}"
    );
}

/// Union members: function types need parens; simple types do not.
/// Adjacent case: different precedence types within the union.
#[test]
fn parenthesized_union_member_gets_structural_parens_for_function() {
    let Some(dts) = emit_dts(
        "union_member",
        r#"
export var a: ((x: string) => void) | number;
export var b: ((new (x: string) => void)) | number;
export var c: (string | number) | boolean;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("((x: string) => void) | number"),
        "function type in union member needs parens:\n{dts}"
    );
    assert!(
        dts.contains("((new (x: string) => void)) | number")
            || dts.contains("(new (x: string) => void) | number"),
        "constructor type in union member needs parens:\n{dts}"
    );
    // Already-flat union member does not get extra parens.
    assert!(!dts.contains("((("), "no triple parens anywhere:\n{dts}");
}

/// Intersection arms: union types need parens; simple types do not.
#[test]
fn parenthesized_intersection_arm_gets_structural_parens_for_union() {
    let Some(dts) = emit_dts(
        "intersection_arm",
        r#"
export var a: (string | number) & object;
export var b: (string extends number ? true : false) & object;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("(string | number) & object"),
        "union in intersection arm keeps parens:\n{dts}"
    );
    assert!(
        dts.contains("(string extends number ? true : false) & object"),
        "conditional in intersection arm keeps parens:\n{dts}"
    );
}

/// Conditional type positions: conditional in extends-type keeps parens;
/// function type in check-type keeps parens; simple types do not.
#[test]
fn parenthesized_conditional_type_structural_positions() {
    let Some(dts) = emit_dts(
        "conditional",
        r#"
export type A<T, U, V> = T extends (U extends V ? string : number) ? true : false;
export type B = (<T>() => T) extends number ? true : false;
export type C = string extends (number | boolean) ? true : false;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("T extends (U extends V ? string : number) ? true : false"),
        "conditional in extends-type position keeps parens:\n{dts}"
    );
    assert!(
        dts.contains("(<T>() => T) extends number ? true : false"),
        "function type in check-type position keeps parens:\n{dts}"
    );
    // Union in check_type position — union needs parens there too.
    assert!(
        dts.contains("string extends (number | boolean) ? true : false")
            || dts.contains("string extends number | boolean ? true : false"),
        "union in extends-type or check-type position:\n{dts}"
    );
}
