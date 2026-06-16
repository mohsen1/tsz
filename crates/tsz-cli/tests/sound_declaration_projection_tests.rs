//! End-to-end coverage for opt-in Sound Mode declaration-boundary projection
//! (issue #8533).
//!
//! When `--soundDeclarationProjection` is set, a value whose type is owned by an
//! external declaration file (here a `node_modules` package `.d.ts`) is observed
//! by sound user code with `any` projected to `unknown` in read/covariant
//! positions — function return types, readable properties, and library-supplied
//! callback parameters — while write/contravariant positions stay permissive.
//!
//! The flag is additive and off by default: plain checking and `--sound`
//! without the projection flag must report no projection diagnostics on the same
//! project, so ordinary `tsc` parity is untouched.

use std::path::{Path, PathBuf};
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
        path.push(format!("tsz_sound_projection_{name}_{nanos}"));
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

// Binder names are deliberately non-obvious: the projection is driven by
// declaration-file ownership and structural polarity, never by identifier text.
const EXTLIB_DTS: &str = "\
export declare function decode(raw: string): any;
export declare const carrier: { payload: any };
export declare function subscribe(handler: (delivered: any) => void): void;
";

const EXTLIB_PACKAGE_JSON: &str =
    "{ \"name\": \"vendorpkg\", \"version\": \"1.0.0\", \"types\": \"index.d.ts\" }";

const MAIN_TS: &str = "\
import { decode, carrier, subscribe } from \"vendorpkg\";

// Return type (read position) projects any -> unknown.
export const a: number = decode(\"x\");

// Readable property (read position) projects any -> unknown.
export const b: number = carrier.payload;

// Write position stays permissive: a value flowing INTO the library keeps any.
carrier.payload = 123;

// Library-supplied callback parameter is a read position for user code.
subscribe((delivered) => {
  const c: number = delivered;
  void c;
});
";

const TSCONFIG: &str = "\
{
  \"compilerOptions\": {
    \"strict\": true,
    \"module\": \"nodenext\",
    \"moduleResolution\": \"nodenext\",
    \"noEmit\": true,
    \"skipLibCheck\": true
  },
  \"include\": [\"main.ts\"]
}
";

fn write_fixture(root: &Path) {
    let pkg = root.join("node_modules").join("vendorpkg");
    std::fs::create_dir_all(&pkg).expect("create node_modules pkg dir");
    std::fs::write(pkg.join("index.d.ts"), EXTLIB_DTS).expect("write d.ts");
    std::fs::write(pkg.join("package.json"), EXTLIB_PACKAGE_JSON).expect("write package.json");
    std::fs::write(root.join("main.ts"), MAIN_TS).expect("write main.ts");
    std::fs::write(root.join("tsconfig.json"), TSCONFIG).expect("write tsconfig");
}

fn run(tsz_bin: &Path, root: &Path, extra: &[&str]) -> String {
    let mut cmd = Command::new(tsz_bin);
    cmd.args(["-p", "tsconfig.json", "--pretty", "false"])
        .args(extra)
        .current_dir(root);
    let output = cmd.output().expect("run tsz");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn error_count(diagnostics: &str) -> usize {
    diagnostics
        .lines()
        .filter(|line| line.contains("error TS"))
        .count()
}

/// Ordinary checking and `--sound` (without projection) must not introduce any
/// boundary diagnostics on declaration-owned `any`.
#[test]
fn projection_is_opt_in_and_off_by_default() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping sound projection test: tsz binary not found");
        return;
    };
    let temp = TempDir::new("optin").expect("temp dir");
    write_fixture(&temp.path);

    let normal = run(&tsz_bin, &temp.path, &[]);
    assert_eq!(
        error_count(&normal),
        0,
        "ordinary checking must not project declaration-boundary any; got:\n{normal}"
    );

    let sound_only = run(&tsz_bin, &temp.path, &["--sound"]);
    assert_eq!(
        error_count(&sound_only),
        0,
        "--sound without projection must not project; got:\n{sound_only}"
    );
}

/// With the flag on, read positions across the declaration boundary surface
/// `unknown`, while the write position stays permissive.
#[test]
fn projection_rewrites_read_positions_only() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping sound projection test: tsz binary not found");
        return;
    };
    let temp = TempDir::new("rewrite").expect("temp dir");
    write_fixture(&temp.path);

    let projected = run(&tsz_bin, &temp.path, &["--soundDeclarationProjection"]);

    // Exactly three read positions become `unknown`: the function return, the
    // readable property, and the library-supplied callback parameter. The
    // write `carrier.payload = 123` must NOT error (write side keeps any).
    let ts2322 = projected
        .lines()
        .filter(|line| line.contains("error TS2322"))
        .count();
    assert_eq!(
        ts2322, 3,
        "expected 3 read-position projections (return, property, callback param); got:\n{projected}"
    );
    assert_eq!(
        error_count(&projected),
        3,
        "the permissive write position must not add a diagnostic; got:\n{projected}"
    );
    assert!(
        !projected.contains("(10,"),
        "the write `carrier.payload = 123` (line 10) must not be flagged; got:\n{projected}"
    );
}
