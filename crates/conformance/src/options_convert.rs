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

    Value::Object(opts)
}

/// Directives that are test-harness metadata, not compiler options.
fn is_harness_directive(key: &str) -> bool {
    matches!(
        key,
        "filename"
            | "symlink"
            | "skip"
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
        "nocheck",
    ];
    directives
        .keys()
        .any(|k| UNSUPPORTED.contains(&k.to_lowercase().as_str()))
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
        "nocheck" => "noCheck",
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
    fn target_preserves_default_lib_resolution() {
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2015".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["target"], "es2015");
        assert_eq!(
            opts.get("lib"),
            None,
            "target-only check options must leave lib absent so the server resolves the full default lib set"
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
    fn nolib_directive_preserves_absent_lib() {
        // `noLib: true` leaves `lib` absent and lets the server suppress
        // default libs through the explicit noLib option.
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2015".to_string());
        directives.insert("nolib".to_string(), "true".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(
            opts.get("lib"),
            None,
            "noLib must not synthesize lib entries, got {opts:?}"
        );
        assert_eq!(opts["noLib"], true);
    }

    #[test]
    fn nolib_false_preserves_absent_lib() {
        // Boolean conversion still preserves explicit `noLib: false`, but
        // target-only default libs remain the resolver's responsibility.
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2015".to_string());
        directives.insert("nolib".to_string(), "false".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["noLib"], false);
        assert_eq!(
            opts.get("lib"),
            None,
            "noLib false must not synthesize lib entries, got {opts:?}"
        );
    }

    #[test]
    fn comma_separated_target_does_not_synthesize_lib() {
        // The target field itself keeps the original comma-separated string;
        // default library resolution happens downstream in the compiler.
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "es2015,es2020".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["target"], "es2015,es2020");
        assert_eq!(
            opts.get("lib"),
            None,
            "target-only check options must not synthesize lib entries, got {opts:?}"
        );
    }

    #[test]
    fn unrecognized_target_preserves_absent_lib() {
        // Unknown targets still pass through as target values, but default
        // library selection remains downstream compiler behavior.
        let mut directives = HashMap::new();
        directives.insert("target".to_string(), "foobar".to_string());
        let opts = directives_to_check_options(&directives);
        assert_eq!(opts["target"], "foobar");
        assert_eq!(
            opts.get("lib"),
            None,
            "unrecognized target must not synthesize lib entries, got {opts:?}"
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
        // No directives, no target, no lib => empty options object.
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
    fn esnext_and_latest_targets_preserve_absent_lib() {
        let mut d1 = HashMap::new();
        d1.insert("target".to_string(), "esnext".to_string());
        let opts1 = directives_to_check_options(&d1);

        let mut d2 = HashMap::new();
        d2.insert("target".to_string(), "latest".to_string());
        let opts2 = directives_to_check_options(&d2);

        assert_eq!(
            opts1.get("lib"),
            None,
            "esnext target-only options must not synthesize lib entries"
        );
        assert_eq!(
            opts2.get("lib"),
            None,
            "latest target-only options must not synthesize lib entries"
        );
    }
}
