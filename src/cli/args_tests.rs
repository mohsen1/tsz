use clap::Parser;

use super::args::{CliArgs, Module, Target};

#[test]
fn parses_defaults() {
    let args = CliArgs::try_parse_from(["tsz"]).expect("default args should parse");

    assert_eq!(args.target, None);
    assert_eq!(args.module, None);
    assert!(args.out_dir.is_none());
    assert!(args.project.is_none());
    assert!(!args.strict);
    assert!(!args.no_emit);
    assert!(!args.watch);
    assert!(args.files.is_empty());
}

#[test]
fn parses_common_flags() {
    let args = CliArgs::try_parse_from([
        "tsz",
        "--target",
        "es2020",
        "--module",
        "commonjs",
        "--outDir",
        "dist",
        "--project",
        "configs/tsconfig.json",
        "--strict",
        "--noEmit",
        "--watch",
        "src/index.ts",
    ])
    .expect("flagged args should parse");

    assert_eq!(args.target, Some(Target::Es2020));
    assert_eq!(args.module, Some(Module::CommonJs));
    assert_eq!(args.out_dir.as_deref(), Some(std::path::Path::new("dist")));
    assert_eq!(
        args.project.as_deref(),
        Some(std::path::Path::new("configs/tsconfig.json"))
    );
    assert!(args.strict);
    assert!(args.no_emit);
    assert!(args.watch);
    assert_eq!(args.files, vec![std::path::PathBuf::from("src/index.ts")]);
}
