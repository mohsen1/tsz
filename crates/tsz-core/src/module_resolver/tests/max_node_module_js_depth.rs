//! `maxNodeModuleJsDepth` (default 0) admits a resolved `node_modules` JS
//! dependency into the program when its nesting depth is within the budget;
//! once admitted the file is type-checked as real JS, so the
//! external-`require()` TS7016 ("could not find a declaration file") does not
//! fire. Oracle-verified against `typescript@7.0.2` (#16921):
//! `require("untyped")` of a `node_modules/untyped` JS file reports TS7016 at
//! the default depth of 0 but is clean at `--maxNodeModuleJsDepth 1`. This is
//! the residual of #16921 after #16926 handled the explicit-`files`-root shape.

use super::super::*;
use super::fixtures::TempFixture;

/// A `node`-resolution resolver with `allowJs` on (so a `.js` resolution takes
/// the primary `Ok(resolved_module)` branch) and the given
/// `maxNodeModuleJsDepth`.
fn resolver_with_depth(max_node_module_js_depth: u32) -> ModuleResolver {
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        module_suffixes: vec![String::new()],
        allow_js: true,
        max_node_module_js_depth,
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    };
    ModuleResolver::new(&options)
}

fn setup_untyped_package(fixture: &TempFixture) -> std::path::PathBuf {
    fixture.write(
        "node_modules/untyped/package.json",
        r#"{"name":"untyped","version":"1.0.0","main":"index.js"}"#,
    );
    fixture.write("node_modules/untyped/index.js", "module.exports = 1;")
}

fn require_request<'a>(
    specifier: &'a str,
    containing_file: &'a std::path::Path,
) -> ModuleLookupRequest<'a> {
    ModuleLookupRequest {
        specifier,
        containing_file,
        specifier_span: Span::new(0, 0),
        import_kind: ImportKind::CjsRequire,
        resolution_mode_override: None,
        no_implicit_any: true,
        implied_classic_resolution: false,
    }
}

#[test]
fn default_depth_zero_reports_ts7016() {
    let fixture = TempFixture::new();
    setup_untyped_package(&fixture);
    let containing_file = fixture.write("a.ts", "import u = require(\"untyped\");");

    let mut resolver = resolver_with_depth(0);
    let request = require_request("untyped", &containing_file);
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    let error = result
        .error
        .expect("maxNodeModuleJsDepth 0 must report TS7016 for an untyped node_modules require()");
    assert_eq!(error.code, COULD_NOT_FIND_DECLARATION_FILE);
}

#[test]
fn depth_one_admits_top_level_dependency_and_suppresses_ts7016() {
    let fixture = TempFixture::new();
    let resolved_js = setup_untyped_package(&fixture);
    let containing_file = fixture.write("a.ts", "import u = require(\"untyped\");");

    let mut resolver = resolver_with_depth(1);
    let request = require_request("untyped", &containing_file);
    let result = resolver.lookup(&request, |_, _| None, |_| false, None);

    assert_eq!(
        result.resolved_path.as_deref(),
        Some(resolved_js.as_path()),
        "the require() must still resolve to the real JS file"
    );
    assert!(
        result.error.is_none(),
        "maxNodeModuleJsDepth 1 admits the top-level dependency, so no TS7016: {:?}",
        result.error
    );
}

#[test]
fn nesting_depth_counts_node_modules_segments() {
    use std::path::Path;
    assert_eq!(node_modules_nesting_depth(Path::new("/p/src/index.js")), 0);
    assert_eq!(
        node_modules_nesting_depth(Path::new("/p/node_modules/foo/index.js")),
        1
    );
    assert_eq!(
        node_modules_nesting_depth(Path::new("/p/node_modules/foo/node_modules/bar/index.js")),
        2
    );
}
