//! Convert test directive options to tsz-server CheckOptions JSON.
//!
//! The server's legacy protocol accepts a `CheckOptions` object with camelCase
//! field names. Test directives are `HashMap<String, String>` with lowercase
//! keys and string values. This module converts between the two.

#[cfg(test)]
use serde_json::json;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Convert test directive options to a JSON object matching the server's
/// `CheckOptions` struct.
pub fn directives_to_check_options(directives: &HashMap<String, String>) -> Value {
    let mut opts = Map::new();

    for (key, value) in directives {
        let key_lower = key.to_lowercase();

        if is_harness_directive(&key_lower) {
            continue;
        }

        let Some(field_name) = directive_to_field_name(&key_lower) else {
            continue;
        };

        let json_value = if field_name == "lib" {
            let libs: Vec<Value> = value
                .split(',')
                .map(|s| Value::String(s.trim().to_lowercase()))
                .collect();
            Value::Array(libs)
        } else if value == "true" {
            Value::Bool(true)
        } else if value == "false" {
            Value::Bool(false)
        } else {
            Value::String(value.clone())
        };

        opts.insert(field_name.to_string(), json_value);
    }

    // When `target` is set but `lib` is not, inject the default lib chain.
    // The server's determine_libs returns a single lib name (e.g. "es6") that
    // doesn't resolve properly in the source tree. The CLI works because tsconfig
    // resolution follows /// <reference lib="..." /> chains. We replicate that
    // here by mapping target to the canonical lib list.
    if opts.contains_key("target") && !opts.contains_key("lib") && !opts.contains_key("noLib") {
        if let Some(Value::String(target)) = opts.get("target") {
            if let Some(libs) = default_libs_for_target(target) {
                let lib_values: Vec<Value> =
                    libs.iter().map(|s| Value::String(s.to_string())).collect();
                opts.insert("lib".to_string(), Value::Array(lib_values));
            }
        }
    }

    Value::Object(opts)
}

/// Directives that are test-harness metadata, not compiler options.
fn is_harness_directive(key: &str) -> bool {
    matches!(
        key,
        "filename"
            | "symlink"
            | "skip"
            | "nocheck"
            | "notypesandscript"
            | "declarationdir"
            | "declarationmap"
            | "emitdeclarationonly"
            | "sourcemap"
            | "inlinesourcemap"
            | "inlinesources"
            | "outfile"
            | "outdir"
            | "out"
            | "rootdir"
            | "rootdirs"
    )
}

/// Directives that are real compiler options but NOT yet supported by the
/// server's CheckOptions struct. Tests using these must fall back to CLI mode.
pub fn has_unsupported_server_options(directives: &HashMap<String, String>) -> bool {
    const UNSUPPORTED: &[&str] = &[
        "jsx",
        "jsxfactory",
        "jsximportsource",
        "jsximportfragment",
        "moduleresolution",
        "modulesuffixes",
        "paths",
        "baseurl",
        "types",
        "typeroots",
    ];
    directives
        .keys()
        .any(|k| UNSUPPORTED.contains(&k.to_lowercase().as_str()))
}

/// Map a target string to the default lib names the server should load.
///
/// The conformance test source tree uses files like `es2015.d.ts`, `es2015.core.d.ts`, etc.
/// When only `target` is specified (no `lib`), the server needs the full set of lib names
/// that the CLI would resolve via tsconfig.json reference-following.
fn default_libs_for_target(target: &str) -> Option<Vec<&'static str>> {
    let first = target
        .split(',')
        .next()
        .unwrap_or(target)
        .trim()
        .to_lowercase();
    Some(match first.as_str() {
        "es3" | "es5" => vec!["es5"],
        "es6" | "es2015" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
        ],
        "es2016" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
        ],
        "es2017" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
        ],
        "es2018" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
        ],
        "es2019" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019",
        ],
        "es2020" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019",
            "es2020.bigint",
            "es2020.date",
            "es2020.number",
            "es2020.promise",
            "es2020.sharedmemory",
            "es2020.string",
            "es2020.symbol.wellknown",
            "es2020",
        ],
        "es2021" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019",
            "es2020.bigint",
            "es2020.date",
            "es2020.number",
            "es2020.promise",
            "es2020.sharedmemory",
            "es2020.string",
            "es2020.symbol.wellknown",
            "es2020",
            "es2021.promise",
            "es2021.string",
            "es2021.weakref",
            "es2021",
        ],
        "es2022" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019",
            "es2020.bigint",
            "es2020.date",
            "es2020.number",
            "es2020.promise",
            "es2020.sharedmemory",
            "es2020.string",
            "es2020.symbol.wellknown",
            "es2020",
            "es2021.promise",
            "es2021.string",
            "es2021.weakref",
            "es2021",
            "es2022.array",
            "es2022.error",
            "es2022.object",
            "es2022.regexp",
            "es2022.string",
            "es2022",
        ],
        "es2023" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019",
            "es2020.bigint",
            "es2020.date",
            "es2020.number",
            "es2020.promise",
            "es2020.sharedmemory",
            "es2020.string",
            "es2020.symbol.wellknown",
            "es2020",
            "es2021.promise",
            "es2021.string",
            "es2021.weakref",
            "es2021",
            "es2022.array",
            "es2022.error",
            "es2022.object",
            "es2022.regexp",
            "es2022.string",
            "es2022",
            "es2023.array",
            "es2023.collection",
            "es2023",
        ],
        "es2024" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019",
            "es2020.bigint",
            "es2020.date",
            "es2020.number",
            "es2020.promise",
            "es2020.sharedmemory",
            "es2020.string",
            "es2020.symbol.wellknown",
            "es2020",
            "es2021.promise",
            "es2021.string",
            "es2021.weakref",
            "es2021",
            "es2022.array",
            "es2022.error",
            "es2022.object",
            "es2022.regexp",
            "es2022.string",
            "es2022",
            "es2023.array",
            "es2023.collection",
            "es2023",
            "es2024.arraybuffer",
            "es2024.collection",
            "es2024.object",
            "es2024.promise",
            "es2024.regexp",
            "es2024.sharedmemory",
            "es2024.string",
            "es2024",
        ],
        "es2025" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019",
            "es2020.bigint",
            "es2020.date",
            "es2020.number",
            "es2020.promise",
            "es2020.sharedmemory",
            "es2020.string",
            "es2020.symbol.wellknown",
            "es2020",
            "es2021.promise",
            "es2021.string",
            "es2021.weakref",
            "es2021",
            "es2022.array",
            "es2022.error",
            "es2022.object",
            "es2022.regexp",
            "es2022.string",
            "es2022",
            "es2023.array",
            "es2023.collection",
            "es2023",
            "es2024.arraybuffer",
            "es2024.collection",
            "es2024.object",
            "es2024.promise",
            "es2024.regexp",
            "es2024.sharedmemory",
            "es2024.string",
            "es2024",
            "es2025.collection",
            "es2025",
        ],
        "esnext" | "latest" => vec![
            "es5",
            "es2015.core",
            "es2015.collection",
            "es2015.generator",
            "es2015.iterable",
            "es2015.promise",
            "es2015.proxy",
            "es2015.reflect",
            "es2015.symbol",
            "es2015.symbol.wellknown",
            "es2016.array.include",
            "es2016",
            "es2017.arraybuffer",
            "es2017.date",
            "es2017.object",
            "es2017.sharedmemory",
            "es2017.string",
            "es2017.typedarrays",
            "es2017",
            "es2018.asyncgenerator",
            "es2018.asynciterable",
            "es2018.promise",
            "es2018.regexp",
            "es2018",
            "es2019.array",
            "es2019.object",
            "es2019.string",
            "es2019.symbol",
            "es2019",
            "es2020.bigint",
            "es2020.date",
            "es2020.number",
            "es2020.promise",
            "es2020.sharedmemory",
            "es2020.string",
            "es2020.symbol.wellknown",
            "es2020",
            "es2021.promise",
            "es2021.string",
            "es2021.weakref",
            "es2021",
            "es2022.array",
            "es2022.error",
            "es2022.object",
            "es2022.regexp",
            "es2022.string",
            "es2022",
            "es2023.array",
            "es2023.collection",
            "es2023",
            "es2024.arraybuffer",
            "es2024.collection",
            "es2024.object",
            "es2024.promise",
            "es2024.regexp",
            "es2024.sharedmemory",
            "es2024.string",
            "es2024",
            "es2025.collection",
            "es2025",
            "esnext.array",
            "esnext.collection",
            "esnext.decorators",
            "esnext.disposable",
            "esnext.error",
            "esnext.float16",
            "esnext.intl",
            "esnext.iterator",
            "esnext.object",
            "esnext.promise",
            "esnext.regexp",
            "esnext.string",
            "esnext.symbol",
            "esnext.typedarrays",
        ],
        _ => return None,
    })
}

/// Map a lowercase directive key to the camelCase field name in CheckOptions.
fn directive_to_field_name(key: &str) -> Option<&'static str> {
    Some(match key {
        "strict" => "strict",
        "strictnullchecks" => "strictNullChecks",
        "strictfunctiontypes" => "strictFunctionTypes",
        "strictbindcallapply" => "strictBindCallApply",
        "strictpropertyinitialization" => "strictPropertyInitialization",
        "noimplicitany" => "noImplicitAny",
        "noimplicitthis" => "noImplicitThis",
        "noimplicitreturns" => "noImplicitReturns",
        "useunknownincatchvariables" => "useUnknownInCatchVariables",
        "alwaysstrict" => "alwaysStrict",
        "nounusedlocals" => "noUnusedLocals",
        "nounusedparameters" => "noUnusedParameters",
        "exactoptionalpropertytypes" => "exactOptionalPropertyTypes",
        "nouncheckedindexedaccess" => "noUncheckedIndexedAccess",
        "allowunreachablecode" => "allowUnreachableCode",
        "allowunusedlabels" => "allowUnusedLabels",
        "nopropertyaccessfromindexsignature" => "noPropertyAccessFromIndexSignature",
        "esmoduleinterop" => "esModuleInterop",
        "allowsyntheticdefaultimports" => "allowSyntheticDefaultImports",
        "isolatedmodules" => "isolatedModules",
        "nolib" => "noLib",
        "lib" => "lib",
        "target" => "target",
        "module" => "module",
        "experimentaldecorators" => "experimentalDecorators",
        "noresolve" => "noResolve",
        "allowjs" => "allowJs",
        "checkjs" => "checkJs",
        "resolvejsonmodule" => "resolveJsonModule",
        "nouncheckedsideeffectimports" => "noUncheckedSideEffectImports",
        "noimplicitoverride" => "noImplicitOverride",
        "strictbuiltiniteratorreturn" => "strictBuiltinIteratorReturn",
        "declaration" => "declaration",
        "nofallthroughcasesinswitch" | "nofallthrough" => "noFallthroughCasesInSwitch",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_strict_mode() {
        let mut directives = HashMap::new();
        directives.insert("strict".to_string(), "true".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["strict"], true);
    }

    #[test]
    fn lib_directive_becomes_array() {
        let mut directives = HashMap::new();
        directives.insert("lib".to_string(), "es6,dom".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["lib"], json!(["es6", "dom"]));
    }

    #[test]
    fn target_stays_string() {
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2020".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["target"], "es2020");
    }

    #[test]
    fn harness_directives_skipped() {
        let mut directives = HashMap::new();
        directives.insert("filename".to_string(), "test.ts".to_string());
        directives.insert("strict".to_string(), "true".to_string());
        let opts = directives_to_check_options(&directives);
        assert!(opts.get("filename").is_none());
        assert_eq!(opts["strict"], true);
    }

    #[test]
    fn unknown_directives_skipped() {
        let mut directives = HashMap::new();
        directives.insert("unknownoption".to_string(), "foo".to_string());
        let opts = directives_to_check_options(&directives);
        let map = opts.as_object().unwrap();
        assert!(!map.contains_key("unknownoption"));
        // With only unknown options, opts is empty so strict defaults aren't injected
        assert!(map.is_empty());
    }

    #[test]
    fn unsupported_server_options_detected() {
        let mut directives = HashMap::new();
        directives.insert("jsx".to_string(), "react".to_string());
        assert!(has_unsupported_server_options(&directives));

        let mut directives2 = HashMap::new();
        directives2.insert("strict".to_string(), "true".to_string());
        assert!(!has_unsupported_server_options(&directives2));
    }

    #[test]
    fn target_injects_default_libs() {
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2015".to_string());
        let opts = directives_to_check_options(&directives);
        assert!(
            opts.get("lib").is_some(),
            "lib should be injected when target is set"
        );
        let libs = opts["lib"].as_array().unwrap();
        assert!(libs.iter().any(|v| v == "es5"));
        assert!(
            libs.iter()
                .any(|v| v.as_str().is_some_and(|s| s.starts_with("es2015"))),
            "expected at least one es2015.* sub-lib, got: {libs:?}"
        );
    }

    #[test]
    fn explicit_lib_not_overridden_by_target() {
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2020".to_string());
        directives.insert("lib".to_string(), "es5".to_string());
        let opts = directives_to_check_options(&directives);
        // When lib is explicit, target should not inject defaults
        let libs = opts["lib"].as_array().unwrap();
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0], "es5");
    }

    #[test]
    fn declaration_directive_passed_to_server_options() {
        let mut directives = HashMap::new();
        directives.insert("declaration".to_string(), "true".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(
            opts["declaration"], true,
            "declaration: true should be passed to server CheckOptions"
        );
    }

    #[test]
    fn declaration_directive_not_treated_as_harness_only() {
        assert!(
            !is_harness_directive("declaration"),
            "declaration must not be filtered as a harness-only directive"
        );
    }

    // ───────── edge-case behaviour locks ─────────────────────────────────
    // The conformance harness routes every test directive through this
    // module, so untested branches here can break large test buckets at
    // once when new directives are added.

    #[test]
    fn nolib_directive_blocks_target_default_lib_injection() {
        // `noLib: true` must prevent the target-driven default-lib chain
        // from being injected, even when `target` is recognized.
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2015".to_string());
        directives.insert("nolib".to_string(), "true".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(
            opts.get("lib"),
            None,
            "noLib must block target-driven default-lib injection, got {opts:?}"
        );
        assert_eq!(opts["noLib"], true);
    }

    #[test]
    fn nolib_false_also_blocks_lib_injection_via_key_presence_check() {
        // The injection guard tests *presence* of the noLib key, not its
        // truthiness: setting `noLib: false` via directives still blocks
        // target-driven injection. Lock this behaviour explicitly so a
        // future tightening to `is_truthy(noLib)` is a deliberate change.
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2015".to_string());
        directives.insert("nolib".to_string(), "false".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["noLib"], false);
        assert_eq!(
            opts.get("lib"),
            None,
            "lib injection guard checks key presence — \
             noLib: false still blocks injection, got {opts:?}"
        );
    }

    #[test]
    fn comma_separated_target_uses_first_token_for_lib_chain() {
        // `default_libs_for_target` splits on `,` and uses the first
        // (trimmed, lowercased) token. The `target` field itself keeps
        // the original comma-separated string — only the lib chain is
        // derived from the first token.
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2015,es2020".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["target"], "es2015,es2020");
        let libs = opts["lib"].as_array().expect("lib should be injected");
        // First-token (es2015) drives selection: must contain es2015.* but
        // not es2020.* sub-libs.
        assert!(
            libs.iter().any(|v| v == "es5"),
            "es2015 chain must include es5"
        );
        assert!(
            libs.iter()
                .any(|v| v.as_str().is_some_and(|s| s.starts_with("es2015"))),
            "es2015 chain must include at least one es2015.* sub-lib, got: {libs:?}"
        );
        assert!(
            !libs
                .iter()
                .any(|v| v.as_str().is_some_and(|s| s.starts_with("es2020"))),
            "first-token-only selection must not pull in es2020.* sub-libs, got: {libs:?}"
        );
    }

    #[test]
    fn unrecognized_target_skips_lib_injection() {
        // `default_libs_for_target` returns `None` for unknown targets,
        // and the caller must not inject a `lib` field in that case.
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "foobar".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["target"], "foobar");
        assert_eq!(
            opts.get("lib"),
            None,
            "unrecognized target must not trigger lib injection, got {opts:?}"
        );
    }

    #[test]
    fn bool_false_directive_value_maps_to_json_false() {
        // The other tests cover `"true"` boolean conversion; this locks
        // the symmetric `"false"` branch (line 36-37 of the source).
        let mut directives = HashMap::new();
        directives.insert("strict".to_string(), "false".to_string());
        directives.insert("noimplicitany".to_string(), "false".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["strict"], false);
        assert_eq!(opts["noImplicitAny"], false);
    }

    #[test]
    fn mixed_case_directive_keys_are_normalized() {
        // Directive keys go through `to_lowercase` before lookup, so
        // mixed-case input from `// @StrictNullChecks: true`-style
        // directives must still produce the camelCase JSON field.
        let mut directives = HashMap::new();
        directives.insert("StrictNullChecks".to_string(), "true".to_string());
        directives.insert("NOIMPLICITANY".to_string(), "true".to_string());
        directives.insert("EsModuleInterop".to_string(), "true".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["strictNullChecks"], true);
        assert_eq!(opts["noImplicitAny"], true);
        assert_eq!(opts["esModuleInterop"], true);
    }

    #[test]
    fn nofallthrough_alias_maps_to_no_fallthrough_cases_in_switch() {
        // `nofallthrough` is an alias spelling that maps to the same
        // canonical `noFallthroughCasesInSwitch` field as the canonical
        // spelling. Lock both spellings.
        let mut directives = HashMap::new();
        directives.insert("nofallthrough".to_string(), "true".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["noFallthroughCasesInSwitch"], true);

        let mut directives2 = HashMap::new();
        directives2.insert("nofallthroughcasesinswitch".to_string(), "true".to_string());
        let opts2 = directives_to_check_options(&directives2);
        assert_eq!(opts2["noFallthroughCasesInSwitch"], true);
    }

    #[test]
    fn empty_directives_produce_empty_options_object() {
        // No directives, no target, no lib => empty options object (the
        // target-driven lib injection guard requires `target` to be set).
        let directives: HashMap<String, String> = HashMap::new();
        let opts = directives_to_check_options(&directives);
        let map = opts.as_object().expect("opts must be an object");
        assert!(
            map.is_empty(),
            "empty directives must produce empty options, got {map:?}"
        );
    }

    #[test]
    fn lib_directive_lowercases_and_trims_each_token() {
        // `lib` parsing splits on `,`, trims whitespace, and lowercases
        // each token. Directives written as `lib: ES5, DOM ` must match
        // the canonical lowercase lib names the server expects.
        let mut directives = HashMap::new();
        directives.insert("lib".to_string(), "ES5, DOM , ES2015.Core".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["lib"], json!(["es5", "dom", "es2015.core"]));
    }

    #[test]
    fn has_unsupported_server_options_is_case_insensitive() {
        // Like `directives_to_check_options`, the unsupported-server
        // detection lowercases each key before comparison. Mixed-case
        // `JSX`, `Paths`, etc. must still trigger CLI fallback.
        let mut directives = HashMap::new();
        directives.insert("JSX".to_string(), "react".to_string());
        assert!(has_unsupported_server_options(&directives));

        let mut directives2 = HashMap::new();
        directives2.insert("Paths".to_string(), "{}".to_string());
        assert!(has_unsupported_server_options(&directives2));

        let mut directives3 = HashMap::new();
        directives3.insert("MODULERESOLUTION".to_string(), "node".to_string());
        assert!(has_unsupported_server_options(&directives3));
    }

    #[test]
    fn esnext_and_latest_targets_share_default_lib_chain() {
        // `esnext` and `latest` both map to the same lib chain via the
        // `"esnext" | "latest"` arm. Lock that they produce identical
        // arrays so a future split is a deliberate change.
        let mut d1 = HashMap::new();
        d1.insert("target".to_string(), "esnext".to_string());
        let opts1 = directives_to_check_options(&d1);

        let mut d2 = HashMap::new();
        d2.insert("target".to_string(), "latest".to_string());
        let opts2 = directives_to_check_options(&d2);

        let libs1 = opts1["lib"].as_array().expect("esnext should inject libs");
        let libs2 = opts2["lib"].as_array().expect("latest should inject libs");
        assert_eq!(
            libs1, libs2,
            "esnext and latest must share the same lib chain"
        );
    }
}
