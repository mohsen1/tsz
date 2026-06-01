use rustc_hash::FxHashMap;
use std::ffi::OsString;

use super::clap_errors::KNOWN_TSC_OPTIONS;
use super::{TSC_VERSION, help};

/// Preprocess command-line arguments for tsc compatibility.
///
/// Handles (BEFORE clap parsing):
/// - `--version` / `-v` / `-V` → print version, exit 0
/// - `--help` / `-h` / `-?` → print help, exit 0
/// - `--all` (with or without `--help`) → print all options, exit 0
/// - `@file` response file expansion (tsc reads args from response files)
/// - Build mode flag remapping: when `--build`/`-b` is the first argument,
///   `-v` maps to `--build-verbose`, `-d` maps to `--dry`, `-f` maps to `--force`
/// - Case-insensitive flag names: `--NoEmit` → `--noEmit` (tsc v6 compat)
/// - Boolean flag values: `--strict false` → strip the flag (tsc v6 compat)
/// - Optional boolean flags: `--strictNullChecks file.ts` → `--strictNullChecks=true file.ts`
/// - Duplicate flags: `--strict --strict` → deduplicated (tsc v6 compat)
pub(super) fn preprocess_args(args: Vec<OsString>) -> Vec<OsString> {
    let mut expanded = expand_response_files(args);
    canonicalize_long_flags(&mut expanded);
    handle_preparse_exits(&expanded);
    reject_preparse_unknowns(&expanded);

    let build_remapped = remap_build_mode_flags(expanded);
    normalize_bool_values_and_deduplicate(build_remapped)
}

fn expand_response_files(args: Vec<OsString>) -> Vec<OsString> {
    let mut expanded = Vec::with_capacity(args.len());

    for (i, arg) in args.into_iter().enumerate() {
        if i == 0 {
            expanded.push(arg);
            continue;
        }

        let arg_str = arg.to_string_lossy();

        if arg_str.starts_with('@') && arg_str.len() > 1 {
            // Response file: @path reads arguments from file
            let path = &arg_str[1..];
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            for part in split_response_line(trimmed) {
                                expanded.push(OsString::from(part));
                            }
                        }
                    }
                }
                Err(_) => {
                    expanded.push(arg);
                }
            }
        } else {
            expanded.push(arg);
        }
    }

    expanded
}

fn canonicalize_long_flags(args: &mut [OsString]) {
    for arg in args.iter_mut().skip(1) {
        let s = arg.to_string_lossy();
        if s.starts_with("--") && s.len() > 2 {
            if let Some(eq_pos) = s.find('=') {
                let flag_part = &s[2..eq_pos];
                let value_part = &s[eq_pos..];
                if let Some(canonical) = canonicalize_long_flag(flag_part) {
                    *arg = OsString::from(format!("{canonical}{value_part}"));
                }
            } else {
                let flag_part = &s[2..];
                if let Some(canonical) = canonicalize_long_flag(flag_part) {
                    *arg = OsString::from(canonical);
                }
            }
        }
    }
}

fn handle_preparse_exits(args: &[OsString]) {
    let is_build_mode = is_build_mode(args);
    let mut has_help = false;
    let mut has_all = false;
    let mut has_version = false;

    for (i, arg) in args.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let s = arg.to_string_lossy();
        match s.as_ref() {
            "--help" | "-h" | "-?" => has_help = true,
            "--all" => has_all = true,
            "--version" | "-V" => has_version = true,
            // -v means version only outside build mode; in build mode it means --build-verbose
            "-v" if !is_build_mode => has_version = true,
            _ => {}
        }
    }

    // --all takes precedence (with or without --help)
    if has_all {
        println!(
            "{}",
            help::colorize_help(&help::render_help_all(TSC_VERSION))
        );
        std::process::exit(0);
    }

    // --help / -h / -?
    if has_help {
        println!("{}", help::colorize_help(&help::render_help(TSC_VERSION)));
        std::process::exit(0);
    }

    // --version / -v / -V
    if has_version {
        println!("Version {TSC_VERSION}");
        std::process::exit(0);
    }
}

fn reject_preparse_unknowns(args: &[OsString]) {
    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--" || s == "-" {
            println!("error TS5023: Unknown compiler option '{s}'.");
            std::process::exit(1);
        }
        // tsc treats --boolFlag=value as an unknown option (the whole --flag=value string)
        if let Some(eq_pos) = s.find('=') {
            let flag_part = &s[..eq_pos];
            if is_boolean_flag(flag_part) {
                println!("error TS5023: Unknown compiler option '{s}'.");
                std::process::exit(1);
            }
        }
    }
}

fn is_build_mode(args: &[OsString]) -> bool {
    args.get(1)
        .map(|a| {
            let s = a.to_string_lossy();
            s == "--build" || s == "-b"
        })
        .unwrap_or(false)
}

fn remap_build_mode_flags(args: Vec<OsString>) -> Vec<OsString> {
    let is_build_mode = is_build_mode(&args);
    let mut result = Vec::with_capacity(args.len());

    for (i, arg) in args.into_iter().enumerate() {
        if i == 0 {
            result.push(arg);
            continue;
        }

        let arg_str = arg.to_string_lossy();

        if is_build_mode && i > 1 {
            // In build mode, remap short flags:
            //   -v → --build-verbose (not --version)
            //   -d → --dry           (not --declaration)
            //   -f → --force
            match arg_str.as_ref() {
                "-v" => {
                    result.push(OsString::from("--build-verbose"));
                    continue;
                }
                "-d" => {
                    result.push(OsString::from("--dry"));
                    continue;
                }
                "-f" => {
                    result.push(OsString::from("--force"));
                    continue;
                }
                _ => {}
            }
        }

        result.push(arg);
    }

    result
}

fn normalize_bool_values_and_deduplicate(args: Vec<OsString>) -> Vec<OsString> {
    let mut final_result = Vec::with_capacity(args.len());
    let mut flag_positions: FxHashMap<String, usize> = FxHashMap::default();
    let mut skip_positions: Vec<bool> = Vec::new();

    let mut i = 0;
    if !args.is_empty() {
        final_result.push(args[0].clone());
        skip_positions.push(false);
        i = 1;
    }

    while i < args.len() {
        let arg_str = args[i].to_string_lossy().to_string();

        if arg_str.starts_with("--") {
            let flag_name = if let Some(eq_pos) = arg_str.find('=') {
                arg_str[..eq_pos].to_string()
            } else {
                arg_str.clone()
            };

            let is_boolean = is_boolean_flag(flag_name.as_str());
            let takes_value = is_valued_flag(flag_name.as_str());

            // Check if next arg is "true" or "false" for boolean flags
            if is_boolean
                && !arg_str.contains('=')
                && let Some(next) = args.get(i + 1)
            {
                let next_str = next.to_string_lossy();
                let next_lower = next_str.to_lowercase();
                if next_lower == "false" {
                    if is_option_bool_flag(flag_name.as_str()) {
                        push_option_bool_arg(
                            &mut final_result,
                            &mut skip_positions,
                            &mut flag_positions,
                            &flag_name,
                            false,
                        );
                        i += 2;
                        continue;
                    }
                    // Plain bool flag: clap can't represent an explicit `false`,
                    // so strip the `--flag false` pair and forward the intent
                    // through a hidden side-channel arg. The override pipeline
                    // reads `args.explicitly_disabled_bool_flags` and uses it to
                    // flip a `true` value loaded from `tsconfig.json` to `false`.
                    if let Some(&prev_idx) = flag_positions.get(&flag_name) {
                        skip_positions[prev_idx] = true;
                    }
                    flag_positions.remove(&flag_name);
                    let bare = flag_name.trim_start_matches("--");
                    final_result.push(OsString::from(format!(
                        "--__explicitly-disabled-bool-flag={bare}"
                    )));
                    skip_positions.push(false);
                    i += 2;
                    continue;
                } else if next_lower == "true" {
                    if is_option_bool_flag(flag_name.as_str()) {
                        push_option_bool_arg(
                            &mut final_result,
                            &mut skip_positions,
                            &mut flag_positions,
                            &flag_name,
                            true,
                        );
                        i += 2;
                        continue;
                    }
                    // Plain bool flag: keep the flag, skip the "true" token
                    i += 1;
                }
            }

            if is_boolean && !arg_str.contains('=') && is_option_bool_flag(flag_name.as_str()) {
                push_option_bool_arg(
                    &mut final_result,
                    &mut skip_positions,
                    &mut flag_positions,
                    &flag_name,
                    true,
                );
                i += 1;
                continue;
            }

            // Deduplicate: if we've seen this flag before, mark old position for skip
            if let Some(&prev_idx) = flag_positions.get(&flag_name) {
                skip_positions[prev_idx] = true;
                if takes_value
                    && !final_result[prev_idx].to_string_lossy().contains('=')
                    && prev_idx + 1 < skip_positions.len()
                {
                    skip_positions[prev_idx + 1] = true;
                }
            }

            let current_idx = final_result.len();
            flag_positions.insert(flag_name, current_idx);
            final_result.push(OsString::from(&arg_str));
            skip_positions.push(false);

            if takes_value && !arg_str.contains('=') {
                i += 1;
                if i < args.len() {
                    final_result.push(args[i].clone());
                    skip_positions.push(false);
                }
            }
        } else {
            final_result.push(args[i].clone());
            skip_positions.push(false);
        }

        i += 1;
    }

    final_result
        .into_iter()
        .zip(skip_positions)
        .filter_map(|(arg, skip)| if skip { None } else { Some(arg) })
        .collect()
}

fn push_option_bool_arg(
    final_result: &mut Vec<OsString>,
    skip_positions: &mut Vec<bool>,
    flag_positions: &mut FxHashMap<String, usize>,
    flag_name: &str,
    value: bool,
) {
    if let Some(&prev_idx) = flag_positions.get(flag_name) {
        skip_positions[prev_idx] = true;
    }

    let current_idx = final_result.len();
    flag_positions.insert(flag_name.to_string(), current_idx);
    final_result.push(OsString::from(format!("{flag_name}={value}")));
    skip_positions.push(false);
}

/// Return the canonical long flag spelling for tsc-compatible case-insensitive
/// input, accepting both camelCase and kebab-case spellings.
fn canonicalize_long_flag(flag: &str) -> Option<&'static str> {
    for &known in KNOWN_TSC_OPTIONS {
        if flag_key_matches(&known[2..], flag) {
            return Some(known);
        }
    }

    match normalized_flag_key(flag).as_str() {
        "buildverbose" | "verbose" => Some("--build-verbose"),
        "batch" => Some("--batch"),
        "diagnosticsjson" => Some("--diagnostics-json"),
        "perfcountersjson" => Some("--perf-counters-json"),
        "tracedependencies" => Some("--traceDependencies"),
        "__explicitlydisabledboolflag" => Some("--__explicitly-disabled-bool-flag"),
        _ => None,
    }
}

fn flag_key_matches(canonical: &str, input: &str) -> bool {
    canonical
        .bytes()
        .filter(|&b| b != b'-')
        .map(|b| b.to_ascii_lowercase())
        .eq(input
            .bytes()
            .filter(|&b| b != b'-')
            .map(|b| b.to_ascii_lowercase()))
}

fn normalized_flag_key(flag: &str) -> String {
    flag.bytes()
        .filter(|&b| b != b'-')
        .map(|b| b.to_ascii_lowercase() as char)
        .collect()
}

/// Known boolean flags (flags that accept no value or optional true/false).
const BOOLEAN_FLAGS: &[&str] = &[
    "--all",
    "--build",
    "--init",
    "--listFilesOnly",
    "--showConfig",
    "--ignoreConfig",
    "--libReplacement",
    "--watch",
    "--noLib",
    "--useDefineForClassFields",
    "--experimentalDecorators",
    "--emitDecoratorMetadata",
    "--resolveJsonModule",
    "--resolvePackageJsonExports",
    "--resolvePackageJsonImports",
    "--allowArbitraryExtensions",
    "--allowImportingTsExtensions",
    "--rewriteRelativeImportExtensions",
    "--noResolve",
    "--allowUmdGlobalAccess",
    "--noUncheckedSideEffectImports",
    "--allowJs",
    "--checkJs",
    "--declaration",
    "--declarationMap",
    "--emitDeclarationOnly",
    "--sourceMap",
    "--inlineSourceMap",
    "--inlineSources",
    "--noEmit",
    "--noEmitOnError",
    "--noEmitHelpers",
    "--importHelpers",
    "--downlevelIteration",
    "--removeComments",
    "--preserveConstEnums",
    "--stripInternal",
    "--emitBOM",
    "--esModuleInterop",
    "--allowSyntheticDefaultImports",
    "--isolatedModules",
    "--isolatedDeclarations",
    "--verbatimModuleSyntax",
    "--forceConsistentCasingInFileNames",
    "--preserveSymlinks",
    "--erasableSyntaxOnly",
    "--strict",
    "--noImplicitAny",
    "--strictNullChecks",
    "--strictFunctionTypes",
    "--strictBindCallApply",
    "--strictPropertyInitialization",
    "--strictBuiltinIteratorReturn",
    "--noImplicitThis",
    "--useUnknownInCatchVariables",
    "--alwaysStrict",
    "--noUnusedLocals",
    "--noUnusedParameters",
    "--exactOptionalPropertyTypes",
    "--noImplicitReturns",
    "--noFallthroughCasesInSwitch",
    "--sound",
    "--soundReportOnly",
    "--noUncheckedIndexedAccess",
    "--noImplicitOverride",
    "--noPropertyAccessFromIndexSignature",
    "--allowUnreachableCode",
    "--allowUnusedLabels",
    "--skipDefaultLibCheck",
    "--skipLibCheck",
    "--composite",
    "--incremental",
    "--disableReferencedProjectLoad",
    "--disableSolutionSearching",
    "--disableSourceOfProjectReferenceRedirect",
    "--diagnostics",
    "--extendedDiagnostics",
    "--explainFiles",
    "--listFiles",
    "--listEmittedFiles",
    "--traceResolution",
    "--traceDependencies",
    "--noCheck",
    "--pretty",
    "--noErrorTruncation",
    "--preserveWatchOutput",
    "--synchronousWatchDirectory",
    "--build-verbose",
    "--dry",
    "--force",
    "--clean",
    "--stopBuildOnErrors",
    "--assumeChangesOnlyAffectDirectDependencies",
    "--keyofStringsOnly",
    "--noImplicitUseStrict",
    "--noStrictGenericChecks",
    "--preserveValueImports",
    "--suppressExcessPropertyErrors",
    "--suppressImplicitAnyIndexErrors",
    "--disableSizeLimit",
    "--batch",
];

fn is_boolean_flag(flag: &str) -> bool {
    BOOLEAN_FLAGS.contains(&flag)
}

/// Flags that take a mandatory value argument (not boolean flags).
const VALUED_FLAGS: &[&str] = &[
    "--locale",
    "--project",
    "--target",
    "--module",
    "--lib",
    "--jsx",
    "--jsxFactory",
    "--jsxFragmentFactory",
    "--jsxImportSource",
    "--moduleDetection",
    "--moduleResolution",
    "--baseUrl",
    "--typeRoots",
    "--types",
    "--rootDirs",
    "--paths",
    "--plugins",
    "--moduleSuffixes",
    "--customConditions",
    "--maxNodeModuleJsDepth",
    "--declarationDir",
    "--outDir",
    "--rootDir",
    "--outFile",
    "--mapRoot",
    "--sourceRoot",
    "--newLine",
    "--tsBuildInfoFile",
    "--generateTrace",
    "--generateCpuProfile",
    "--ignoreDeprecations",
    "--watchFile",
    "--watchDirectory",
    "--fallbackPolling",
    "--excludeDirectories",
    "--excludeFiles",
    "--reactNamespace",
    "--charset",
    "--importsNotUsedAsValues",
    "--out",
    "--typesVersions",
];

fn is_valued_flag(flag: &str) -> bool {
    VALUED_FLAGS.contains(&flag)
}

/// Flags that are Option<bool> (tri-state: None, Some(true), Some(false)).
/// These need --flag=true or --flag=false rather than flag removal.
const OPTION_BOOL_FLAGS: &[&str] = &[
    "--useDefineForClassFields",
    "--resolvePackageJsonExports",
    "--resolvePackageJsonImports",
    "--allowSyntheticDefaultImports",
    "--forceConsistentCasingInFileNames",
    "--noImplicitAny",
    "--strictNullChecks",
    "--strictFunctionTypes",
    "--strictBindCallApply",
    "--strictPropertyInitialization",
    "--strictBuiltinIteratorReturn",
    "--noImplicitThis",
    "--useUnknownInCatchVariables",
    "--alwaysStrict",
    "--allowUnreachableCode",
    "--allowUnusedLabels",
    "--pretty",
];

fn is_option_bool_flag(flag: &str) -> bool {
    OPTION_BOOL_FLAGS.contains(&flag)
}

/// Split a response file line into arguments, respecting quoted strings.
///
/// Handles both double (`"`) and single (`'`) quotes. Quotes are stripped
/// from the resulting tokens. Unquoted regions are split on whitespace.
pub(super) fn split_response_line(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in line.chars() {
        match in_quote {
            Some(q) if ch == q => {
                // Closing quote — end quoted region but don't push yet,
                // there may be more content adjacent (e.g. foo"bar"baz)
                in_quote = None;
            }
            Some(_) => {
                // Inside quotes — take character literally
                current.push(ch);
            }
            None if ch == '"' || ch == '\'' => {
                // Opening quote
                in_quote = Some(ch);
            }
            None if ch.is_ascii_whitespace() => {
                // Unquoted whitespace — flush current token
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

/// Check that --build/-b is the first argument.
/// tsc v6 behavior:
///   - `--build` (long form) not first → TS6369 ("must be first")
///   - `-b` (short form) not first → TS5023 ("unknown compiler option")
///
/// Returns an error message if either form appears but is not first.
pub(super) fn check_build_position(args: &[OsString]) -> Option<String> {
    // Skip program name (index 0)
    let mut first_non_program = true;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--build" {
            if !first_non_program {
                return Some(
                    "error TS6369: Option '--build' must be the first command line argument.\n"
                        .to_string(),
                );
            }
            return None;
        }
        if s == "-b" {
            if !first_non_program {
                return Some("error TS5023: Unknown compiler option '-b'.\n".to_string());
            }
            return None;
        }
        first_non_program = false;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preprocess_strs(args: &[&str]) -> Vec<String> {
        preprocess_args(args.iter().map(OsString::from).collect())
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn preprocesses_tsc_compat_rewrites_from_case_table() {
        struct Case {
            name: &'static str,
            input: &'static [&'static str],
            expected: &'static [&'static str],
        }

        let cases = [
            Case {
                name: "case-insensitive camel and kebab flags",
                input: &["tsz", "--No-Emit", "--types-versions", "5.7", "file.ts"],
                expected: &["tsz", "--noEmit", "--typesVersions", "5.7", "file.ts"],
            },
            Case {
                name: "build-mode short flags",
                input: &["tsz", "--build", "-v", "-d", "-f"],
                expected: &["tsz", "--build", "--build-verbose", "--dry", "--force"],
            },
            Case {
                name: "plain boolean false side channel",
                input: &["tsz", "--strict", "false", "file.ts"],
                expected: &["tsz", "--__explicitly-disabled-bool-flag=strict", "file.ts"],
            },
            Case {
                name: "option boolean defaults to true before file",
                input: &["tsz", "--strictNullChecks", "file.ts"],
                expected: &["tsz", "--strictNullChecks=true", "file.ts"],
            },
            Case {
                name: "duplicate valued flag keeps last value",
                input: &["tsz", "--target", "ES2020", "--target", "ES2022", "file.ts"],
                expected: &["tsz", "--target", "ES2022", "file.ts"],
            },
        ];

        for case in cases {
            assert_eq!(preprocess_strs(case.input), case.expected, "{}", case.name);
        }
    }

    #[test]
    fn split_response_line_respects_single_and_double_quotes() {
        let cases = [
            (
                r#"--outDir "my output" --rootDir 'src root'"#,
                vec!["--outDir", "my output", "--rootDir", "src root"],
            ),
            (r#"foo"bar"baz"#, vec!["foobarbaz"]),
            (
                r#""file one.ts" "file two.ts""#,
                vec!["file one.ts", "file two.ts"],
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(split_response_line(input), expected);
        }
    }
}
