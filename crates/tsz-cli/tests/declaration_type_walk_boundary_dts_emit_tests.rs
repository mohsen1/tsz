//! DTS emit pins for the solver-owned declaration type walks (issue #13021).
//!
//! The declaration emitter used to hand-recurse `TypeData` for several
//! structural questions; those walks now live behind the solver boundary
//! (`tsz_solver::type_queries::declaration_walks`) with lazy-`DefId`
//! callbacks. Each test pins the exact `.d.ts` text for one converted walk
//! family so the boundary refactor stays behavior-identical:
//!
//! - mapped-type containment through lazy alias bodies
//!   (`contains_mapped_type_through_lazy`),
//! - conditional-alias application detection and reduction
//!   (`contains_conditional_alias_application_through_lazy`,
//!   `rebuild_with_reduced_alias_applications`),
//! - function-local alias def collection
//!   (`collect_lazy_application_base_defs_matching`),
//! - generic-callee recovery guard (`has_generic_call_signature`),
//! - lazy def-body display classification
//!   (`lazy_body_resolves_for_declaration_display`).

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
        path.push(format!("tsz_declaration_walk_boundary_dts_{name}_{nanos}"));
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

/// Compile `source` with declaration emit and return the generated `.d.ts`
/// text. Returns `None` when the tsz binary is unavailable (lets the test
/// self-skip).
fn emit_dts(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let _ = Command::new(tsz_bin)
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

    Some(std::fs::read_to_string(temp.path.join("repro.d.ts")).unwrap_or_default())
}

#[track_caller]
fn assert_dts(name: &str, source: &str, expected: &str) {
    let Some(dts) = emit_dts(name, source) else {
        println!("skipping: tsz binary unavailable");
        return;
    };
    assert_eq!(dts.trim_end(), expected.trim_end(), "fixture: {name}");
}

/// Pin: an applied alias whose body contains a mapped type is expanded only
/// per the preserve policy backed by `contains_mapped_type_through_lazy`,
/// while a plain object alias application keeps its name.
#[test]
fn mapped_alias_application_dts_surface_is_stable() {
    assert_dts(
        "mapped_alias",
        r#"type Boxed<T> = { [K in keyof T]: { value: T[K] } };
declare function box<T>(value: T): Boxed<T>;
export const boxed = box({ count: 1, label: "x" });
type Pair<T> = { first: T; second: T };
declare function pair<T>(value: T): Pair<T>;
export const paired = pair(2);
"#,
        r#"type Boxed<T> = {
    [K in keyof T]: {
        value: T[K];
    };
};
export declare const boxed: Boxed<{
    count: number;
    label: string;
}>;
type Pair<T> = {
    first: T;
    second: T;
};
export declare const paired: Pair<number>;
export {};"#,
    );
}

/// Pin: conditional-alias applications in inferred declarations are reduced
/// through `rebuild_with_reduced_alias_applications` and detected through
/// `contains_conditional_alias_application_through_lazy`.
#[test]
fn conditional_alias_application_dts_reduction_is_stable() {
    assert_dts(
        "conditional_alias",
        r#"type IsString<T> = T extends string ? "yes" : "no";
declare function probe<T>(value: T): IsString<T>;
export const verdict = probe("hello");
declare function probeAll<T>(values: T[]): Array<IsString<T>>;
export const verdicts = probeAll([42]);
"#,
        r#"type IsString<T> = T extends string ? "yes" : "no";
export declare const verdict: string;
export declare const verdicts: "no"[];
export {};"#,
    );
}

/// Pin: function-local type aliases applied in inferred return types are
/// collected through `collect_lazy_application_base_defs_matching` and elided
/// rather than referenced by their (module-invisible) names.
#[test]
fn function_local_alias_defs_dts_elision_is_stable() {
    assert_dts(
        "local_alias",
        r#"export function makeCounter() {
    type State<T> = { current: T };
    const state: State<number> = { current: 0 };
    return state;
}
export function makeUnion() {
    type Wrapped<T> = { inner: T };
    const w: Wrapped<string> | null = { inner: "a" };
    return w;
}
"#,
        r"export declare function makeCounter(): number | /*elided*/ any;
export declare function makeUnion(): string | /*elided*/ any | null;",
    );
}

/// Pin: inferred declarations for generic call results stay concrete; the
/// structural recovery path guards un-instantiated generic callees through
/// `has_generic_call_signature`.
#[test]
fn generic_callee_recovery_dts_guard_is_stable() {
    assert_dts(
        "generic_call",
        r#"declare function identity<T>(value: T): T;
export const chosen = identity({ flag: true });
declare const overloaded: { <T>(x: T): T[] };
export const wrapped = overloaded("s");
"#,
        r"export declare const chosen: {
    flag: boolean;
};
export declare const wrapped: string[];",
    );
}

/// Pin: type-predicate signature text (`type_predicate_text` helper family)
/// for declared guards and inferred arrow-function guards.
#[test]
fn type_predicate_text_dts_surface_is_stable() {
    assert_dts(
        "predicate_text",
        r#"interface Animal { kind: string }
interface Fish extends Animal { swims: true }
export function isFish(pet: Animal): pet is Fish {
    return pet.kind === "fish";
}
export const stringCheck = (value: unknown): value is string => typeof value === "string";
"#,
        r"interface Animal {
    kind: string;
}
interface Fish extends Animal {
    swims: true;
}
export declare function isFish(pet: Animal): pet is Fish;
export declare const stringCheck: (value: unknown) => value is string;
export {};",
    );
}

/// Pin: binding-pattern declaration types (`js_emit_binding_types` helper
/// family) for object destructuring with defaults and tuple destructuring
/// with optional elements.
#[test]
fn binding_pattern_dts_types_are_stable() {
    assert_dts(
        "binding_types",
        r"declare const settings: { retries: number; verbose?: boolean };
export const { retries, verbose = false } = settings;
declare const coords: [number, string, boolean?];
export const [x, y, flag] = coords;
",
        r"export declare const retries: number, verbose: boolean | undefined;
export declare const x: number, y: string, flag: boolean | undefined;",
    );
}

/// Pin: lazy alias bodies classified by
/// `lazy_body_resolves_for_declaration_display` (union, keyof/index access,
/// template literal) keep their printable named surfaces.
#[test]
fn lazy_body_display_classification_dts_is_stable() {
    assert_dts(
        "lazy_kinds",
        r#"type Mode = "draft" | "final";
declare function currentMode(): Mode;
export const mode = currentMode();
interface Config { host: string; port: number }
declare function readKey(): keyof Config;
export const key = readKey();
type Greeting = `hello ${string}`;
declare function greet(): Greeting;
export const greeting = greet();
"#,
        r#"type Mode = "draft" | "final";
export declare const mode: Mode;
interface Config {
    host: string;
    port: number;
}
export declare const key: keyof Config;
type Greeting = `hello ${string}`;
export declare const greeting: Greeting;
export {};"#,
    );
}
