use super::fs::{FileDiscoveryOptions, discover_ts_files};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_cli_fs_test_{}_{}", std::process::id(), nanos));
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create parent directory");
    }
    std::fs::write(path, contents).expect("failed to write file");
}

fn to_relative(base: &Path, files: &[PathBuf]) -> Vec<String> {
    files
        .iter()
        .map(|path| {
            path.strip_prefix(base)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

#[test]
fn discover_files_defaults_exclude_common_dirs() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("src/a.ts"), "export const a = 1;");
    write_file(&base.join("src/b.tsx"), "export const b = <div />;");
    write_file(&base.join("node_modules/skip.ts"), "export {};");
    write_file(&base.join("dist/skip.ts"), "export {};");

    let options = FileDiscoveryOptions {
        base_dir: base.to_path_buf(),
        files: Vec::new(),
        include: None,
        exclude: None,
        out_dir: Some(PathBuf::from("dist")),
        follow_links: false,
    };

    let files = discover_ts_files(&options).expect("should discover files");
    let relative = to_relative(base, &files);

    assert_eq!(relative, vec!["src/a.ts", "src/b.tsx"]);
}

#[test]
fn discover_files_with_include_exclude() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("src/keep.ts"), "export const keep = 1;");
    write_file(&base.join("src/ignore.spec.ts"), "export const skip = 1;");
    write_file(&base.join("src/nested/skip.ts"), "export const skip = 2;");
    write_file(&base.join("scripts/tool.ts"), "export const tool = 1;");

    let options = FileDiscoveryOptions {
        base_dir: base.to_path_buf(),
        files: Vec::new(),
        include: Some(vec!["src/**/*.ts".to_string()]),
        exclude: Some(vec!["**/*.spec.ts".to_string(), "src/nested".to_string()]),
        out_dir: None,
        follow_links: false,
    };

    let files = discover_ts_files(&options).expect("should discover files");
    let relative = to_relative(base, &files);

    assert_eq!(relative, vec!["src/keep.ts"]);
}

#[test]
fn discover_files_includes_explicit_files() {
    let temp = TempDir::new().expect("temp dir");
    let base = &temp.path;

    write_file(&base.join("src/explicit.ts"), "export const x = 1;");
    write_file(&base.join("src/other.ts"), "export const y = 2;");

    let options = FileDiscoveryOptions {
        base_dir: base.to_path_buf(),
        files: vec![PathBuf::from("src/explicit.ts")],
        include: Some(vec!["src/**/*.ts".to_string()]),
        exclude: Some(vec!["src/**".to_string()]),
        out_dir: None,
        follow_links: false,
    };

    let files = discover_ts_files(&options).expect("should discover files");
    let relative = to_relative(base, &files);

    assert_eq!(relative, vec!["src/explicit.ts"]);
}

#[test]
fn discover_files_missing_explicit_file_errors() {
    let temp = TempDir::new().expect("temp dir");

    let options = FileDiscoveryOptions {
        base_dir: temp.path.clone(),
        files: vec![PathBuf::from("missing.ts")],
        include: None,
        exclude: None,
        out_dir: None,
        follow_links: false,
    };

    let err = discover_ts_files(&options).expect_err("missing file should error");
    let message = err.to_string();
    assert!(message.contains("file not found"), "{message}");
}

#[cfg(unix)]
#[test]
fn discover_files_follow_links_when_enabled() {
    use std::os::unix::fs::symlink;

    let base = TempDir::new().expect("temp dir");
    let external = TempDir::new().expect("external dir");

    write_file(&external.path.join("linked.ts"), "export const linked = 1;");
    let link_path = base.path.join("linked");
    symlink(&external.path, &link_path).expect("create symlink");

    let mut options = FileDiscoveryOptions {
        base_dir: base.path.clone(),
        files: Vec::new(),
        include: Some(vec!["linked/**/*.ts".to_string()]),
        exclude: None,
        out_dir: None,
        follow_links: false,
    };

    let files = discover_ts_files(&options).expect("discover without links");
    assert!(files.is_empty(), "expected no files without follow_links");

    options.follow_links = true;
    let files = discover_ts_files(&options).expect("discover with links");
    let expected = std::fs::canonicalize(external.path.join("linked.ts"))
        .unwrap_or_else(|_| external.path.join("linked.ts"));
    assert_eq!(files, vec![expected]);
}
