//! Project-mode parity guard for `typeof <imported class>` — kysely tracker
//! #10663 (`typeof` family), the residual inverse of the #10661 cross-file
//! instance-resolution fix.
//! Structural rule: `typeof C` is a value-space query and resolves to the
//! class's CONSTRUCTOR type, regardless of which module declares `C`. #10661
//! made a cross-file class reference in *type* position resolve to the INSTANCE
//! type (published through the shared `class_to_instance` slot). The constructor
//! (value-space) type, however, is the class def's *body*; a consuming checker's
//! per-file `symbol_types` cache is empty for a class declared in another file,
//! so a deferred `TypeQuery(SymbolRef)` over such a class resolved `None` in
//! `resolve_type_query` and the call site fell through to `resolve_lazy`, which
//! returns the INSTANCE type. That inverted constructor and instance for a
//! property typed `typeof ImportedClass`:
//!   * a false TS2322 rejecting the constructor value, and
//!   * a missing TS2739/TS2741 accepting an instance value.
//!
//! The fix resolves a class symbol's constructor from the shared
//! `DefinitionStore` body when `symbol_types` misses.
//! These cases run the real multi-file driver (shared `DefinitionStore`,
//! separate per-file arenas, real module resolution) — the faithful path for
//! cross-module resolution. The single-arena in-crate checker harness conflates
//! per-file `SymbolId`/`DefId` namespaces and cannot reproduce it.
//! The trigger needs the `typeof` to be deferred to a `TypeQuery(SymbolRef)`,
//! which happens when the property type is part of a NAMED interface/object-type
//! alias body (resolved lazily) and the class is cross-file. Binder names vary
//! across cases so the guard follows the structural shape (anti-hardcoding).

use super::compile;
use crate::args::CliArgs;
use clap::Parser;
use std::fs;
use tsz_common::diagnostics::Diagnostic;

/// Write `files` plus a strict `noEmit` tsconfig into a fresh temp dir and run
/// the project-mode compile. Returns every emitted diagnostic.
/// Write every `file` to disk, but list only `roots` in the tsconfig `files`
/// array (non-root files are still loaded through import resolution). This
/// reproduces the import-graph scenario the fix targets: a consumer file is the
/// compilation root and the class it imports is reached cross-file as a
/// `Lazy(DefId)` whose constructor lives only in the shared `DefinitionStore`
/// body — the path where `resolve_type_query`'s `resolve_ref` fallback misses
/// (or, once the #10661 instance publication populates `symbol_types`, returns
/// the instance).
fn compile_roots(files: &[(&str, &str)], roots: &[&str]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    let names: Vec<String> = roots.iter().map(|name| format!("\"{name}\"")).collect();
    let tsconfig = format!(
        r#"{{ "compilerOptions": {{ "strict": true, "target": "es2022", "lib": ["es2022"], "module": "node16", "moduleResolution": "node16", "skipLibCheck": true, "noEmit": true }}, "files": [{}] }}"#,
        names.join(", ")
    );
    fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write tsconfig");
    for (name, source) in files {
        fs::write(dir.path().join(name), source).expect("write source");
    }

    let project = dir.path().to_string_lossy().to_string();
    let args = CliArgs::try_parse_from([
        "tsz",
        "--project",
        project.as_str(),
        "--noEmit",
        "--pretty",
        "false",
    ])
    .expect("project args");
    compile(&args, dir.path())
        .expect("compile succeeds")
        .diagnostics
}

fn count_code(diags: &[Diagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

fn codes(diags: &[Diagnostic]) -> Vec<(u32, String)> {
    diags
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

const CLASS_FILE: &str = r#"
export class TediousRequest {
  readonly #handle: number;
  constructor(sql: string, cb: () => void) { this.#handle = 1; void sql; void cb; }
  run(): void {}
}
"#;

/// FP witness: the imported class *value* (a constructor) assigned to an
/// interface property typed `typeof TediousRequest` must be accepted (tsc
/// clean). Before the fix tsz resolved the target property to the INSTANCE type
/// and reported a false TS2322 ("missing #handle, run").
#[test]
fn typeof_imported_class_property_accepts_constructor_value() {
    let main = r#"
import { TediousRequest } from "./request.js";
interface DialectConfig { TediousRequest: typeof TediousRequest }
const cfg: DialectConfig = { TediousRequest };
void cfg;
"#;
    let diags = compile_roots(
        &[("request.ts", CLASS_FILE), ("main.ts", main)],
        &["main.ts"],
    );
    assert_eq!(
        count_code(&diags, 2322),
        0,
        "constructor value into `typeof ImportedClass` property must be accepted; got {:#?}",
        codes(&diags)
    );
}

/// FN witness: an *instance* assigned to a `typeof TediousRequest` property must
/// be rejected with TS2741 (the constructor's `prototype` is missing on an
/// instance) — tsc errors here. Before the fix tsz resolved the target property
/// to the instance type and silently accepted the instance.
#[test]
fn typeof_imported_class_property_rejects_instance_value() {
    let main = r#"
import { TediousRequest } from "./request.js";
interface DialectConfig { TediousRequest: typeof TediousRequest }
declare const instance: TediousRequest;
const cfg: DialectConfig = { TediousRequest: instance };
void cfg;
"#;
    let diags = compile_roots(
        &[("request.ts", CLASS_FILE), ("main.ts", main)],
        &["main.ts"],
    );
    assert_eq!(
        count_code(&diags, 2741),
        1,
        "instance into `typeof ImportedClass` property must be rejected (TS2741); got {:#?}",
        codes(&diags)
    );
}

/// Anti-hardcoding: rename every binder (class, members, interface, property,
/// module). The rule is structural (a `typeof` over a cross-file class resolves
/// to the constructor), not name-driven. Both directions are checked.
#[test]
fn typeof_imported_class_property_is_binder_name_independent() {
    let cls = r#"
export class Widget {
  private state = 0;
  build(): void { this.state += 1; }
}
"#;
    let main = r#"
import { Widget } from "./widget.js";
interface Registry { make: typeof Widget }
const reg: Registry = { make: Widget };
declare const w: Widget;
const bad: Registry = { make: w };
void reg;
void bad;
"#;
    let diags = compile_roots(&[("widget.ts", cls), ("main.ts", main)], &["main.ts"]);
    assert_eq!(
        count_code(&diags, 2322),
        0,
        "renamed-binder constructor value must be accepted; got {:#?}",
        codes(&diags)
    );
    assert_eq!(
        count_code(&diags, 2741),
        1,
        "renamed-binder instance value must be rejected (TS2741); got {:#?}",
        codes(&diags)
    );
}

/// Adjacent position: a `typeof` nested inside an interface property that is
/// itself an object type, reached through indexed access. The deferred
/// `TypeQuery` must still resolve to the constructor (so `new` works and the
/// result has instance members).
#[test]
fn typeof_imported_class_nested_object_property_resolves_constructor() {
    let main = r#"
import { TediousRequest } from "./request.js";
interface Config { ctors: { request: typeof TediousRequest } }
declare const c: Config;
const made = new c.ctors.request("sql", () => {});
made.run();
"#;
    let diags = compile_roots(
        &[("request.ts", CLASS_FILE), ("main.ts", main)],
        &["main.ts"],
    );
    assert_eq!(
        count_code(&diags, 2322) + count_code(&diags, 2741) + count_code(&diags, 2339),
        0,
        "nested `typeof ImportedClass` must resolve to the constructor; got {:#?}",
        codes(&diags)
    );
}

/// Generic imported class: `typeof G` is the generic constructor; a `new` on it
/// infers type arguments and the result exposes instance members.
#[test]
fn typeof_imported_generic_class_property_resolves_constructor() {
    let cls = r#"
export class Box<T> {
  readonly #value: T;
  constructor(value: T) { this.#value = value; }
  get(): T { return this.#value; }
}
"#;
    let main = r#"
import { Box } from "./box.js";
interface Factory { box: typeof Box }
const f: Factory = { box: Box };
const made = new f.box(123);
const n: number = made.get();
void n;
"#;
    let diags = compile_roots(&[("box.ts", cls), ("main.ts", main)], &["main.ts"]);
    assert_eq!(
        count_code(&diags, 2322) + count_code(&diags, 2741),
        0,
        "generic `typeof ImportedClass` must resolve to the constructor; got {:#?}",
        codes(&diags)
    );
}

/// Negative control: the #10661 instance resolution must still hold and the
/// `typeof` fix must not leak into plain *type* position. A cross-file class in
/// plain type position (no `typeof`) is the INSTANCE type: an instance value is
/// accepted, and the *constructor* value is rejected (TS2739, the constructor
/// lacks the instance members `#handle`/`run`).
#[test]
fn imported_class_plain_type_position_stays_instance() {
    let main = r#"
import { TediousRequest } from "./request.js";
interface Holder { req: TediousRequest }
declare const instance: TediousRequest;
const ok: Holder = { req: instance };
const bad: Holder = { req: TediousRequest };
void ok;
void bad;
"#;
    let diags = compile_roots(
        &[("request.ts", CLASS_FILE), ("main.ts", main)],
        &["main.ts"],
    );
    assert_eq!(
        count_code(&diags, 2322),
        0,
        "instance into a plain instance-typed property must be accepted (#10661); got {:#?}",
        codes(&diags)
    );
    assert_eq!(
        count_code(&diags, 2739),
        1,
        "constructor into a plain instance-typed property must be rejected (TS2739); got {:#?}",
        codes(&diags)
    );
}
