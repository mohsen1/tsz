//! Shared compiler option name helpers for conformance tooling.

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

#[cfg(test)]
mod tests {
    use super::canonical_option_name;

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
}
