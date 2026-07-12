use rustc_hash::FxHashMap;
use std::ffi::OsString;

use super::clap_errors::KNOWN_TSC_OPTIONS;
use super::{TSC_VERSION, help};

/// A pre-clap exit directive produced by [`preprocess_args`].
///
/// Preprocessing never writes to stdout or terminates the process itself.
/// When a tsc-compatible quirk requires printing and exiting before clap runs
/// (help, version, `--all`, or a pre-parse rejection), it returns this value so
/// the binary entrypoint owns the I/O. `message` is a single line of text with
/// no trailing newline — the entrypoint adds it.
pub(super) struct EarlyExit {
    pub(super) message: String,
    pub(super) code: i32,
}

impl EarlyExit {
    /// A success early exit (code 0): help, version, or `--all`.
    const fn print(message: String) -> Self {
        Self { message, code: 0 }
    }

    /// A pre-parse rejection (code 1): TS5023 / TS6369.
    const fn reject(message: String) -> Self {
        Self { message, code: 1 }
    }
}

/// Outcome of tsc-compatibility argument preprocessing.
///
/// Keeping this side-effect free is what makes every rewrite and pre-parse
/// diagnostic unit-testable: the early-exit paths can be asserted without a
/// test terminating the process.
pub(super) enum PreprocessOutcome {
    /// Continue startup with these normalized arguments (handed to clap).
    Continue(Vec<OsString>),
    /// Print [`EarlyExit::message`] to stdout and exit before clap parsing.
    EarlyExit(EarlyExit),
}

impl PreprocessOutcome {
    /// Unwrap the normalized arguments for tests that exercise the rewrite
    /// pipeline (inputs that never trigger an early exit). Panics otherwise.
    #[cfg(test)]
    pub(super) fn into_continue(self) -> Vec<OsString> {
        match self {
            PreprocessOutcome::Continue(args) => args,
            PreprocessOutcome::EarlyExit(exit) => panic!(
                "expected PreprocessOutcome::Continue, got EarlyExit(code={}): {:?}",
                exit.code, exit.message
            ),
        }
    }
}

/// Preprocess command-line arguments for tsc compatibility.
///
/// Runs (BEFORE clap parsing), in order:
/// - `@file` response file expansion (tsc reads args from response files)
/// - Case-insensitive / kebab-case flag names: `--NoEmit` → `--noEmit` (tsc v6)
/// - Pre-parse early exits, returned as [`PreprocessOutcome::EarlyExit`]:
///   - `--all` (with or without `--help`) → print all options, exit 0
///   - `--help` / `-h` / `-?` → print help, exit 0
///   - `--version` / `-v` / `-V` → print version, exit 0 (`-v` is version only
///     outside build mode; in build mode it means `--build-verbose`)
///   - `--` / `-` / `--boolFlag=value` → TS5023 unknown option, exit 1
///   - `--build`/`-b` not first → TS6369 / TS5023, exit 1
/// - Build mode flag remapping: when `--build`/`-b` is the first argument,
///   `-v` maps to `--build-verbose`, `-d` maps to `--dry`, `-f` maps to `--force`
/// - Boolean flag values: `--strict false` → strip the flag (tsc v6 compat)
/// - Optional boolean flags: `--strictNullChecks file.ts` → `--strictNullChecks=true file.ts`
/// - Duplicate flags: `--strict --strict` → deduplicated (tsc v6 compat)
///
/// The function is side-effect free: all stdout/exit I/O is owned by the
/// caller via the returned [`PreprocessOutcome`].
pub(super) fn preprocess_args(args: Vec<OsString>) -> PreprocessOutcome {
    let mut expanded = expand_response_files(args);
    canonicalize_long_flags(&mut expanded);

    if let Some(exit) = preparse_exit_directive(&expanded) {
        return PreprocessOutcome::EarlyExit(exit);
    }
    if let Some(exit) = preparse_unknown_rejection(&expanded) {
        return PreprocessOutcome::EarlyExit(exit);
    }

    let build_remapped = remap_build_mode_flags(expanded);
    let mut normalized = normalize_bool_values_and_deduplicate(build_remapped);

    if let Some(exit) = build_position_rejection(&normalized) {
        return PreprocessOutcome::EarlyExit(exit);
    }

    append_direct_cli_option_order(&mut normalized);

    PreprocessOutcome::Continue(normalized)
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

/// Detect the help / `--all` / version pre-parse exits.
///
/// `--all` takes precedence over `--help`, which takes precedence over
/// version. Returns the matching [`EarlyExit`] (code 0), or `None`.
fn preparse_exit_directive(args: &[OsString]) -> Option<EarlyExit> {
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
        return Some(EarlyExit::print(help::colorize_help(
            &help::render_help_all(TSC_VERSION),
        )));
    }

    // --help / -h / -?
    if has_help {
        return Some(EarlyExit::print(help::colorize_help(&help::render_help(
            TSC_VERSION,
        ))));
    }

    // --version / -v / -V
    if has_version {
        return Some(EarlyExit::print(format!("Version {TSC_VERSION}")));
    }

    None
}

/// Detect the `--` / `-` / `--boolFlag=value` pre-parse rejections that tsc
/// reports as TS5023 before clap parsing. Returns the matching [`EarlyExit`]
/// (stdout text + code 1), or `None`.
fn preparse_unknown_rejection(args: &[OsString]) -> Option<EarlyExit> {
    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--" || s == "-" {
            return Some(unknown_option_exit(&s));
        }
        // tsc treats --boolFlag=value as an unknown option (the whole --flag=value string)
        if let Some(eq_pos) = s.find('=') {
            let flag_part = &s[..eq_pos];
            if is_boolean_flag(flag_part) {
                return Some(unknown_option_exit(&s));
            }
        }
    }
    None
}

fn unknown_option_exit(option: &str) -> EarlyExit {
    EarlyExit::reject(format!("error TS5023: Unknown compiler option '{option}'."))
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

/// Append a hidden, repeated side-channel that preserves the final argv order
/// of options whose diagnostics are produced by the shared config parser.
///
/// The side-channel carries option identities only. Diagnostic construction
/// remains structural in the driver, and no rendered message text is parsed.
fn append_direct_cli_option_order(args: &mut Vec<OsString>) {
    const MARKER_PREFIX: &str = "--__direct-cli-option-order=";

    // Do not allow an externally supplied hidden marker to influence order.
    args.retain(|arg| !arg.to_string_lossy().starts_with(MARKER_PREFIX));

    let mut ordered = Vec::new();
    for index in 1..args.len() {
        let Some(name) = direct_cli_diagnostic_option_at(args, index) else {
            continue;
        };

        // Duplicate compiler options have last-occurrence semantics. Move an
        // existing entry rather than emitting the same diagnostic twice.
        if let Some(previous) = ordered.iter().position(|candidate| *candidate == name) {
            ordered.remove(previous);
        }
        ordered.push(name);
    }

    args.extend(
        ordered
            .into_iter()
            .map(|name| OsString::from(format!("{MARKER_PREFIX}{name}"))),
    );
}

fn direct_cli_diagnostic_option_at(args: &[OsString], index: usize) -> Option<&'static str> {
    let arg = args[index].to_string_lossy();

    if let Some(name) = arg.strip_prefix("--__explicitly-disabled-bool-flag=") {
        return dropped_bool_option_name(name);
    }

    let (flag, inline_value) = arg
        .split_once('=')
        .map_or((arg.as_ref(), None), |(flag, value)| (flag, Some(value)));
    let value_equals = |expected: &str| {
        inline_value.map_or_else(
            || {
                args.get(index + 1)
                    .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case(expected))
            },
            |value| value.eq_ignore_ascii_case(expected),
        )
    };

    match flag {
        "--target" | "-t" if value_equals("es3") => Some("target"),
        "--module" | "-m" if value_equals("none") => Some("module"),
        "--paths" if !value_equals("null") => Some("paths"),
        "--plugins" if !value_equals("null") => Some("plugins"),
        "--charset" => Some("charset"),
        "--importsNotUsedAsValues" => Some("importsNotUsedAsValues"),
        "--out" => Some("out"),
        _ => flag.strip_prefix("--").and_then(dropped_bool_option_name),
    }
}

fn dropped_bool_option_name(name: &str) -> Option<&'static str> {
    match name {
        "keyofStringsOnly" => Some("keyofStringsOnly"),
        "noImplicitUseStrict" => Some("noImplicitUseStrict"),
        "noStrictGenericChecks" => Some("noStrictGenericChecks"),
        "preserveValueImports" => Some("preserveValueImports"),
        "suppressExcessPropertyErrors" => Some("suppressExcessPropertyErrors"),
        "suppressImplicitAnyIndexErrors" => Some("suppressImplicitAnyIndexErrors"),
        _ => None,
    }
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
        "__directclioptionorder" => Some("--__direct-cli-option-order"),
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
    "--__direct-cli-option-order",
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

/// Check that `--build`/`-b` is the first argument.
/// tsc v6 behavior:
///   - `--build` (long form) not first → TS6369 ("must be first")
///   - `-b` (short form) not first → TS5023 ("unknown compiler option")
///
/// Returns an [`EarlyExit`] (code 1) if either form appears but is not first.
fn build_position_rejection(args: &[OsString]) -> Option<EarlyExit> {
    // Skip program name (index 0)
    let mut first_non_program = true;

    for arg in args.iter().skip(1) {
        let s = arg.to_string_lossy();
        if s == "--build" {
            if !first_non_program {
                return Some(EarlyExit::reject(
                    "error TS6369: Option '--build' must be the first command line argument."
                        .to_string(),
                ));
            }
            return None;
        }
        if s == "-b" {
            if !first_non_program {
                return Some(EarlyExit::reject(
                    "error TS5023: Unknown compiler option '-b'.".to_string(),
                ));
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

    fn run(args: &[&str]) -> PreprocessOutcome {
        preprocess_args(args.iter().map(OsString::from).collect())
    }

    fn preprocess_strs(args: &[&str]) -> Vec<String> {
        run(args)
            .into_continue()
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

    /// Extract the [`EarlyExit`] for inputs expected to short-circuit before
    /// clap. Panics if preprocessing instead returned `Continue` — which is the
    /// whole point of this refactor: these paths used to call `process::exit`
    /// and so could not be asserted at all.
    fn early_exit(args: &[&str]) -> EarlyExit {
        match run(args) {
            PreprocessOutcome::EarlyExit(exit) => exit,
            PreprocessOutcome::Continue(args) => {
                panic!("expected EarlyExit, got Continue({args:?})")
            }
        }
    }

    fn is_continue(args: &[&str]) -> bool {
        matches!(run(args), PreprocessOutcome::Continue(_))
    }

    #[test]
    fn version_flags_exit_zero_with_version_banner() {
        let expected = format!("Version {TSC_VERSION}");
        for input in [
            &["tsz", "--version"][..],
            &["tsz", "-V"][..],
            &["tsz", "-v"][..],
            &["tsz", "file.ts", "--version"][..],
        ] {
            let exit = early_exit(input);
            assert_eq!(exit.code, 0, "{input:?}");
            assert_eq!(exit.message, expected, "{input:?}");
        }
    }

    #[test]
    fn help_and_all_flags_exit_zero_with_nonempty_banner() {
        for input in [
            &["tsz", "--help"][..],
            &["tsz", "-h"][..],
            &["tsz", "-?"][..],
            &["tsz", "--all"][..],
        ] {
            let exit = early_exit(input);
            assert_eq!(exit.code, 0, "{input:?}");
            assert!(!exit.message.trim().is_empty(), "{input:?}");
        }
    }

    #[test]
    fn all_takes_precedence_over_help() {
        // `--all` and `--help` together must render the all-options banner,
        // preserving the original precedence order.
        let combined = early_exit(&["tsz", "--all", "--help"]);
        let all_only = early_exit(&["tsz", "--all"]);
        assert_eq!(combined.message, all_only.message);
        assert_eq!(combined.code, 0);
    }

    #[test]
    fn dash_v_is_build_verbose_not_version_in_build_mode() {
        // In build mode `-v` means --build-verbose, so it must NOT early-exit
        // as a version request; it stays in the normalized args.
        assert!(is_continue(&["tsz", "--build", "-v"]));
        let normalized = preprocess_strs(&["tsz", "--build", "-v"]);
        assert!(normalized.iter().any(|a| a == "--build-verbose"));
        assert!(!normalized.iter().any(|a| a == "--version"));
    }

    #[test]
    fn unknown_bare_dashes_reject_with_ts5023() {
        for opt in ["--", "-"] {
            let exit = early_exit(&["tsz", opt]);
            assert_eq!(exit.code, 1, "{opt}");
            assert_eq!(
                exit.message,
                format!("error TS5023: Unknown compiler option '{opt}'."),
            );
        }
    }

    #[test]
    fn boolean_flag_with_equals_value_rejects_with_ts5023() {
        // tsc treats `--noEmit=true` (a boolean flag in `--flag=value` form) as
        // an unknown option, reported verbatim with the whole token.
        let exit = early_exit(&["tsz", "--noEmit=true", "file.ts"]);
        assert_eq!(exit.code, 1);
        assert_eq!(
            exit.message,
            "error TS5023: Unknown compiler option '--noEmit=true'."
        );
    }

    #[test]
    fn build_must_be_the_first_argument() {
        // `--build` not first → TS6369; `-b` not first → TS5023; first → continue.
        let long = early_exit(&["tsz", "file.ts", "--build"]);
        assert_eq!(long.code, 1);
        assert_eq!(
            long.message,
            "error TS6369: Option '--build' must be the first command line argument."
        );

        let short = early_exit(&["tsz", "file.ts", "-b"]);
        assert_eq!(short.code, 1);
        assert_eq!(short.message, "error TS5023: Unknown compiler option '-b'.");

        assert!(is_continue(&["tsz", "--build", "file.ts"]));
    }

    #[test]
    fn ordinary_invocation_returns_continue_byte_stable() {
        // A normal compile invocation triggers no early exit and needs no
        // rewrite: it must come back as Continue with the argv unchanged.
        let input = &["tsz", "--noEmit", "src/main.ts"];
        assert!(is_continue(input));
        assert_eq!(preprocess_strs(input), input.to_vec());
    }
}
