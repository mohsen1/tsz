use anyhow::Result;
use clap::CommandFactory;
use std::ffi::OsString;

use super::CliArgs;

/// Handle a clap parsing error by reformatting it as a tsc-style diagnostic.
pub(super) fn handle_clap_error(err: clap::Error, args: &[OsString]) -> Result<()> {
    use clap::error::ErrorKind;

    match err.kind() {
        ErrorKind::UnknownArgument => {
            // Extract ALL unknown flags from the args and report each one
            let unknown_flags = extract_all_unknown_flags(args);
            if unknown_flags.is_empty() {
                // Fallback: just print TS5023 with whatever info we have
                println!("error TS5023: Unknown compiler option.");
            } else {
                for flag in &unknown_flags {
                    // tsc 7.0.2 (native) reports every unknown option as a
                    // bare TS5023 — the TS5025 "Did you mean" suggestion was
                    // dropped (oracle: `tsc --targett esnext` and
                    // `tsc --tagret esnext` both emit TS5023 with no
                    // suggestion). Match it.
                    println!("error TS5023: Unknown compiler option '{flag}'.");
                }
            }
            std::process::exit(1);
        }
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            // Help and version are handled in preprocess_args before clap,
            // but keep this arm for safety
            err.exit();
        }
        ErrorKind::MissingRequiredArgument => {
            // TS6044: Compiler option 'X' expects an argument.
            // Extract the option name from clap's error message
            let msg = err.to_string();
            if let Some(option_name) = extract_option_from_missing_value(&msg) {
                println!("error TS6044: Compiler option '{option_name}' expects an argument.");
                // Also emit TS6046 with valid values if this is an enum option
                if let Some(valid_values) = get_valid_values_for_option(&option_name) {
                    println!(
                        "error TS6046: Argument for '--{option_name}' option must be: {valid_values}."
                    );
                }
            } else {
                let msg = msg
                    .lines()
                    .next()
                    .unwrap_or(&msg)
                    .trim_start_matches("error: ");
                println!("error TS5023: {msg}");
            }
            std::process::exit(1);
        }
        ErrorKind::InvalidValue => {
            let msg = err.to_string();
            // Detect the "missing value" case: clap says "a value is required for"
            let is_missing_value = msg.contains("a value is required for");
            if let Some(option_name) = extract_option_from_invalid_value(&msg) {
                // TS6044: emit when the option was given without any value
                if is_missing_value {
                    println!("error TS6044: Compiler option '{option_name}' expects an argument.");
                }
                // TS6046: list valid values for enum options
                if let Some(valid_values) = get_valid_values_for_option(&option_name) {
                    println!(
                        "error TS6046: Argument for '--{option_name}' option must be: {valid_values}."
                    );
                } else if !is_missing_value {
                    let msg = msg
                        .lines()
                        .next()
                        .unwrap_or(&msg)
                        .trim_start_matches("error: ");
                    println!("error TS5023: {msg}");
                }
            } else {
                let msg = msg
                    .lines()
                    .next()
                    .unwrap_or(&msg)
                    .trim_start_matches("error: ");
                println!("error TS5023: {msg}");
            }
            std::process::exit(1);
        }
        _ => {
            // For other clap errors, still use exit code 1
            // and tsc-style formatting where possible
            let msg = err.to_string();
            // Strip clap's formatting prefix
            let msg = msg
                .lines()
                .next()
                .unwrap_or(&msg)
                .trim_start_matches("error: ");
            println!("error TS5023: {msg}");
            std::process::exit(1);
        }
    }
}

/// Extract ALL unknown flags from the preprocessed args.
/// Scans all args against known options and returns every unrecognized flag.
fn extract_all_unknown_flags(args: &[OsString]) -> Vec<String> {
    let known = collect_known_flags();
    let mut unknown = Vec::new();

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "-" || s == "--" {
            // Bare `-` and `--` are treated as unknown options to match tsc
            unknown.push(s.into_owned());
        } else if s.starts_with('-') && !s.starts_with("--") && s.len() == 2 {
            // Short flag like -x
            if !known.iter().any(|k| k == s.as_ref()) {
                unknown.push(s.into_owned());
            }
        } else if s.starts_with("--") {
            // Long flag like --badFlag (may have =value)
            let flag_part = s.split('=').next().unwrap_or(&s);
            if !known.iter().any(|k| k == flag_part) {
                unknown.push(flag_part.to_string());
            }
        }
    }
    unknown
}

/// Collect all known CLI flags from clap's command definition.
fn collect_known_flags() -> Vec<String> {
    let cmd = CliArgs::command();
    let mut known: Vec<String> = Vec::new();
    for a in cmd.get_arguments() {
        if let Some(long) = a.get_long() {
            known.push(format!("--{long}"));
        }
        // get_visible_aliases returns individual aliases
        for alias in a.get_visible_aliases().unwrap_or_default() {
            known.push(format!("--{alias}"));
        }
        if let Some(short) = a.get_short() {
            known.push(format!("-{short}"));
        }
    }
    // Also add -V (clap's version flag after our -v remapping)
    known.push("-V".to_string());
    known.push("--version".to_string());
    known.push("--help".to_string());
    known.push("-h".to_string());
    known
}

/// Extract the option name from a clap "missing required argument" error.
/// Clap formats these as: "a value is required for '--target <TARGET>' but none was supplied"
fn extract_option_from_missing_value(msg: &str) -> Option<String> {
    // Look for pattern: '--optionName' or '--optionName <VALUE>'
    let start = msg.find("'--")?;
    let after = &msg[start + 3..];
    let end = after.find(['\'', ' ', '<'])?;
    Some(after[..end].to_string())
}

/// Extract the option name from a clap "invalid value" error.
/// Clap formats these as: "invalid value 'blah' for '--target <TARGET>'"
fn extract_option_from_invalid_value(msg: &str) -> Option<String> {
    let start = msg.find("'--")?;
    let after = &msg[start + 3..];
    let end = after.find(['\'', ' ', '<'])?;
    Some(after[..end].to_string())
}

/// Get the valid values string for enum-typed CLI options, matching tsc's TS6046 format.
fn get_valid_values_for_option(option_name: &str) -> Option<&'static str> {
    // Value ordering and inclusion matches tsc baselines exactly.
    match option_name {
        "target" => Some(
            "'es6', 'es2015', 'es2016', 'es2017', 'es2018', 'es2019', 'es2020', 'es2021', 'es2022', 'es2023', 'es2024', 'es2025', 'esnext'",
        ),
        "module" => Some(
            "'commonjs', 'es6', 'es2015', 'es2020', 'es2022', 'esnext', 'node16', 'node18', 'node20', 'nodenext', 'preserve'",
        ),
        "jsx" => Some("'preserve', 'react-native', 'react-jsx', 'react-jsxdev', 'react'"),
        "moduleResolution" | "module-resolution" | "moduleresolution" => {
            // tsc 6.x omits the deprecated 'node10'/'node'/'classic' modes from the
            // TS6046 hint. They remain *accepted* (with a TS5107 deprecation warning) —
            // see `ModuleResolution` in `commands/args.rs` — but TypeScript filters them
            // out of the "must be" list via `deprecatedKeys` in
            // `createCompilerDiagnosticForInvalidCustomType`, so the hint lists only the
            // non-deprecated set.
            Some("'node16', 'nodenext', 'bundler'")
        }
        "moduleDetection" | "module-detection" | "moduledetection" => {
            Some("'auto', 'legacy', 'force'")
        }
        _ => None,
    }
}

/// All known tsc compiler option long names (for edit-distance matching).
/// These are the canonical --camelCase forms that tsc recognizes.
pub(super) const KNOWN_TSC_OPTIONS: &[&str] = &[
    "--all",
    "--allowArbitraryExtensions",
    "--allowImportingTsExtensions",
    "--allowJs",
    "--allowSyntheticDefaultImports",
    "--allowUmdGlobalAccess",
    "--allowUnreachableCode",
    "--allowUnusedLabels",
    "--alwaysStrict",
    "--assumeChangesOnlyAffectDirectDependencies",
    "--baseUrl",
    "--build",
    "--charset",
    "--checkJs",
    "--clean",
    "--composite",
    "--customConditions",
    "--declaration",
    "--declarationDir",
    "--declarationMap",
    "--diagnostics",
    "--disableReferencedProjectLoad",
    "--disableSizeLimit",
    "--disableSolutionSearching",
    "--disableSourceOfProjectReferenceRedirect",
    "--downlevelIteration",
    "--dry",
    "--emitBOM",
    "--emitDeclarationOnly",
    "--emitDecoratorMetadata",
    "--erasableSyntaxOnly",
    "--esModuleInterop",
    "--exactOptionalPropertyTypes",
    "--excludeDirectories",
    "--excludeFiles",
    "--experimentalDecorators",
    "--explainFiles",
    "--extendedDiagnostics",
    "--fallbackPolling",
    "--force",
    "--forceConsistentCasingInFileNames",
    "--generateCpuProfile",
    "--generateTrace",
    "--help",
    "--ignoreConfig",
    "--ignoreDeprecations",
    "--importHelpers",
    "--importsNotUsedAsValues",
    "--incremental",
    "--init",
    "--inlineSourceMap",
    "--inlineSources",
    "--isolatedDeclarations",
    "--isolatedModules",
    "--jsx",
    "--jsxFactory",
    "--jsxFragmentFactory",
    "--jsxImportSource",
    "--keyofStringsOnly",
    "--lib",
    "--libReplacement",
    "--listEmittedFiles",
    "--listFiles",
    "--listFilesOnly",
    "--locale",
    "--mapRoot",
    "--maxNodeModuleJsDepth",
    "--module",
    "--moduleDetection",
    "--moduleResolution",
    "--moduleSuffixes",
    "--newLine",
    "--noCheck",
    "--noEmit",
    "--noEmitHelpers",
    "--noEmitOnError",
    "--noErrorTruncation",
    "--noFallthroughCasesInSwitch",
    "--noImplicitAny",
    "--noImplicitOverride",
    "--noImplicitReturns",
    "--noImplicitThis",
    "--noImplicitUseStrict",
    "--noLib",
    "--noPropertyAccessFromIndexSignature",
    "--noResolve",
    "--noStrictGenericChecks",
    "--noUncheckedIndexedAccess",
    "--noUncheckedSideEffectImports",
    "--noUnusedLocals",
    "--noUnusedParameters",
    "--out",
    "--outDir",
    "--outFile",
    "--paths",
    "--plugins",
    "--preserveConstEnums",
    "--preserveSymlinks",
    "--preserveValueImports",
    "--preserveWatchOutput",
    "--pretty",
    "--project",
    "--reactNamespace",
    "--removeComments",
    "--resolveJsonModule",
    "--resolvePackageJsonExports",
    "--resolvePackageJsonImports",
    "--rewriteRelativeImportExtensions",
    "--rootDir",
    "--rootDirs",
    "--showConfig",
    "--skipDefaultLibCheck",
    "--skipLibCheck",
    "--sound",
    "--soundReportOnly",
    "--sourceMap",
    "--sourceRoot",
    "--stopBuildOnErrors",
    "--strict",
    "--strictBindCallApply",
    "--strictBuiltinIteratorReturn",
    "--strictFunctionTypes",
    "--strictNullChecks",
    "--strictPropertyInitialization",
    "--strict",
    "--stripInternal",
    "--suppressExcessPropertyErrors",
    "--suppressImplicitAnyIndexErrors",
    "--synchronousWatchDirectory",
    "--target",
    "--traceResolution",
    "--tsBuildInfoFile",
    "--typeRoots",
    "--types",
    "--typesVersions",
    "--useDefineForClassFields",
    "--useUnknownInCatchVariables",
    "--verbatimModuleSyntax",
    "--version",
    "--watch",
    "--watchDirectory",
    "--watchFile",
];
