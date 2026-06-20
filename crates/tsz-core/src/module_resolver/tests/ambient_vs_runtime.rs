//! Ambient-module precedence over an untyped on-disk runtime package.
//!
//! Owner layer: the module-resolution engine's `ModuleResolver::lookup`
//! diagnostic-code selection (`crates/tsz-core/src/module_resolver/mod.rs`).
//!
//! Structural rule: when a bare specifier is declared by an in-program ambient
//! `declare module "X"` AND the only on-disk resolution is an *untyped*
//! JavaScript file (a runtime `node_modules/X` package with no bundled or
//! `@types` declarations), `tsc` resolves the import to the ambient declaration
//! rather than reporting the module untyped (TS7016 in import form / TS6504 at
//! the root form). The in-program ambient module must take precedence over an
//! untyped on-disk package. See issue #14169 (rxjs/msw witness on node
//! builtins `events`/`punycode`/`string_decoder`).
//!
//! Adjacent cases covered (§26 generalization gate):
//!   1. Positive repro: ambient `"events"` shadows an untyped on-disk package.
//!   2. Renamed binder: the same shape with `"punycode"` — proves no
//!      fixture-name fast path.
//!   3. Negative control (no ambient): a typed on-disk package (`.d.ts`)
//!      resolves to its real path; an untyped on-disk package without any
//!      ambient declaration still reports TS7016.
//!   4. Negative control (ambient + typed): the precedence gate is keyed on the
//!      on-disk resolution being *JavaScript*, so a typed package whose primary
//!      resolution lands on a `.d.ts` keeps its real path — the ambient block
//!      never fires for typed packages.

use super::super::*;

/// Build the standard Node-resolution options used by every case here.
fn node_options() -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        module_suffixes: vec![String::new()],
        ..Default::default()
    }
}

/// Write an untyped runtime `node_modules/<name>` package (package.json + a
/// runtime `.js` entry, no declarations).
fn write_untyped_package(dir: &std::path::Path, name: &str) {
    use std::fs;
    let pkg = dir.join("node_modules").join(name);
    fs::create_dir_all(&pkg).unwrap();
    fs::write(
        pkg.join("package.json"),
        format!(r#"{{"name":"{name}","version":"3.0.0","main":"./{name}.js"}}"#),
    )
    .unwrap();
    fs::write(
        pkg.join(format!("{name}.js")),
        "module.exports = function () {};",
    )
    .unwrap();
}

/// The issue #14169 repro: an in-program `declare module "events"` plus an
/// untyped on-disk `node_modules/events/events.js`. tsc resolves to the ambient
/// declaration (clean); tsz must too — no TS7016.
#[test]
fn ambient_module_shadows_untyped_runtime_package() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_ambient_vs_runtime_events");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    write_untyped_package(&dir, "events");
    fs::write(
        dir.join("src/index.ts"),
        "import { EventEmitter } from 'events';",
    )
    .unwrap();

    let mut resolver = ModuleResolver::new(&node_options());
    let request = ModuleLookupRequest {
        specifier: "events",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(28, 36),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        // `--strict` => noImplicitAny: on main this is exactly what flips the
        // untyped JS package to TS7016.
        no_implicit_any: true,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |spec| spec == "events", None);
    let outcome = result.classify();

    assert!(
        outcome.is_resolved,
        "ambient module must be treated as resolved"
    );
    assert!(
        outcome.error.is_none(),
        "in-program ambient `declare module \"events\"` must win over the untyped \
         on-disk node_modules/events package: expected no TS7016, got {:?}",
        outcome.error,
    );
    assert!(
        outcome.resolved_path.is_none(),
        "ambient resolution carries no on-disk file path",
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Renamed-binder adjacent case (§25 anti-hardcoding): the same shape with a
/// different specifier name must behave identically. No fixture-name fast path.
#[test]
fn ambient_module_shadows_untyped_runtime_package_renamed() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_ambient_vs_runtime_punycode");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    write_untyped_package(&dir, "punycode");
    fs::write(
        dir.join("src/index.ts"),
        "import { decode } from 'punycode';",
    )
    .unwrap();

    let mut resolver = ModuleResolver::new(&node_options());
    let request = ModuleLookupRequest {
        specifier: "punycode",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(23, 33),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    };
    let result = resolver.lookup(&request, |_, _| None, |spec| spec == "punycode", None);
    let outcome = result.classify();

    assert!(outcome.is_resolved, "ambient module must be resolved");
    assert!(
        outcome.error.is_none(),
        "ambient `declare module \"punycode\"` must shadow the untyped package: {:?}",
        outcome.error,
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Negative control: with NO in-program ambient declaration, an untyped on-disk
/// runtime package must STILL report TS7016. The fix must not blanket-suppress
/// the untyped-JS diagnostic — it only yields to a matching ambient module.
#[test]
fn untyped_runtime_package_without_ambient_still_reports_ts7016() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_ambient_vs_runtime_no_ambient");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    write_untyped_package(&dir, "events");
    fs::write(dir.join("src/index.ts"), "import 'events';").unwrap();

    let mut resolver = ModuleResolver::new(&node_options());
    let request = ModuleLookupRequest {
        specifier: "events",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(8, 16),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    };
    // No specifier matches the ambient closure.
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);
    let outcome = result.classify();

    assert!(outcome.is_resolved, "untyped JS still resolves (no TS2307)");
    let error = outcome
        .error
        .expect("untyped package without ambient declaration must keep TS7016");
    assert_eq!(
        error.code, COULD_NOT_FIND_DECLARATION_FILE,
        "expected TS7016 for the untyped on-disk package",
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Negative control: a TYPED on-disk package (one shipping `.d.ts`) must resolve
/// to its real declaration path even when an ambient declaration of the same
/// name exists. The precedence gate fires only for an *untyped JavaScript*
/// on-disk resolution; a typed package's primary resolution lands on a `.d.ts`,
/// so it keeps its real path and the on-disk types are used.
#[test]
fn ambient_does_not_shadow_typed_on_disk_package() {
    use std::fs;
    let dir = std::env::temp_dir().join("tsz_ambient_vs_runtime_typed");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    let pkg = dir.join("node_modules/typedpkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(
        pkg.join("package.json"),
        r#"{"name":"typedpkg","version":"1.0.0","main":"./index.js","types":"./index.d.ts"}"#,
    )
    .unwrap();
    fs::write(pkg.join("index.js"), "module.exports = {};").unwrap();
    fs::write(
        pkg.join("index.d.ts"),
        "export declare function greet(name: string): string;",
    )
    .unwrap();
    fs::write(
        dir.join("src/index.ts"),
        "import { greet } from 'typedpkg';",
    )
    .unwrap();

    let mut resolver = ModuleResolver::new(&node_options());
    let request = ModuleLookupRequest {
        specifier: "typedpkg",
        containing_file: &dir.join("src/index.ts"),
        specifier_span: Span::new(23, 33),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    };
    // Even with an ambient match for the same name, the typed package wins.
    let result = resolver.lookup(&request, |_, _| None, |spec| spec == "typedpkg", None);
    let outcome = result.classify();

    assert!(outcome.is_resolved, "typed package must resolve");
    assert!(
        outcome.error.is_none(),
        "typed package has declarations: no TS7016 expected, got {:?}",
        outcome.error,
    );
    let resolved = outcome
        .resolved_path
        .expect("typed package must resolve to a real on-disk declaration path");
    assert!(
        resolved.ends_with("index.d.ts"),
        "typed package must resolve to its `.d.ts`, not be shadowed by the ambient \
         declaration: got {}",
        resolved.display(),
    );

    let _ = fs::remove_dir_all(&dir);
}
