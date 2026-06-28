//! DTS emit: an inferred function/arrow/method return type whose body returns a
//! single enum member widens the member to its parent enum (issue #14763).
//!
//! tsc's `getReturnTypeFromBody` runs the aggregated return type through
//! `getWidenedType`, which widens a fresh enum-member literal (`return E.A`) to
//! the parent enum (`E`), exactly as a fresh primitive literal widens to its
//! base (`return "x"` → `string`). Const initializers (`const v = E.A`) and
//! explicit return annotations (`(): E.A`) are NOT widened by tsc, and a reverse
//! mapping (`E[0]`, type `string`) and a non-fresh enum-typed parameter
//! passthrough must be preserved as-is.
//!
//! Binder names are varied across cases so the widening cannot be keyed on any
//! particular enum/member identifier (anti-hardcoding gate).

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
        path.push(format!("tsz_inferred_enum_return_dts_{name}_{nanos}"));
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

fn assert_line(dts: &str, needle: &str) {
    assert!(
        dts.lines().any(|line| line.trim() == needle),
        "expected a line `{needle}` in declaration output:\n{dts}"
    );
}

fn assert_no_line(dts: &str, needle: &str) {
    assert!(
        dts.lines().all(|line| line.trim() != needle),
        "did not expect a line `{needle}` in declaration output:\n{dts}"
    );
}

/// Function declaration, arrow initializer, method, and getter all widen a
/// single returned enum member to the parent enum. Const enum behaves the same.
#[test]
fn single_enum_member_return_widens_to_parent_enum() {
    let Some(dts) = emit_dts(
        "single_member",
        r#"
enum Color { Red, Green }
const enum Mode { On, Off }
export function f() { return Color.Red; }
export const g = () => Color.Green;
export function cf() { return Mode.On; }
export const ca = () => Mode.Off;
export class K {
    m() { return Color.Green; }
    get accessor() { return Color.Red; }
}
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    // Inferred returns widen the member to the parent enum.
    assert_line(&dts, "export declare function f(): Color;");
    assert_line(&dts, "export declare const g: () => Color;");
    assert_line(&dts, "export declare function cf(): Mode;");
    assert_line(&dts, "export declare const ca: () => Mode;");
    assert_line(&dts, "m(): Color;");
    assert_line(&dts, "get accessor(): Color;");

    // The narrow member-qualified type must not survive on any inferred return.
    assert_no_line(&dts, "export declare function f(): Color.Red;");
    assert_no_line(&dts, "export declare const g: () => Color.Green;");
    assert_no_line(&dts, "export declare function cf(): Mode.On;");
    assert_no_line(&dts, "m(): Color.Green;");
}

/// Const initializers and explicit return annotations keep the exact member
/// type; widening is scoped to *inferred* function-like returns.
#[test]
fn const_initializer_and_explicit_annotation_keep_member_type() {
    let Some(dts) = emit_dts(
        "preserved",
        r#"
enum Suit { Hearts, Spades }
export const v = Suit.Hearts;
export function h(): Suit.Hearts { return Suit.Hearts; }
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_line(&dts, "export declare const v = Suit.Hearts;");
    assert_line(&dts, "export declare function h(): Suit.Hearts;");
}

/// A multi-member union of the same enum already collapses to the enum, and must
/// stay that way; an enum-typed parameter passthrough is non-fresh and is never
/// widened; a reverse mapping (`E[0]`, type `string`) stays `string`.
#[test]
fn union_passthrough_and_reverse_mapping_are_unchanged() {
    let Some(dts) = emit_dts(
        "edges",
        r#"
enum Dir { Up = "UP", Down = "DOWN" }
export function both(b: boolean) { return b ? Dir.Up : Dir.Down; }
export function passthrough(x: Dir.Up) { return x; }
enum Num { A, B }
export function rev() { return Num[0]; }
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    // Same-enum union — widened in both tsc and tsz.
    assert_line(&dts, "export declare function both(b: boolean): Dir;");
    // Non-fresh parameter passthrough keeps the exact member type.
    assert_line(
        &dts,
        "export declare function passthrough(x: Dir.Up): Dir.Up;",
    );
    // Reverse mapping is a `string`, never the parent enum.
    assert_line(&dts, "export declare function rev(): string;");
    assert_no_line(&dts, "export declare function rev(): Num;");
}

/// Multiple `return E.A` branches that dedup to a single member still widen to
/// the parent enum (the union collapses to one member, then widens).
#[test]
fn multiple_same_member_returns_widen() {
    let Some(dts) = emit_dts(
        "multi_return",
        r#"
enum Phase { Start, End }
export class Runner {
    run(b: boolean) {
        if (b) return Phase.Start;
        return Phase.Start;
    }
}
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert_line(&dts, "run(b: boolean): Phase;");
    assert_no_line(&dts, "run(b: boolean): Phase.Start;");
}
