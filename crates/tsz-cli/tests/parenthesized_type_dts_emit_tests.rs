//! DTS emit for `ParenthesizedType` nodes (`declFileTypeAnnotationParenType`).
//!
//! tsc 6.0 preserves source-level parenthesized types verbatim in declaration
//! output: `var x: (string)` stays `(string)` and `var a: (string | number)`
//! stays `(string | number)`.  Earlier tsz code stripped the outer parens
//! unconditionally (producing `var x: string`), which did not match tsc.
//!
//! In structural positions (array element, union member, intersection arm,
//! conditional check/extends), the surrounding context calls `peel_paren` and
//! then re-adds parens only when operator precedence requires it.  That path
//! does not go through the `PARENTHESIZED_TYPE` arm in `emit_type`, so no
//! double-parens occur.

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

/// Primary repro: parenthesized primitive in a variable annotation is preserved verbatim.
/// Adjacent case: different primitive types prove the rule is not spelling-dependent.
#[test]
fn parenthesized_primitive_annotation_is_preserved() {
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
        dts.contains("x: (string)"),
        "expected (string) annotation parens preserved:\n{dts}"
    );
    assert!(
        dts.contains("count: (number)"),
        "expected (number) annotation parens preserved:\n{dts}"
    );
    assert!(
        dts.contains("flag: (boolean)"),
        "expected (boolean) annotation parens preserved:\n{dts}"
    );
}

/// Union type in annotation position: parens preserved verbatim.
/// Adjacent case: different union shapes (2-member, 3-member, with null/undefined).
#[test]
fn parenthesized_union_annotation_is_preserved() {
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
        dts.contains("a: (string | number)"),
        "expected 2-member union parens preserved:\n{dts}"
    );
    assert!(
        dts.contains("b: (boolean | null | undefined)"),
        "expected 3-member union parens preserved:\n{dts}"
    );
    assert!(
        dts.contains("c: (string | number | boolean)"),
        "expected 3-member union parens preserved:\n{dts}"
    );
}

/// Function parameter and return type annotations: parens preserved in all positions.
/// Adjacent case: multiple parameters, each with parenthesized type.
#[test]
fn parenthesized_annotation_preserved_in_function_signature() {
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
        dts.contains("x: (string | number)"),
        "expected param parens preserved:\n{dts}"
    );
    assert!(
        dts.contains("y: (boolean)"),
        "expected param parens preserved:\n{dts}"
    );
    assert!(
        dts.contains("): (null | undefined)"),
        "expected return type parens preserved:\n{dts}"
    );
    assert!(
        dts.contains("a: (string)"),
        "expected second function param parens preserved:\n{dts}"
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

/// Function type in direct variable annotation: parens stripped.
/// Adjacent cases: arrow function, constructor, generic call signature.
#[test]
fn parenthesized_function_type_annotation_is_stripped() {
    let Some(dts) = emit_dts(
        "fn_annotation",
        r#"
export var f: (() => string);
export var g: ((x: number) => boolean);
export var h: ((a: string, b: number) => void);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("f: () => string"),
        "expected paren-wrapped arrow function annotation stripped:\n{dts}"
    );
    assert!(
        dts.contains("g: (x: number) => boolean"),
        "expected paren-wrapped single-param function annotation stripped:\n{dts}"
    );
    assert!(
        dts.contains("h: (a: string, b: number) => void"),
        "expected paren-wrapped multi-param function annotation stripped:\n{dts}"
    );
    assert!(
        !dts.contains("f: (() => string)"),
        "no outer parens should remain on function type annotation:\n{dts}"
    );
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

#[test]
fn parenthesized_conditional_mapped_type_operands_keep_parens() {
    let Some(dts) = emit_dts(
        "conditional_mapped",
        r#"
export type T0<T> = ({ [K in keyof T]: ; }) extends ({ [P in keyof T]: T[P]; }) ? number : never;
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("({\n    [K in keyof T]: ;\n}) extends ({\n    [P in keyof T]: T[P];\n}) ? number : never"),
        "mapped type operands in conditional positions need parens:\n{dts}"
    );
}

#[test]
fn mapped_type_indexed_access_value_keeps_object_operand_parens() {
    let Some(dts) = emit_dts(
        "mapped_indexed_access",
        r#"
export type Clone<T> = {
    [P in keyof (T & {})]: (T & {})[P];
};
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("[P in keyof (T & {})]: (T & {})[P];"),
        "indexed access object operand in mapped type value needs parens:\n{dts}"
    );
    assert!(
        !dts.contains("T & {}[P]"),
        "indexed access must not collapse into an unparenthesized intersection:\n{dts}"
    );
}
