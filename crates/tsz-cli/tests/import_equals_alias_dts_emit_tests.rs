//! DTS emit for `import alias = Q.R.S` entity-name aliases.
//!
//! Structural rule: when a declaration-emit type references a symbol that is
//! the resolved target of an in-scope `import alias = Q.R.S` declaration, tsc
//! references that symbol by the bare alias name (rather than expanding the
//! qualified path), and the alias declaration is retained (not elided) because
//! it is used. The alias is only substituted where it is in lexical scope and
//! where the target symbol is not more directly nameable by its own local name.
//!
//! Witnesses: internalAliasClassInsideTopLevelModuleWith{out,}Export,
//! internalAliasClassInsideLocalModuleWithoutExport. Regression guard:
//! privacyTopLevelInternalReferenceImportWith{out,}Export (alias must NOT be
//! used where the target is locally nameable).
//!
//! AgentName: opus48-emit100

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
        path.push(format!("tsz_ie_alias_dts_{name}_{nanos}"));
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
            "--module",
            "commonjs",
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

/// Primary repro shape: a top-level `import alias = ns.Class` aliases a class
/// that is referenced only through an inferred type. The inferred type must
/// print as the alias name, and the alias must be retained.
#[test]
fn top_level_import_equals_alias_used_for_inferred_type() {
    let Some(dts) = emit_dts(
        "toplevel",
        r#"
export namespace x {
    export class c {
        foo(a: number) { return a; }
    }
}
import xc = x.c;
export var cProp = new xc();
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("var cProp: xc"),
        "inferred type should print as alias name `xc`, not the expanded path:\n{dts}"
    );
    assert!(
        !dts.contains("cProp: xc.c") && !dts.contains("cProp: x.c"),
        "alias target must not expand to a qualified path:\n{dts}"
    );
    assert!(
        dts.contains("import xc = x.c"),
        "the used import-equals alias must be retained:\n{dts}"
    );
}

/// Renamed alias / namespace / class: prove the rule is structural, not keyed
/// on the spelling `x`/`c`/`xc` from the primary repro.
#[test]
fn top_level_import_equals_alias_renamed_identifiers() {
    let Some(dts) = emit_dts(
        "renamed",
        r#"
export namespace Outer {
    export class Widget {
        render(n: number) { return n; }
    }
}
import W = Outer.Widget;
export var instance = new W();
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("var instance: W"),
        "inferred type should print as renamed alias `W`:\n{dts}"
    );
    assert!(
        !dts.contains("instance: W.Widget") && !dts.contains("instance: Outer.Widget"),
        "renamed alias target must not expand to a qualified path:\n{dts}"
    );
    assert!(
        dts.contains("import W = Outer.Widget"),
        "the renamed import-equals alias must be retained:\n{dts}"
    );
}

/// Nested-namespace alias: `import c = x.c` inside `m2.m3` must be retained and
/// the inferred type printed as the local alias `c`, even though a same-named
/// class `x.c` exists in another scope.
#[test]
fn nested_namespace_import_equals_alias_used_locally() {
    let Some(dts) = emit_dts(
        "nested",
        r#"
export namespace x {
    export class c {
        foo(a: number) { return a; }
    }
}
export namespace m2 {
    export namespace m3 {
        import c = x.c;
        export var cProp = new c();
        var cReturnVal = cProp.foo(10);
    }
}
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("import c = x.c"),
        "the nested-namespace import-equals alias must be retained:\n{dts}"
    );
    assert!(
        dts.contains("var cProp: c"),
        "inferred type should print as the local alias `c`:\n{dts}"
    );
    assert!(
        !dts.contains("cProp: x.c") && !dts.contains("cProp: c.c"),
        "local alias target must not expand to a qualified path:\n{dts}"
    );
}

/// Regression guard: a top-level alias must NOT be used to name a type when the
/// target is directly nameable in its own scope. Inside `m_private`, the return
/// type of `f_private` is `c_private` (the local class name), not the top-level
/// alias `im_c_private = m_private.c_private`.
#[test]
fn top_level_alias_not_used_when_target_is_locally_nameable() {
    let Some(dts) = emit_dts(
        "locally_nameable",
        r#"
export namespace m_private {
    export class c_private {}
    export function f_private() {
        return new c_private();
    }
}
export import im_c_private = m_private.c_private;
export var topUse = new im_c_private();
"#,
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };

    assert!(
        dts.contains("function f_private(): c_private"),
        "return type inside the namespace must use the local name `c_private`, \
         not the top-level alias:\n{dts}"
    );
    assert!(
        !dts.contains("f_private(): im_c_private"),
        "the same-scope alias must not shadow the local name:\n{dts}"
    );
}
