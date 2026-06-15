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
fn imported_infer_result_array_satisfies_imported_readonly_alias_constraint() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");

    std::fs::write(
        dir.path().join("list.ts"),
        r#"
export type Seq<Item = any> = readonly Item[];
"#,
    )
    .expect("write list.ts");
    std::fs::write(
        dir.path().join("length.ts"),
        r#"
import { Seq } from "./list";
export type Size<Items extends Seq> = Items["length"];
"#,
    )
    .expect("write length.ts");
    std::fs::write(
        dir.path().join("split.ts"),
        r#"
type Split<Text extends string> =
    string[] extends infer Parts
        ? Parts
        : never;
export type Pieces<Text extends string> = Split<Text>;
"#,
    )
    .expect("write split.ts");
    std::fs::write(
        dir.path().join("main.ts"),
        r#"
import { Size } from "./length";
import { Pieces } from "./split";

export type Use<Text extends string> = Size<Pieces<Text>>;
"#,
    )
    .expect("write main.ts");

    let output = Command::new(tsz_bin)
        .args(["--noEmit", "--pretty", "false", "main.ts"])
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "imported infer result `string[]` should satisfy imported readonly-array alias constraint.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
