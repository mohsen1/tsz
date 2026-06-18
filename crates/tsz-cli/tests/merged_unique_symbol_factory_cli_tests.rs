//! Issue #13855 (cross-file driver coverage): a `const X = Symbol.for(...)`
//! name-merged with `type X = typeof X`, imported into another module, must keep
//! its `unique symbol` value identity so a consumer's object literal `{ [X]: v }`
//! interns the same symbol-keyed member the provider's `interface I { [X](): T }`
//! declares — not a wide `[k: symbol]: V` index signature.
//!
//! This exercises the real multi-file driver pipeline (the cross-arena
//! value-declaration delegation path, `type_of_value_declaration_with_mode`),
//! which the in-crate `tsz-checker` unit harness cannot model because it does
//! not install the driver's global symbol-file index. Binder names are varied
//! from the ts-pattern fixture so nothing keys on the identifier text.

use std::path::PathBuf;
use std::process::Command;

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

#[test]
fn merged_symbol_factory_const_keeps_unique_member_across_files() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");

    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "strict": true,
    "target": "es2017",
    "module": "esnext",
    "moduleResolution": "bundler",
    "noEmit": true
  },
  "include": ["*.ts"]
}"#,
    )
    .expect("write tsconfig.json");
    std::fs::write(
        dir.path().join("tokens.ts"),
        "export const sigil = Symbol.for('@demo/sigil');\nexport type sigil = typeof sigil;\n",
    )
    .expect("write tokens.ts");
    std::fs::write(
        dir.path().join("consumer.ts"),
        "import { sigil } from './tokens';\n\
         export interface Sigiled { [sigil](): number; }\n\
         export const build = (): Sigiled => ({ [sigil]: () => 1 });\n\
         const lit = { [sigil]: () => 1 };\n\
         export const s: Sigiled = lit;\n",
    )
    .expect("write consumer.ts");

    let output = Command::new(tsz_bin)
        .args(["--noEmit", "--pretty", "false", "-p", "tsconfig.json"])
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && !stdout.contains("error TS"),
        "cross-file name-merged Symbol.for const must keep its unique-symbol member \
         identity (no TS2322/TS2345 index-signature degradation).\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
