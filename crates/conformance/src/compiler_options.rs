//! Shared compiler option helpers for conformance tooling.

use std::collections::HashMap;

const HARNESS_ONLY_DIRECTIVES: &[&str] = &[
    "allowNonTsExtensions",
    "baselineFile",
    "currentDirectory",
    "filename",
    "fullEmitPaths",
    "noImplicitReferences",
    "noErrorTruncation",
    "noTypesAndSymbols",
    "sourcemap",
    "sourceMap",
    "suppressOutputPathCheck",
    "symlink",
    "link",
    "traceResolution",
    "useCaseSensitiveFileNames",
    "reportDiagnostics",
    "captureSuggestions",
    "typeScriptVersion",
    "skip",
];

const LIST_OPTIONS: &[&str] = &[
    "lib",
    "types",
    "typeRoots",
    "rootDirs",
    "moduleSuffixes",
    "customConditions",
];

/// Map lowercase compiler option names to canonical camelCase.
///
/// Options not in this map stay lowercase, which causes TS5025 "Did you mean?"
/// diagnostics. The conformance runner and cache generators must use the same
/// mapping so cached TSC diagnostics and tsz diagnostics are produced from the
/// same tsconfig shape.
pub fn canonical_option_name(key_lower: &str) -> &str {
    match key_lower {
        "allowarbitraryextensions" => "allowArbitraryExtensions",
        "allowimportingtsextensions" => "allowImportingTsExtensions",
        "allowjs" => "allowJs",
        "allowsyntheticdefaultimports" => "allowSyntheticDefaultImports",
        "allowumdglobalaccess" => "allowUmdGlobalAccess",
        "allowunreachablecode" => "allowUnreachableCode",
        "allowunusedlabels" => "allowUnusedLabels",
        "alwaysstrict" => "alwaysStrict",
        "baseurl" => "baseUrl",
        "charset" => "charset",
        "checkjs" => "checkJs",
        "composite" => "composite",
        "customconditions" => "customConditions",
        "declaration" => "declaration",
        "declarationdir" => "declarationDir",
        "declarationmap" => "declarationMap",
        "diagnostics" => "diagnostics",
        "disablereferencedprojectload" => "disableReferencedProjectLoad",
        "disablesizelimt" => "disableSizeLimit",
        "disablesolutioncaching" => "disableSolutionCaching",
        "disablesolutiontypecheck" => "disableSolutionTypeCheck",
        "disablesolutiontypechecking" => "disableSolutionTypeChecking",
        "disablesourceofreferencedprojectload" => "disableSourceOfReferencedProjectLoad",
        "downleveliteration" => "downlevelIteration",
        "emitbom" => "emitBOM",
        "emitdeclarationonly" => "emitDeclarationOnly",
        "emitdecoratormetadata" => "emitDecoratorMetadata",
        "erasablesyntaxonly" => "erasableSyntaxOnly",
        "esmoduleinterop" => "esModuleInterop",
        "exactoptionalpropertytypes" => "exactOptionalPropertyTypes",
        "experimentaldecorators" => "experimentalDecorators",
        "extendeddiagnostics" => "extendedDiagnostics",
        "forceconsecinferfaces" | "forceconsistentcasinginfilenames" => {
            "forceConsistentCasingInFileNames"
        }
        "generatecputrace" | "generatecpuprofile" => "generateCpuProfile",
        "generatetrace" => "generateTrace",
        "ignoredeprecations" => "ignoreDeprecations",
        "importhelpers" => "importHelpers",
        "importsnotusedasvalues" => "importsNotUsedAsValues",
        "incremental" => "incremental",
        "inlineconstants" => "inlineConstants",
        "inlinesourcemap" => "inlineSourceMap",
        "inlinesources" => "inlineSources",
        "isolateddeclarations" => "isolatedDeclarations",
        "isolatedmodules" => "isolatedModules",
        "jsx" => "jsx",
        "jsxfactory" => "jsxFactory",
        "jsxfragmentfactory" => "jsxFragmentFactory",
        "jsximportsource" => "jsxImportSource",
        "keyofstringsonly" => "keyofStringsOnly",
        "lib" => "lib",
        "libreplacement" => "libReplacement",
        "listemittedfiles" => "listEmittedFiles",
        "listfiles" => "listFiles",
        "listfilesonly" => "listFilesOnly",
        "locale" => "locale",
        "maproot" => "mapRoot",
        "maxnodemodulejsdepth" => "maxNodeModuleJsDepth",
        "module" => "module",
        "moduledetection" => "moduleDetection",
        "moduleresolution" => "moduleResolution",
        "modulesuffixes" => "moduleSuffixes",
        "newline" => "newLine",
        "nocheck" => "noCheck",
        "noemit" => "noEmit",
        "noemithelpers" => "noEmitHelpers",
        "noemitonerror" => "noEmitOnError",
        "noerrortruncation" => "noErrorTruncation",
        "nofallthrough" | "nofallthroughcasesinswitch" => "noFallthroughCasesInSwitch",
        "noimplicitany" => "noImplicitAny",
        "noimplicitoverride" => "noImplicitOverride",
        "noimplicitreturns" => "noImplicitReturns",
        "noimplicitthis" => "noImplicitThis",
        "noimplicitusestrict" => "noImplicitUseStrict",
        "nolib" => "noLib",
        "nopropertyaccessfromindexsignature" => "noPropertyAccessFromIndexSignature",
        "noresolve" => "noResolve",
        "nostrictgenericchecks" => "noStrictGenericChecks",
        "notypesandsymbols" => "noTypesAndSymbols",
        "nouncheckedindexedaccess" => "noUncheckedIndexedAccess",
        "nouncheckedsideeffectimports" => "noUncheckedSideEffectImports",
        "nounusedlocals" => "noUnusedLocals",
        "nounusedparameters" => "noUnusedParameters",
        "out" => "out",
        "outdir" => "outDir",
        "outfile" => "outFile",
        "paths" => "paths",
        "plugins" => "plugins",
        "preserveconstenums" => "preserveConstEnums",
        "preservesymlinks" => "preserveSymlinks",
        "preservevalueimports" => "preserveValueImports",
        "preservewatchoutput" => "preserveWatchOutput",
        "pretty" => "pretty",
        "reactnamespace" => "reactNamespace",
        "removecomments" => "removeComments",
        "resolvejsonmodule" => "resolveJsonModule",
        "resolvepackagejsonexports" => "resolvePackageJsonExports",
        "resolvepackagejsonimports" => "resolvePackageJsonImports",
        "rewriterelativeimportextensions" => "rewriteRelativeImportExtensions",
        "rootdir" => "rootDir",
        "rootdirs" => "rootDirs",
        "skipdefaultlibcheck" => "skipDefaultLibCheck",
        "skiplibcheck" => "skipLibCheck",
        "sourcemap" => "sourceMap",
        "sourceroot" => "sourceRoot",
        "strict" => "strict",
        "strictbindcallapply" => "strictBindCallApply",
        "strictbuiltiniteratorreturn" => "strictBuiltinIteratorReturn",
        "strictfunctiontypes" => "strictFunctionTypes",
        "strictnullchecks" => "strictNullChecks",
        "strictpropertyinitialization" => "strictPropertyInitialization",
        "stripinternal" => "stripInternal",
        "suppressexcesspropertyerrors" => "suppressExcessPropertyErrors",
        "suppressimplicitanyindexerrors" => "suppressImplicitAnyIndexErrors",
        "target" => "target",
        "traceresolution" => "traceResolution",
        "tsbuildinfofile" => "tsBuildInfoFile",
        "typeroots" => "typeRoots",
        "types" => "types",
        "usedefineforclassfields" => "useDefineForClassFields",
        "useunknownincatchvariables" => "useUnknownInCatchVariables",
        "verbatimmodulesyntax" => "verbatimModuleSyntax",
        _ => key_lower,
    }
}

/// Convert TypeScript test-harness directive options into a `tsconfig`
/// `compilerOptions` object.
///
/// The conformance runner and both TSC cache generators must share this exact
/// conversion so cached TSC baselines and live tsz diagnostics see the same
/// option shape.
pub fn directives_to_tsconfig(options: &HashMap<String, String>) -> serde_json::Value {
    let mut opts = serde_json::Map::new();
    let mut strict_explicit = false;

    for (key, value) in options {
        let key_lower = key.to_lowercase();
        if HARNESS_ONLY_DIRECTIVES
            .iter()
            .any(|directive| directive.eq_ignore_ascii_case(&key_lower))
        {
            continue;
        }

        if key_lower == "strict" {
            strict_explicit = true;
        }

        let tsconfig_key = canonical_option_name(&key_lower);
        let json_value = if value == "true" {
            serde_json::Value::Bool(true)
        } else if value == "false" {
            serde_json::Value::Bool(false)
        } else if LIST_OPTIONS
            .iter()
            .any(|option| option.eq_ignore_ascii_case(&key_lower))
        {
            let is_type_roots = key_lower == "typeroots";
            let items = value
                .split(',')
                .map(|item| {
                    let item = item.trim();
                    let item = if is_type_roots {
                        item.trim_start_matches('/')
                    } else {
                        item
                    };
                    serde_json::Value::String(item.to_string())
                })
                .collect();
            serde_json::Value::Array(items)
        } else {
            let effective_value = value.split(',').next().unwrap_or(value).trim();
            if let Ok(num) = effective_value.parse::<i64>() {
                serde_json::Value::Number(num.into())
            } else {
                serde_json::Value::String(effective_value.to_string())
            }
        };

        opts.insert(tsconfig_key.to_string(), json_value);
    }

    if strict_explicit {
        if let Some(serde_json::Value::Bool(strict)) = opts.get("strict") {
            let strict = *strict;
            for key in [
                "alwaysStrict",
                "noImplicitAny",
                "noImplicitThis",
                "strictNullChecks",
                "strictFunctionTypes",
                "strictBindCallApply",
                "strictPropertyInitialization",
                "useUnknownInCatchVariables",
            ] {
                opts.entry(key.to_string())
                    .or_insert(serde_json::Value::Bool(strict));
            }
        }
    }

    opts.sort_keys();

    serde_json::Value::Object(opts)
}

#[cfg(test)]
mod tests {
    use super::{canonical_option_name, directives_to_tsconfig};
    use std::collections::HashMap;

    #[test]
    fn canonicalizes_camel_case_options() {
        assert_eq!(
            canonical_option_name("strictnullchecks"),
            "strictNullChecks"
        );
        assert_eq!(
            canonical_option_name("allowunusedlabels"),
            "allowUnusedLabels"
        );
        assert_eq!(canonical_option_name("typeroots"), "typeRoots");
    }

    #[test]
    fn canonicalizes_aliases() {
        assert_eq!(
            canonical_option_name("nofallthrough"),
            "noFallthroughCasesInSwitch"
        );
        assert_eq!(
            canonical_option_name("forceconsecinferfaces"),
            "forceConsistentCasingInFileNames"
        );
        assert_eq!(
            canonical_option_name("generatecputrace"),
            "generateCpuProfile"
        );
    }

    #[test]
    fn leaves_unknown_options_lowercase() {
        assert_eq!(canonical_option_name("notarealoption"), "notarealoption");
    }

    #[test]
    fn shared_converter_canonicalizes_and_filters_options() {
        let options = HashMap::from([
            ("moduleresolution".to_string(), "node16".to_string()),
            ("captureSuggestions".to_string(), "true".to_string()),
            ("lib".to_string(), "es6, dom".to_string()),
            ("typeroots".to_string(), "/types, /more-types".to_string()),
        ]);

        let value = directives_to_tsconfig(&options);
        let opts = value.as_object().expect("compilerOptions object");

        assert_eq!(
            opts.get("moduleResolution"),
            Some(&serde_json::Value::String("node16".to_string()))
        );
        assert!(!opts.contains_key("captureSuggestions"));
        assert_eq!(opts.get("lib"), Some(&serde_json::json!(["es6", "dom"])));
        assert_eq!(
            opts.get("typeRoots"),
            Some(&serde_json::json!(["types", "more-types"]))
        );
    }

    #[test]
    fn shared_converter_expands_explicit_strict_false() {
        let options = HashMap::from([("strict".to_string(), "false".to_string())]);
        let value = directives_to_tsconfig(&options);
        let opts = value.as_object().expect("compilerOptions object");

        assert_eq!(opts.get("strict"), Some(&serde_json::Value::Bool(false)));
        assert_eq!(
            opts.get("strictPropertyInitialization"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            opts.get("alwaysStrict"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn shared_converter_preserves_no_check_compiler_option() {
        let options = HashMap::from([("nocheck".to_string(), "true".to_string())]);
        let value = directives_to_tsconfig(&options);
        let opts = value.as_object().expect("compilerOptions object");

        assert_eq!(opts.get("noCheck"), Some(&serde_json::Value::Bool(true)));
    }
}
