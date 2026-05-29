//! DTS emit for generic calls whose source return type is a *simple* type-
//! parameter surface (a bare type parameter, or an array of one).
//!
//! Structural rule: when a generic call's source return annotation is a bare
//! type-parameter reference (`T`) or an array of one (`U[]`), and the checker's
//! canonical type for the call is fully resolved (it contains no `infer`
//! placeholders and no free type parameters), declaration emit serializes that
//! canonical type via the type printer instead of reconstructing it by
//! substituting type parameters into the source return text. The legacy text
//! path mangled member values containing `<`, `>`, or `,` (e.g. a reverse-
//! mapped `unboxify(x)` emitting `a: number>, b: Box<string[]`).
//!
//! Composite returns (intersection `D & M`, union `T | U`, application
//! `Foo<T>`) and unresolved-conditional/`infer` returns intentionally keep the
//! text path, so their tsc-faithful source structure is preserved.
//!
//! These cases vary type-parameter names, member spellings, and member-value
//! generic shapes so the coverage proves the rule rather than a single
//! spelling. They invoke the full `tsz` binary so the checker's node-type
//! cache (which the fix consults) is populated.

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
        path.push(format!("tsz_bare_tp_dts_{name}_{nanos}"));
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
            "--strict",
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
// Bare type-parameter return resolving to an object: no reflow mangling
// =============================================================================

/// Primary repro (mappedTypesArraysTuples): `unboxify<T>(x: Boxified<T>): T`
/// reverse-mapped inference whose argument members carry nested generic values.
/// The inferred declaration must render the resolved object members, not the
/// mangled comma/angle-bracket reflow `a: number>, b: Box<string[]`.
#[test]
fn reverse_mapped_object_members_render_without_reflow_mangling() {
    let Some(dts) = emit_dts(
        "unboxify",
        r#"
type Box<T> = { value: T };
type Boxified<T> = { [P in keyof T]: Box<T[P]> };
declare function unboxify<T>(x: Boxified<T>): T;
declare let src: { a: Box<number>, b: Box<string[]> };
export let dst = unboxify(src);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        !dts.contains("number>,"),
        "member-value reflow must not leak `number>,`:\n{dts}"
    );
    assert!(
        !dts.contains("Box<string[];"),
        "member-value reflow must not split nested generics:\n{dts}"
    );
    assert!(
        dts.contains("a: number;"),
        "expected resolved member `a: number;`:\n{dts}"
    );
    assert!(
        dts.contains("b: string[];"),
        "expected resolved member `b: string[];`:\n{dts}"
    );
}

/// Same rule, renamed type parameter (`E`/`K`) and different member spellings
/// (`first`/`second`). Renaming the bound variables must not change the
/// outcome — proving the fix is keyed on structure, not on the name `T`/`P`.
#[test]
fn reverse_mapped_renamed_type_parameter_still_renders_resolved_members() {
    let Some(dts) = emit_dts(
        "unwrap_renamed",
        r#"
type Wrap<E> = { value: E };
type Wrapped<E> = { [K in keyof E]: Wrap<E[K]> };
declare function unwrap<E>(x: Wrapped<E>): E;
declare let boxes: { first: Wrap<number[]>, second: Wrap<boolean> };
export let plain = unwrap(boxes);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        !dts.contains("number[]>,"),
        "renamed reverse-mapped case must not leak reflow mangling:\n{dts}"
    );
    assert!(
        dts.contains("first: number[];"),
        "expected resolved member `first: number[];`:\n{dts}"
    );
    assert!(
        dts.contains("second: boolean;"),
        "expected resolved member `second: boolean;`:\n{dts}"
    );
}

// =============================================================================
// Array of a bare type parameter (`U[]`) uses resolved element type
// =============================================================================

/// Primary repro (genericFunctions2): `map<T, U>(items: T[], f: (x: T) => U):
/// U[]`. The inferred declaration must use the resolved element type
/// (`number[]`), not lose `U` and collapse to `any[]`.
#[test]
fn array_of_type_parameter_return_uses_resolved_element_type() {
    let Some(dts) = emit_dts(
        "map_lengths",
        r#"
declare function map<T, U>(items: T[], f: (x: T) => U): U[];
declare var names: string[];
export let lengths = map(names, x => x.length);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("lengths: number[]"),
        "expected resolved array element type `number[]`:\n{dts}"
    );
    assert!(
        !dts.contains("lengths: any[]"),
        "array-of-type-parameter return must not collapse to `any[]`:\n{dts}"
    );
}

/// Adjacent shape: a different result element type (`boolean[]`) and a renamed
/// type parameter (`A`/`B`) prove the rule is not tied to `number`/`U`.
#[test]
fn array_of_renamed_type_parameter_uses_resolved_element_type() {
    let Some(dts) = emit_dts(
        "map_flags",
        r#"
declare function project<A, B>(items: A[], f: (x: A) => B): B[];
declare var nums: number[];
export let flags = project(nums, x => x > 0);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("flags: boolean[]"),
        "expected resolved array element type `boolean[]`:\n{dts}"
    );
    assert!(
        !dts.contains("flags: any[]"),
        "renamed array-of-type-parameter return must not collapse to `any[]`:\n{dts}"
    );
}

// =============================================================================
// Bare type-parameter return resolving to a primitive
// =============================================================================

/// Primary repro (recursiveTypeReferences1): `head<T>(a: Nest<T>): T` where the
/// argument's declared type is a recursive alias. The inferred declaration
/// emits the resolved bare type parameter (`string`), not the argument alias.
#[test]
fn bare_type_parameter_return_resolves_to_primitive_not_argument_alias() {
    let Some(dts) = emit_dts(
        "head_nest",
        r#"
type Nest<T> = T | Nest<T>[];
declare function head<T>(a: Nest<T>): T;
declare let xs: Nest<string>;
export let h = head(xs);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("h: string"),
        "expected resolved bare type parameter `string`:\n{dts}"
    );
    assert!(
        !dts.contains("h: Nest<string>"),
        "bare type-parameter return must not reuse the argument alias:\n{dts}"
    );
}

// =============================================================================
// Negative / fallback: composite returns keep the text path
// =============================================================================

/// A composite intersection return (`D & M`) must NOT be routed through the
/// canonical printer (which would merge/reorder the intersection). tsc
/// preserves the `&` source structure, so the emitted type keeps the `&`
/// rather than a single merged object literal.
#[test]
fn intersection_return_preserves_source_intersection_structure() {
    let Some(dts) = emit_dts(
        "combine_intersection",
        r#"
declare function combine<D, M>(a: D, b: M): D & M;
declare let left: { x: number };
declare let right: { run(): void };
export let r = combine(left, right);
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    // The intersection surface is preserved as two operands joined by `&`,
    // not flattened/reordered into a single merged object literal.
    assert!(
        dts.contains('&'),
        "composite intersection return must keep its `&` structure:\n{dts}"
    );
    assert!(
        dts.contains("r: { x: number } & { run(): void }")
            || dts.contains("x: number") && dts.contains("run(): void") && dts.contains('&'),
        "intersection members must both survive on the `&` surface:\n{dts}"
    );
}
