//! Tests for the structural rule "any types-flavored conditional key
//! propagates declaration-aware probing to its nested target".
//!
//! tsc treats a conditional `exports`/`imports` key as a TypeScript types
//! lookup when its base name is `"types"`, including the versioned variant
//! `"types@<range>"`. The matched target then goes through declaration-
//! aware probing (`try_types_entry`) instead of the runtime probe
//! (`try_export_target`). Under Node16/NodeNext the runtime probe is
//! intentionally strict and refuses to add extensions to an extensionless
//! target, so without the flavor classification a legitimate versioned-
//! types branch resolving to `"./types/v5/api"` would fail to find
//! `./types/v5/api.d.ts` and the next condition would silently take over.

use super::super::exports_imports::is_types_condition_key;
use super::super::*;
use super::fixtures::TempFixture;

fn node16_options() -> ResolvedCompilerOptions {
    ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node16),
        resolve_package_json_exports: true,
        module_suffixes: vec![String::new()],
        printer: crate::emitter::PrinterOptions {
            module: crate::emitter::ModuleKind::Node16,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Build a `node_modules/pkg` package whose `./api` subpath has a custom
/// `exports` block, write the named declaration target plus a fallback
/// `dist/api.js`, write an importer that asks for `pkg/api`, and return
/// the resolved path so each test can assert which branch won.
///
/// The exports block is interpolated as `{ <api_exports>, "default": "./dist/api.js" }`
/// where `<api_exports>` is the test-supplied JSON fragment. This keeps
/// every variant focused on the structural axis (key flavor + target
/// extensionless-ness) instead of repeating boilerplate.
fn resolve_pkg_api(api_exports: &str, target_rel: &str) -> std::path::PathBuf {
    let fx = TempFixture::new();
    fx.write(
        "node_modules/pkg/package.json",
        &format!(
            r#"{{
              "name": "pkg",
              "exports": {{
                "./api": {{ {api_exports}, "default": "./dist/api.js" }}
              }}
            }}"#
        ),
    );
    fx.write(
        format!("node_modules/pkg/{target_rel}"),
        "export declare const api: unknown;",
    );
    fx.write("node_modules/pkg/dist/api.js", "module.exports = {};");
    fx.write("src/app.ts", "import { api } from 'pkg/api';");

    let mut resolver = ModuleResolver::new(&node16_options());
    resolver
        .resolve("pkg/api", &fx.join("src/app.ts"), Span::new(0, 9))
        .expect("pkg/api must resolve")
        .resolved_path
}

/// Pure-function table for the classifier. The brief in §26 requires
/// covering at least two name choices for any structural axis the fix
/// reads; this table varies (a) base name spelling (`types` vs other
/// well-known conditions) and (b) versioned vs unversioned, so a renamed
/// range or a non-types versioned key cannot accidentally pass.
#[test]
fn classifier_recognizes_types_flavors_only() {
    for (key, expected, why) in [
        ("types", true, "canonical types condition"),
        ("types@>=5.0", true, "versioned types with one range"),
        (
            "types@>=4.7",
            true,
            "versioned types with a different range",
        ),
        ("node", false, "well-known non-types condition"),
        ("node@>=18", false, "versioned non-types condition"),
        ("import", false, "module condition"),
        ("default", false, "fallback condition"),
        (
            "typings",
            false,
            "typings is a top-level field, not a condition",
        ),
        ("", false, "empty key"),
    ] {
        assert_eq!(
            is_types_condition_key(key),
            expected,
            "is_types_condition_key({key:?}) — {why}"
        );
    }
}

/// End-to-end: a versioned-types branch with an extensionless target
/// must probe declaration extensions. Pre-fix the resolver only set
/// `is_types_condition = true` for keys literally spelled `"types"`,
/// so the versioned-types branch routed through `try_export_target`
/// and the Node16/NodeNext "no extension probing on runtime targets"
/// guard dropped the otherwise valid `.d.ts` lookup.
#[test]
fn versioned_types_extensionless_target_resolves_to_d_ts() {
    let resolved = resolve_pkg_api(r#""types@>=5.0": "./types/v5/api""#, "types/v5/api.d.ts");
    assert!(
        resolved.ends_with("types/v5/api.d.ts"),
        "versioned types@>=5.0 extensionless target should resolve to \
         declaration sibling, got {}",
        resolved.display(),
    );
}

/// Renaming the range from `>=5.0` to `>=4.7` must not change the
/// outcome. If the fix were fingerprinting a specific range spelling,
/// this would regress. Combined with the classifier table this proves
/// the rule reads the BASE of the key, never a specific range.
#[test]
fn versioned_types_range_spelling_is_irrelevant() {
    let resolved = resolve_pkg_api(r#""types@>=4.7": "./types/api""#, "types/api.d.ts");
    assert!(
        resolved.ends_with("types/api.d.ts"),
        "renaming the range must not affect the types-flavor \
         classification, got {}",
        resolved.display(),
    );
}

/// Nested conditional inside a versioned-types outer key: the outer
/// `"types@<range>"` must mark its inner conditional as types-flavored
/// so an extensionless inner target still goes through declaration-
/// aware probing. Without the BASE-flavor classifier, the outer
/// `types@<range>` failed to set `is_types_condition`, the inner branch
/// reverted to the runtime probe, and the extensionless target was
/// dropped.
#[test]
fn versioned_types_propagates_flavor_into_nested_conditional() {
    let resolved = resolve_pkg_api(
        r#""types@>=5.0": { "node": "./node-types/api", "default": "./node-types/api" }"#,
        "node-types/api.d.ts",
    );
    assert!(
        resolved.ends_with("node-types/api.d.ts"),
        "versioned types@>=5.0 must propagate types flavor into its \
         inner conditional, got {}",
        resolved.display(),
    );
}

/// Negative: a non-types versioned condition like `"node@>=18"` must
/// NOT receive declaration-aware probing. Even when a `.d.ts` sibling
/// for an extensionless target sits on disk, the runtime branch must
/// refuse to add the extension under Node16/NodeNext, so resolution
/// falls through to `"default"`. Without this guard the classifier
/// would be over-broad and any versioned condition would silently
/// flavor as types.
#[test]
fn versioned_non_types_branch_falls_through_to_default() {
    let resolved = resolve_pkg_api(
        // `runtime/api.d.ts` exists on disk but the `node@>=18` branch
        // must not pick it up extensionlessly.
        r#""node@>=18": "./runtime/api""#,
        "runtime/api.d.ts",
    );
    // `default` points at `./dist/api.js`; that path has a runtime
    // extension so `try_export_target`'s declaration substitution kicks
    // in and lands on `dist/api.d.ts` if it exists, or on `dist/api.js`.
    // What must NOT happen is `runtime/api.d.ts`.
    assert!(
        !resolved.ends_with("runtime/api.d.ts"),
        "non-types versioned condition must NOT receive declaration- \
         aware probing for extensionless targets — got {}",
        resolved.display(),
    );
}

/// Regression for the exact-key fast path in `condition_key_matches`. A
/// user-supplied `customConditions` entry can legally contain `@` (for
/// example, an ENV-tag spelling like `"custom@edge"`) and is meant to
/// match its LITERAL spelling, not be parsed as `<base>@<range>`. The
/// shared `parse_condition_key` helper would otherwise split such a key
/// and only match if its base (`"custom"`) was in `conditions`, which
/// silently regresses any package whose subpath uses an exact custom
/// condition spelling.
#[test]
fn custom_condition_with_at_sign_matches_literally() {
    let fx = TempFixture::new();
    fx.write(
        "node_modules/pkg/package.json",
        r#"{
          "name": "pkg",
          "exports": {
            "./api": {
              "custom@edge": "./edge/api.d.ts",
              "default": "./dist/api.js"
            }
          }
        }"#,
    );
    fx.write(
        "node_modules/pkg/edge/api.d.ts",
        "export declare const api: 'edge';",
    );
    fx.write("node_modules/pkg/dist/api.js", "module.exports = {};");
    fx.write("src/app.ts", "import { api } from 'pkg/api';");

    let options = ResolvedCompilerOptions {
        custom_conditions: vec!["custom@edge".to_string()],
        ..node16_options()
    };
    let mut resolver = ModuleResolver::new(&options);
    let resolved = resolver
        .resolve("pkg/api", &fx.join("src/app.ts"), Span::new(0, 9))
        .expect("pkg/api must resolve")
        .resolved_path;

    assert!(
        resolved.ends_with("edge/api.d.ts"),
        "exact custom condition `custom@edge` must match literally even \
         though it contains `@` — got {}",
        resolved.display(),
    );
}
