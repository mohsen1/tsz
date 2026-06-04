//! `@types/<mangled>` fallback for runtime packages that ship no declarations.
//!
//! TypeScript's `DefinitelyTyped` convention: typings for `@scope/name` live in
//! `@types/scope__name` (the `/` collapses to `__`), and typings for a plain
//! `name` live in `@types/name`. tsc probes the `@types` package whenever the
//! runtime package itself yields no declaration — whether because the package
//! resolves to an untyped JavaScript file or because its `exports` map has no
//! typed entry. An `@types` declaration is preferred over the untyped JS, but a
//! package's *own* declarations always win over `@types`.
//!
//! Regression coverage for the scoped-`@types` name-mangling bug where a runtime
//! `@scope/name` package (e.g. `@babel/core`) hid the installed
//! `@types/scope__name` declarations and surfaced a false TS7016.

use super::super::*;
use super::fixtures::TempFixture;

fn bundler_resolver() -> ModuleResolver {
    ModuleResolver::new(&ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Bundler),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        ..Default::default()
    })
}

/// A scoped runtime package whose `exports` map points only at JavaScript must
/// still pick up its `@types/scope__name` declarations.
#[test]
fn scoped_runtime_js_package_falls_back_to_mangled_types() {
    let fx = TempFixture::new();
    fx.write(
        "node_modules/@acme/widget/package.json",
        r#"{"name":"@acme/widget","version":"1.0.0","exports":{".":"./lib/index.js"}}"#,
    );
    fx.write(
        "node_modules/@acme/widget/lib/index.js",
        "module.exports = {};",
    );
    fx.write(
        "node_modules/@types/acme__widget/package.json",
        r#"{"name":"@types/acme__widget","version":"0.0.0","types":"index.d.ts"}"#,
    );
    let dts = fx.write(
        "node_modules/@types/acme__widget/index.d.ts",
        "export declare function widget(): string;",
    );
    let importer = fx.write("src/app.ts", "import { widget } from '@acme/widget';");

    let mut resolver = bundler_resolver();
    let resolved = resolver
        .resolve("@acme/widget", &importer, Span::new(0, 12))
        .expect("@acme/widget should resolve to @types/acme__widget declarations");
    assert_eq!(resolved.resolved_path, dts);
    assert_eq!(resolved.extension, ModuleExtension::Dts);
}

/// The same fallback must apply to a plain (non-scoped) runtime package.
#[test]
fn plain_runtime_js_package_falls_back_to_types() {
    let fx = TempFixture::new();
    fx.write(
        "node_modules/leftpadish/package.json",
        r#"{"name":"leftpadish","version":"1.0.0","exports":{".":"./dist/index.js"}}"#,
    );
    fx.write(
        "node_modules/leftpadish/dist/index.js",
        "module.exports = {};",
    );
    fx.write(
        "node_modules/@types/leftpadish/package.json",
        r#"{"name":"@types/leftpadish","version":"0.0.0","types":"index.d.ts"}"#,
    );
    let dts = fx.write(
        "node_modules/@types/leftpadish/index.d.ts",
        "export declare function pad(): string;",
    );
    let importer = fx.write("src/app.ts", "import { pad } from 'leftpadish';");

    let mut resolver = bundler_resolver();
    let resolved = resolver
        .resolve("leftpadish", &importer, Span::new(0, 10))
        .expect("leftpadish should resolve to @types/leftpadish declarations");
    assert_eq!(resolved.resolved_path, dts);
    assert_eq!(resolved.extension, ModuleExtension::Dts);
}

/// A runtime package that ships its *own* declarations must keep them; the
/// `@types` package is only a fallback and must not override real typings.
#[test]
fn own_declarations_win_over_types_package() {
    let fx = TempFixture::new();
    fx.write(
        "node_modules/@acme/gadget/package.json",
        r#"{"name":"@acme/gadget","version":"1.0.0","types":"index.d.ts","main":"lib/index.js"}"#,
    );
    let own_dts = fx.write(
        "node_modules/@acme/gadget/index.d.ts",
        "export declare function gadget(): number;",
    );
    fx.write(
        "node_modules/@acme/gadget/lib/index.js",
        "module.exports = {};",
    );
    fx.write(
        "node_modules/@types/acme__gadget/package.json",
        r#"{"name":"@types/acme__gadget","version":"0.0.0","types":"index.d.ts"}"#,
    );
    fx.write(
        "node_modules/@types/acme__gadget/index.d.ts",
        "export declare function gadget(): string;",
    );
    let importer = fx.write("src/app.ts", "import { gadget } from '@acme/gadget';");

    let mut resolver = bundler_resolver();
    let resolved = resolver
        .resolve("@acme/gadget", &importer, Span::new(0, 12))
        .expect("@acme/gadget should resolve to its own declarations");
    assert_eq!(resolved.resolved_path, own_dts);
    assert_eq!(resolved.extension, ModuleExtension::Dts);
}

/// With no `@types` package present, a runtime JS package still resolves to the
/// JavaScript file (preserving `allowJs`/TS7016 behavior in the checker).
#[test]
fn js_only_package_without_types_keeps_javascript() {
    let fx = TempFixture::new();
    fx.write(
        "node_modules/@acme/plain/package.json",
        r#"{"name":"@acme/plain","version":"1.0.0","exports":{".":"./lib/index.js"}}"#,
    );
    let js = fx.write(
        "node_modules/@acme/plain/lib/index.js",
        "module.exports = {};",
    );
    let importer = fx.write("src/app.ts", "import '@acme/plain';");

    let mut resolver = bundler_resolver();
    let resolved = resolver
        .resolve("@acme/plain", &importer, Span::new(0, 11))
        .expect("@acme/plain should still resolve to its JavaScript file");
    assert_eq!(resolved.resolved_path, js);
    assert_eq!(resolved.extension, ModuleExtension::Js);
}

/// The `@types` fallback keeps walking ancestor `node_modules` even when the
/// nearest runtime copy resolved to untyped JavaScript: a nested JS-only
/// package picks up `@types` declarations hoisted to an ancestor.
#[test]
fn types_fallback_reaches_ancestor_when_nearest_is_js_only() {
    let fx = TempFixture::new();
    // Nearest copy: untyped JS only, nested beside the importer.
    fx.write(
        "app/node_modules/leftpadish/package.json",
        r#"{"name":"leftpadish","version":"1.0.0","exports":{".":"./index.js"}}"#,
    );
    fx.write(
        "app/node_modules/leftpadish/index.js",
        "module.exports = {};",
    );
    // Declarations hoisted to the workspace-root `@types`.
    fx.write(
        "node_modules/@types/leftpadish/package.json",
        r#"{"name":"@types/leftpadish","version":"0.0.0","types":"index.d.ts"}"#,
    );
    let dts = fx.write(
        "node_modules/@types/leftpadish/index.d.ts",
        "export declare function pad(): string;",
    );
    let importer = fx.write("app/src/app.ts", "import { pad } from 'leftpadish';");

    let mut resolver = bundler_resolver();
    let resolved = resolver
        .resolve("leftpadish", &importer, Span::new(0, 10))
        .expect("ancestor @types/leftpadish should type the nested JS-only copy");
    assert_eq!(resolved.resolved_path, dts);
    assert_eq!(resolved.extension, ModuleExtension::Dts);
}

/// Node resolution stops at the nearest package: a nested untyped-JS copy is
/// kept even when a *same-named* typed copy exists in an ancestor
/// `node_modules` (the ancestor package must not win).
#[test]
fn nearest_js_package_is_not_overridden_by_ancestor_same_named_package() {
    let fx = TempFixture::new();
    // Nearest copy: untyped JS only.
    fx.write(
        "app/node_modules/widgetlib/package.json",
        r#"{"name":"widgetlib","version":"1.0.0","exports":{".":"./index.js"}}"#,
    );
    let nested_js = fx.write(
        "app/node_modules/widgetlib/index.js",
        "module.exports = {};",
    );
    // Ancestor copy of the SAME package, this one with declarations.
    fx.write(
        "node_modules/widgetlib/package.json",
        r#"{"name":"widgetlib","version":"1.0.0","types":"index.d.ts"}"#,
    );
    fx.write(
        "node_modules/widgetlib/index.d.ts",
        "export declare function w(): string;",
    );
    let importer = fx.write("app/src/app.ts", "import 'widgetlib';");

    let mut resolver = bundler_resolver();
    let resolved = resolver
        .resolve("widgetlib", &importer, Span::new(0, 9))
        .expect("nearest widgetlib should resolve (to its JS), not the ancestor copy");
    assert_eq!(resolved.resolved_path, nested_js);
    assert_eq!(resolved.extension, ModuleExtension::Js);
}

/// A scoped `@types` package is reachable even when no runtime package is
/// installed under the original scoped name.
#[test]
fn scoped_types_resolves_without_runtime_package() {
    let fx = TempFixture::new();
    fx.write(
        "node_modules/@types/acme__only/package.json",
        r#"{"name":"@types/acme__only","version":"0.0.0","types":"index.d.ts"}"#,
    );
    let dts = fx.write(
        "node_modules/@types/acme__only/index.d.ts",
        "export declare function only(): string;",
    );
    let importer = fx.write("src/app.ts", "import { only } from '@acme/only';");

    let mut resolver = bundler_resolver();
    let resolved = resolver
        .resolve("@acme/only", &importer, Span::new(0, 11))
        .expect("@acme/only should resolve to @types/acme__only declarations");
    assert_eq!(resolved.resolved_path, dts);
    assert_eq!(resolved.extension, ModuleExtension::Dts);
}
