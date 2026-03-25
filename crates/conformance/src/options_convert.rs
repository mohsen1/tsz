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

    // Match tsc's default checker behavior for strict sub-flags.
    // In tsc, `strictPropertyInitialization` and `strictNullChecks` default to true
    // even without `strict: true` (since TS 4.0+). The server derives these from
    // the `strict` flag (which defaults false), so we inject the tsc defaults.
    // Only inject when neither `strict` nor the specific sub-flag is set.
    if !opts.is_empty() && !opts.contains_key("strict") {
        // These flags default to true in tsc's conformance test harness
        for key in ["strictPropertyInitialization", "strictNullChecks"] {
            if !opts.contains_key(key) {
                opts.insert(key.to_string(), Value::Bool(true));
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
            | "declaration"
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
        "es6" | "es2015" => vec!["es5", "es2015"],
        "es2016" => vec!["es5", "es2015", "es2016"],
        "es2017" => vec!["es5", "es2015", "es2016", "es2017"],
        "es2018" => vec!["es5", "es2015", "es2016", "es2017", "es2018"],
        "es2019" => vec!["es5", "es2015", "es2016", "es2017", "es2018", "es2019"],
        "es2020" => vec![
            "es5", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020",
        ],
        "es2021" => vec![
            "es5", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021",
        ],
        "es2022" | "es2023" => vec![
            "es5", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021", "es2022",
        ],
        "esnext" | "latest" => vec![
            "es5", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021", "es2022",
            "es2023", "esnext",
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
        "checkjs" => "checkJs",
        "resolvejsonmodule" => "resolveJsonModule",
        "nouncheckedsideeffectimports" => "noUncheckedSideEffectImports",
        "noimplicitoverride" => "noImplicitOverride",
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
        assert!(opts.as_object().unwrap().is_empty());
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
        assert!(libs.iter().any(|v| v == "es2015"));
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
}
