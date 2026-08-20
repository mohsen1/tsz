//! Tests for the single-owner import-site ESM/CJS classifier
//! (`ModuleResolver::importing_module_kind_for_import`).
//!
//! This classifier decides whether an import site uses the `import` or
//! `require` conditional-`exports`/`imports` branch (and the matching
//! Node16/NodeNext extension priority). It is the one place every resolution
//! entry point — the primary `resolve`, the `lookup` fallback bookkeeping, the
//! TS7016 `probe_js_file`, and the CLI driver's checker resolution-mode map —
//! must agree on. These tests pin the rule structurally (by extension / import
//! kind / `module` target / `package.json#type`), never by a specific file
//! name, and protect the consistency the consolidation restored: `probe_js_file`
//! must classify a `module: preserve` import site exactly like the primary
//! resolution it probes on behalf of, instead of falling back to the bare
//! `get_importing_module_kind` (which ignores `module: preserve`).

use super::super::*;

fn resolver_with_module(module: crate::emitter::ModuleKind) -> ModuleResolver {
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        printer: crate::emitter::PrinterOptions {
            module,
            ..Default::default()
        },
        ..Default::default()
    };
    ModuleResolver::new(&options)
}

#[test]
fn dynamic_import_is_always_esm() {
    let mut resolver = resolver_with_module(crate::emitter::ModuleKind::CommonJS);
    // A `.cts` file would force CJS for a static import, but a dynamic
    // `import()` is ESM regardless of the importing file's extension.
    assert_eq!(
        resolver.importing_module_kind_for_import(
            std::path::Path::new("/proj/loader.cts"),
            ImportKind::DynamicImport,
            None,
        ),
        ImportingModuleKind::Esm
    );
}

#[test]
fn require_call_is_always_commonjs() {
    let mut resolver = resolver_with_module(crate::emitter::ModuleKind::ESNext);
    // An `.mts` file forces ESM for a static import, but a `require(...)` call
    // is CJS regardless.
    assert_eq!(
        resolver.importing_module_kind_for_import(
            std::path::Path::new("/proj/runtime.mts"),
            ImportKind::CjsRequire,
            None,
        ),
        ImportingModuleKind::CommonJs
    );
}

#[test]
fn resolution_mode_override_wins_over_everything() {
    let mut resolver = resolver_with_module(crate::emitter::ModuleKind::CommonJS);
    // Override beats a `require` import kind and a `.cts` file.
    assert_eq!(
        resolver.importing_module_kind_for_import(
            std::path::Path::new("/proj/attr.cts"),
            ImportKind::CjsRequire,
            Some(ImportingModuleKind::Esm),
        ),
        ImportingModuleKind::Esm
    );
}

#[test]
fn preserve_plain_extension_static_import_is_esm() {
    let mut resolver = resolver_with_module(crate::emitter::ModuleKind::Preserve);
    // Under `module: preserve`, an ordinary `import` from a plain-extension
    // file resolves with the `import` condition (ESM), matching tsc's
    // bundler-style mode — independent of any nearby `package.json#type`.
    for name in ["entry.ts", "widget.tsx", "data.js", "view.jsx"] {
        assert_eq!(
            resolver.importing_module_kind_for_import(
                std::path::Path::new("/proj").join(name).as_path(),
                ImportKind::EsmImport,
                None,
            ),
            ImportingModuleKind::Esm,
            "preserve static import from {name} should be ESM"
        );
    }
}

#[test]
fn preserve_respects_extension_forced_cjs_and_esm() {
    let mut resolver = resolver_with_module(crate::emitter::ModuleKind::Preserve);
    // `.cts`/`.cjs` force CJS even under preserve...
    for name in ["legacy.cts", "shim.cjs"] {
        assert_eq!(
            resolver.importing_module_kind_for_import(
                std::path::Path::new("/proj").join(name).as_path(),
                ImportKind::EsmImport,
                None,
            ),
            ImportingModuleKind::CommonJs,
            "{name} should force CJS under preserve"
        );
    }
    // ...and `.mts`/`.mjs` force ESM.
    for name in ["modern.mts", "esm.mjs"] {
        assert_eq!(
            resolver.importing_module_kind_for_import(
                std::path::Path::new("/proj").join(name).as_path(),
                ImportKind::EsmImport,
                None,
            ),
            ImportingModuleKind::Esm,
            "{name} should force ESM under preserve"
        );
    }
}

#[test]
fn preserve_plain_import_is_esm_even_in_a_commonjs_package() {
    use std::fs;
    // The consistency crux: a plain `.ts` import inside a `"type": "commonjs"`
    // (or type-less) package is ESM under `module: preserve`. The bare
    // `get_importing_module_kind` returns CJS for exactly this shape, so the
    // canonical classifier must NOT defer to it for preserve static imports —
    // otherwise the primary resolution and the TS7016 `probe_js_file` would
    // pick different export conditions for the same import site.
    let dir = std::env::temp_dir().join("tsz_importing_module_kind_preserve_cjs_pkg");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("app")).unwrap();
    fs::write(dir.join("package.json"), r#"{"type": "commonjs"}"#).unwrap();
    let importer = dir.join("app/main.ts");
    fs::write(&importer, "import { x } from 'pkg';").unwrap();

    let mut resolver = resolver_with_module(crate::emitter::ModuleKind::Preserve);

    // Bare classifier (used pre-consolidation by probe_js_file) sees the CJS
    // package and returns CommonJs — the divergence the fix removes.
    assert_eq!(
        resolver.get_importing_module_kind(&importer),
        ImportingModuleKind::CommonJs,
        "bare classifier follows package.json#type"
    );

    // Canonical classifier keeps the preserve static-import site ESM.
    assert_eq!(
        resolver.importing_module_kind_for_import(&importer, ImportKind::EsmImport, None),
        ImportingModuleKind::Esm,
        "preserve static import stays ESM regardless of CJS package.json"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn non_preserve_module_defers_to_package_type() {
    use std::fs;
    // Outside `module: preserve`, a plain-extension static import follows the
    // existing `get_importing_module_kind` rule (here: package.json type), so
    // the consolidation is behavior-preserving for the common Node16 path.
    let dir = std::env::temp_dir().join("tsz_importing_module_kind_node16_pkg_type");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("esm")).unwrap();
    fs::create_dir_all(dir.join("cjs")).unwrap();
    fs::write(dir.join("esm/package.json"), r#"{"type": "module"}"#).unwrap();
    fs::write(dir.join("cjs/package.json"), r#"{"type": "commonjs"}"#).unwrap();
    let esm_importer = dir.join("esm/a.ts");
    let cjs_importer = dir.join("cjs/b.ts");
    fs::write(&esm_importer, "import 'pkg';").unwrap();
    fs::write(&cjs_importer, "import 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::NodeNext),
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::NodeNext,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    assert_eq!(
        resolver.importing_module_kind_for_import(&esm_importer, ImportKind::EsmImport, None),
        ImportingModuleKind::Esm
    );
    assert_eq!(
        resolver.importing_module_kind_for_import(&cjs_importer, ImportKind::EsmImport, None),
        ImportingModuleKind::CommonJs
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preserve_static_import_of_require_only_js_is_ts2307_not_ts7016() {
    use std::fs;
    // End-to-end witness of the `probe_js_file` consistency fix. A package
    // exposes a JS entry ONLY via the `require` condition (no `import`, no
    // types). A static `import` from a plain `.ts` file under `module:
    // preserve` resolves with the `import` condition, which the package does
    // not satisfy → tsc reports TS2307 ("Cannot find module").
    //
    // Before the fix, the primary resolution failed under ESM but the TS7016
    // `probe_js_file` re-resolved under CommonJS (the bare classifier), matched
    // the `require`-only JS, and downgraded the diagnostic to TS7016 with the
    // module treated as resolved. The probe must classify the import site the
    // same way as the primary resolution, so the require-only JS stays
    // unreachable and the diagnostic remains TS2307.
    let dir = std::env::temp_dir().join("tsz_preserve_require_only_js_probe");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(dir.join("app")).unwrap();
    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{"name":"pkg","exports":{".":{"require":"./impl.js"}}}"#,
    )
    .unwrap();
    fs::write(dir.join("node_modules/pkg/impl.js"), "module.exports = {};").unwrap();
    let importer = dir.join("app/main.ts");
    fs::write(&importer, "import { x } from 'pkg';").unwrap();

    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Preserve,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);

    let request = ModuleLookupRequest {
        specifier: "pkg",
        containing_file: &importer,
        specifier_span: Span::new(15, 20),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert!(
        result.resolved_path.is_none() && !result.treat_as_resolved,
        "require-only JS must stay unreachable for a preserve static import"
    );
    let error = result.error.expect("should report a not-found diagnostic");
    assert_eq!(
        error.code, CANNOT_FIND_MODULE,
        "should be TS2307, not the TS7016 the divergent CJS probe produced: {}",
        error.message
    );

    let _ = fs::remove_dir_all(&dir);
}
