//! A value-position class reference (`typeof C`) is always a valid `instanceof`
//! right-hand side, because a class value carries a construct signature. Under
//! whole-module load tsz can resolve the RHS class *value* to the class
//! INSTANCE type (an object with no construct signature) when the same class was
//! used in TYPE position earlier in the module — the deferred `Lazy(classDef)`
//! then reads the already-populated instance-type cache. `check_instanceof_operator`
//! recognizes the value-position class reference directly and accepts it, keyed
//! off the RHS being a class *value* reference (its declared type is a `Lazy` for
//! a `Class`/`ClassConstructor` definition) — NOT a structural sniff of the
//! (mis-)resolved shape, and never accepting a class *instance* operand.
//!
//! The emergent whole-module witness is the `zod` canary row (`deepPartialify`'s
//! `schema instanceof ZodObject` / `instanceof ZodArray`): it does not reduce to
//! a minimal fixture (the deferral-to-`Lazy` requires the full self-referential
//! generic-class module), so the cases below lock in the fix's *contract* — a
//! class value reference is accepted, a class instance value is rejected —
//! rather than reproducing the emergent resolution state. Driven through the
//! real binary with the embedded lib.

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
        path.push(format!("tsz_instanceof_class_rhs_{name}_{nanos}"));
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

/// Run `tsz --strict --noEmit` over the given `(filename, contents)` files,
/// type-checking `entry`. Returns combined stdout+stderr.
fn run_tsz(name: &str, files: &[(&str, &str)], entry: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    for (fname, contents) in files {
        std::fs::write(temp.path.join(fname), contents).expect("write file");
    }
    let output = Command::new(tsz_bin)
        .args([entry, "--strict", "--noEmit", "--pretty", "false"])
        .env("TSZ_USE_EMBEDDED_LIBS", "1")
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

/// Positive: a generic class used in TYPE position (populating the instance-type
/// cache) and then referenced in VALUE position as an `instanceof` RHS must not
/// emit TS2359 — the RHS denotes `typeof C`, which has a construct signature.
#[test]
fn class_value_rhs_after_type_position_use_is_valid_instanceof() {
    let src = r#"
class Box<T> {
  value!: T;
  unwrap(): T { return this.value; }
}
// type-position uses populate the instance-type cache for `Box`
function make(): Box<number> { return new Box(); }
let held: Box<string> | undefined;
// value-position use: `Box` is `typeof Box` here, a valid instanceof RHS
function check(x: unknown): x is Box<unknown> {
  return x instanceof Box;
}
"#;
    let Some(out) = run_tsz("value_after_type", &[("main.ts", src)], "main.ts") else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS2359"),
        "a value-position class reference is a valid `instanceof` RHS; got:\n{out}"
    );
}

/// Positive, renamed binders: the acceptance keys off the class *definition*, not
/// any identifier text, so arbitrary class/type-parameter names behave the same.
#[test]
fn class_value_rhs_instanceof_is_binder_name_independent() {
    let src = r#"
class Widget<Elem> {
  slot!: Elem;
  read(): Elem { return this.slot; }
}
function build(): Widget<boolean> { return new Widget(); }
function guard(v: unknown): boolean {
  return v instanceof Widget;
}
"#;
    let Some(out) = run_tsz("renamed_binders", &[("main.ts", src)], "main.ts") else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS2359"),
        "value-position class-ref acceptance must be binder-name independent; got:\n{out}"
    );
}

/// Positive, cross-file: an imported class used in both type and value position
/// across files is still a valid `instanceof` RHS.
#[test]
fn imported_class_value_rhs_is_valid_instanceof() {
    let shape = r#"
export class Shape<T> {
  data!: T;
  get(): T { return this.data; }
}
"#;
    let main = r#"
import { Shape } from "./shape";
function first(): Shape<number> { return new Shape(); }
function narrow(x: unknown): boolean {
  return x instanceof Shape;
}
"#;
    let Some(out) = run_tsz(
        "cross_file",
        &[("shape.ts", shape), ("main.ts", main)],
        "main.ts",
    ) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS2359"),
        "an imported class value reference is a valid `instanceof` RHS; got:\n{out}"
    );
}

/// Negative control: a class *instance* value is NOT a valid `instanceof` RHS
/// (it has no construct signature). The fix is positional — it accepts a class
/// value reference, never a class instance — so this must still emit TS2359,
/// matching tsc 7.0.2.
#[test]
fn class_instance_value_is_not_valid_instanceof_rhs() {
    let src = r#"
class Box<T> {
  value!: T;
}
function bad(x: unknown, inst: Box<number>): void {
  if (x instanceof inst) {
    void x;
  }
}
"#;
    let Some(out) = run_tsz("instance_negative", &[("main.ts", src)], "main.ts") else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        out.contains("error TS2359"),
        "a class *instance* value has no construct signature and must still be a \
         rejected `instanceof` RHS; got:\n{out}"
    );
}
