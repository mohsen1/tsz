use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
        path.push(format!("tsz_recursive_accumulator_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write repro file");
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

#[test]
fn recursive_defaulted_template_accumulator_reports_ts2589_at_use_site() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let temp = TempDir::new("use_site").expect("temp dir");
    write_file(
        &temp.path.join("repro.ts"),
        "type Grow<S extends string, A extends string = \"\"> = A extends infer X ? (X extends string ? Grow<S, `${A}${S}`> : never) : never;\ntype R = Grow<\"ab\">;\n",
    );

    let mut child = Command::new(tsz_bin)
        .args(["repro.ts", "--noEmit", "--pretty", "false"])
        .current_dir(&temp.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tsz repro");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().expect("poll tsz repro").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect killed tsz repro");
            panic!(
                "tsz should report TS2589 instead of hanging.\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let output = child.wait_with_output().expect("collect tsz repro output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "tsz should reject the recursive accumulator repro.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        format!("{stdout}{stderr}").contains("repro.ts(2,10): error TS2589"),
        "expected TS2589 at the concrete alias use site.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
