//! Convert test directive options to tsz-server CheckOptions JSON.
//!
//! The server's legacy protocol accepts a `CheckOptions` object with camelCase
//! field names. Test directives are `HashMap<String, String>` with lowercase
//! keys and string values. This module converts between the two.

use serde_json::{json, Map, Value};
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
}
