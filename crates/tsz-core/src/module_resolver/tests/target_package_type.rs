//! Tests for the structural rule "external package file probing uses the
//! TARGET package's `package.json#type`, not the importer's".
//!
//! These cover the bug class fixed by removing the resolver's
//! `current_package_type` shared state in favour of explicitly threading
//! the target package's type into every probe inside
//! [`ModuleResolver::resolve_package`] and friends. The rule:
//!
//! > In Node16/NodeNext, when an external `node_modules/<pkg>` package's
//! > `main`, `types`, `typesVersions`, exports/imports, or extensionless
//! > subpath fallback is being probed, the `(.mts vs .cts vs .ts)`
//! > priority is taken from the TARGET package's own `package.json#type`.
//! > The importer's ESM/CJS mode is irrelevant once the resolver has
//! > stepped inside the target package.
//!
//! Variants tested:
//! 1. ESM importer + CJS target main field: must prefer `.cts`/`.d.cts`
//!    even though the importer is ESM.
//! 2. CJS importer + ESM target main field: must prefer `.mts`/`.d.mts`
//!    even though the importer is CJS.
//! 3. Renaming an importer ESM → CJS without touching the target keeps
//!    the resolved file stable (proves the choice depends on target, not
//!    importer).
//! 4. The same rule extends to `types` field resolution.

use super::super::*;
use std::fs;

/// Standard Node16 resolver options used by every test in this module.
/// Every test in this file exercises the `Node16`/`NodeNext` extension-
/// priority rule, so the resolver and emitter both need to be in Node16
/// mode; factoring this out keeps each test focused on its target-package
/// shape rather than option boilerplate.
fn node16_options() -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// ESM importer reaches into a CJS target whose `main` is extensionless.
/// Pre-fix tsz used the importer's ESM extension order
/// (`.mts`/`.d.mts` first), so a sibling `lib/index.mts` placeholder would
/// have shadowed the real `lib/index.cts`. The rule says the target's
/// `package.json` decides, and this target declares no `"type"` (CJS by
/// default), so `.cts`/`.d.cts` must win.
#[test]
fn esm_importer_cjs_target_main_picks_cts_first() {
    let dir = std::env::temp_dir().join("tsz_target_pt_esm_importer_cjs_main");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg/lib")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    // Target package is CJS (no "type" field → defaults to CommonJS in
    // Node16). Its main is extensionless, so the resolver must probe
    // extensions. Both .mts and .cts files exist; tsc picks .cts.
    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{ "name": "pkg", "main": "./lib/index" }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/lib/index.mts"),
        "export declare const which: 'mts';",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/lib/index.cts"),
        "export declare const which: 'cts';",
    )
    .unwrap();

    // ESM importer (package marks src as ESM).
    fs::write(dir.join("src/package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("src/app.ts"), "import { which } from 'pkg';").unwrap();

    let mut resolver = ModuleResolver::new(&node16_options());
    let resolved = resolver
        .resolve("pkg", &dir.join("src/app.ts"), Span::new(0, 5))
        .expect("pkg should resolve");

    assert!(
        resolved.resolved_path.ends_with("lib/index.cts"),
        "ESM importer reaching into a CJS target must still pick the \
         target's CJS extension priority (.cts), got {}",
        resolved.resolved_path.display(),
    );

    let _ = fs::remove_dir_all(&dir);
}

/// CJS importer reaches into an ESM target whose `main` is extensionless.
/// Pre-fix tsz used the importer's CJS extension order, so a sibling
/// `lib/index.cts` placeholder would have shadowed the real
/// `lib/index.mts`. With the target's `"type":"module"` driving the
/// probe, `.mts`/`.d.mts` must come first.
#[test]
fn cjs_importer_esm_target_main_picks_mts_first() {
    let dir = std::env::temp_dir().join("tsz_target_pt_cjs_importer_esm_main");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg/lib")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{ "name": "pkg", "type": "module", "main": "./lib/index" }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/lib/index.mts"),
        "export declare const which: 'mts';",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/lib/index.cts"),
        "export declare const which: 'cts';",
    )
    .unwrap();

    // CJS importer (no type=module in src, .cts extension forces CJS).
    fs::write(dir.join("src/app.cts"), "import { which } from 'pkg';").unwrap();

    let mut resolver = ModuleResolver::new(&node16_options());
    let resolved = resolver
        .resolve("pkg", &dir.join("src/app.cts"), Span::new(0, 5))
        .expect("pkg should resolve");

    assert!(
        resolved.resolved_path.ends_with("lib/index.mts"),
        "CJS importer reaching into an ESM target must still pick the \
         target's ESM extension priority (.mts), got {}",
        resolved.resolved_path.display(),
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Two importers (one ESM, one CJS) in the same project; the target
/// package's resolution must be IDENTICAL because the target's
/// `package.json#type` is what drives probing — not the importer mode.
/// This is the cross-row leakage symptom from the originating issue:
/// pre-fix, swapping the importer would shadow one extension with the
/// other and the cache would echo the wrong file across files.
#[test]
fn target_resolution_is_independent_of_importer_mode() {
    let dir = std::env::temp_dir().join("tsz_target_pt_importer_independence");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg/lib")).unwrap();
    fs::create_dir_all(dir.join("esm_src")).unwrap();
    fs::create_dir_all(dir.join("cjs_src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{ "name": "pkg", "type": "module", "main": "./lib/index" }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/lib/index.mts"),
        "export declare const x: 'mts';",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/lib/index.cts"),
        "export declare const x: 'cts';",
    )
    .unwrap();

    // ESM importer side (type=module).
    fs::write(dir.join("esm_src/package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("esm_src/app.ts"), "import { x } from 'pkg';").unwrap();

    // CJS importer side (no type, .cts file).
    fs::write(dir.join("cjs_src/app.cts"), "import { x } from 'pkg';").unwrap();

    let mut resolver = ModuleResolver::new(&node16_options());
    let esm_resolved = resolver
        .resolve("pkg", &dir.join("esm_src/app.ts"), Span::new(0, 5))
        .expect("pkg resolves from ESM importer");
    let cjs_resolved = resolver
        .resolve("pkg", &dir.join("cjs_src/app.cts"), Span::new(0, 5))
        .expect("pkg resolves from CJS importer");

    assert_eq!(
        esm_resolved.resolved_path,
        cjs_resolved.resolved_path,
        "Target package resolution must depend on the target's package.json#type, \
         not the importer's mode. ESM importer got {}, CJS importer got {}",
        esm_resolved.resolved_path.display(),
        cjs_resolved.resolved_path.display(),
    );
    assert!(
        esm_resolved.resolved_path.ends_with("lib/index.mts"),
        "ESM target should resolve to .mts regardless of importer, got {}",
        esm_resolved.resolved_path.display(),
    );

    let _ = fs::remove_dir_all(&dir);
}

/// The `types` field falls back to the same extension-priority pass, so
/// the target-package-type rule extends to it. CJS target with a
/// declaration-less extensionless `types` entry must prefer `.d.cts` over
/// `.d.mts` even when imported from an ESM file.
#[test]
fn esm_importer_cjs_target_types_field_picks_d_cts_first() {
    let dir = std::env::temp_dir().join("tsz_target_pt_types_field");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("node_modules/pkg/types")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(
        dir.join("node_modules/pkg/package.json"),
        r#"{ "name": "pkg", "types": "./types/index" }"#,
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/types/index.d.mts"),
        "export declare const x: 'mts';",
    )
    .unwrap();
    fs::write(
        dir.join("node_modules/pkg/types/index.d.cts"),
        "export declare const x: 'cts';",
    )
    .unwrap();

    fs::write(dir.join("src/package.json"), r#"{"type":"module"}"#).unwrap();
    fs::write(dir.join("src/app.ts"), "import { x } from 'pkg';").unwrap();

    let mut resolver = ModuleResolver::new(&node16_options());
    let resolved = resolver
        .resolve("pkg", &dir.join("src/app.ts"), Span::new(0, 5))
        .expect("pkg should resolve via types field");

    assert!(
        resolved.resolved_path.ends_with("types/index.d.cts"),
        "ESM importer + CJS target should still pick .d.cts for the \
         target's types field, got {}",
        resolved.resolved_path.display(),
    );

    let _ = fs::remove_dir_all(&dir);
}
