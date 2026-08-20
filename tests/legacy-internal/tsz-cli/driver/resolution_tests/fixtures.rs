use super::*;

/// Build the shared options used by the pnpm-symlink resolution tests.
pub(super) fn pnpm_symlink_test_options(preserve_symlinks: bool) -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        preserve_symlinks,
        module_suffixes: vec![String::new()],
        printer: tsz::emitter::PrinterOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        checker: tsz::checker::context::CheckerOptions {
            module: ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Build a pnpm sandbox where the top-level `@types/express` is a symlink into
/// the `.pnpm` store and `@types/express-serve-static-core` exists *only* as its
/// transitive (un-hoisted) sibling inside that store. Returns `(project_dir,
/// sandbox_types_dir)`; `express/index.d.ts` is written with `express_content`.
pub(super) fn setup_pnpm_express_sandbox(
    dir_name: &str,
    express_content: &str,
) -> (PathBuf, PathBuf) {
    use std::fs;
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join(dir_name);
    let _ = fs::remove_dir_all(&dir);
    let sandbox = dir.join("node_modules/.pnpm/@types+express@5.0.6/node_modules/@types");
    fs::create_dir_all(sandbox.join("express")).unwrap();
    fs::create_dir_all(sandbox.join("express-serve-static-core")).unwrap();
    fs::write(sandbox.join("express/index.d.ts"), express_content).unwrap();
    fs::write(
        sandbox.join("express-serve-static-core/index.d.ts"),
        "export type Core = number;",
    )
    .unwrap();
    fs::create_dir_all(dir.join("node_modules/@types")).unwrap();
    symlink(
        sandbox.join("express"),
        dir.join("node_modules/@types/express"),
    )
    .unwrap();
    (dir, sandbox)
}

/// Stand up three duplicate-package copies at the given `node_modules`
/// sub-paths, each tagged with `name`/`version` so
/// `build_duplicate_package_redirects` groups them. Returns the per-copy
/// `index.d.ts` paths in input order.
pub(super) fn make_duplicate_package_copies(
    dir: &Path,
    name: &str,
    version: &str,
    relative_roots: [&str; 3],
) -> [PathBuf; 3] {
    let pkg_json = format!(r#"{{"name":"{name}","version":"{version}"}}"#);
    let mut indices: [PathBuf; 3] = Default::default();
    for (i, rel) in relative_roots.iter().enumerate() {
        let pkg_dir = dir.join(rel);
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("package.json"), &pkg_json).unwrap();
        std::fs::write(pkg_dir.join("index.d.ts"), "export {};").unwrap();
        indices[i] = pkg_dir.join("index.d.ts");
    }
    indices
}

pub(super) fn dup_pkg_resolver_options() -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_suffixes: vec![String::new()],
        ..Default::default()
    }
}
