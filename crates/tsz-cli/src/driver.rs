use std::ffi::OsString;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use tsz::{CompileOutput, Compiler, CompilerOptions, SourceInput};
use walkdir::WalkDir;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BATCH_SENTINEL: &str = "---TSZ-BATCH-DONE---";

#[derive(Debug, Default, Clone)]
pub struct Invocation {
    pub options: CompilerOptions,
    pub project: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub ignore_config: bool,
    pub allow_js: bool,
    pub pretty: bool,
    pub batch: bool,
    pub extended_diagnostics: bool,
    pub diagnostics_json: Option<PathBuf>,
    pub perf_counters_json: Option<PathBuf>,
    pub unknown_options: Vec<String>,
}

pub fn main_entry(arguments: impl IntoIterator<Item = OsString>) -> Result<i32> {
    let arguments: Vec<OsString> = arguments.into_iter().collect();
    if arguments
        .iter()
        .any(|argument| argument == "--version" || argument == "-v")
    {
        println!("Version {VERSION}");
        return Ok(0);
    }
    if arguments.is_empty()
        || arguments
            .iter()
            .any(|argument| argument == "--help" || argument == "-h")
    {
        print_help();
        return Ok(0);
    }
    let invocation = parse_arguments(&arguments)?;
    if invocation.batch {
        run_batch(invocation)
    } else {
        run_once(invocation)
    }
}

pub fn parse_arguments(arguments: &[OsString]) -> Result<Invocation> {
    let mut invocation = Invocation {
        pretty: true,
        ..Invocation::default()
    };
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].to_string_lossy();
        if !argument.starts_with('-') {
            invocation.files.push(PathBuf::from(argument.as_ref()));
            index += 1;
            continue;
        }
        let (raw_name, inline_value) = argument
            .split_once('=')
            .map_or((argument.as_ref(), None), |(name, value)| {
                (name, Some(value))
            });
        let name = raw_name.trim_start_matches('-').to_ascii_lowercase();
        let mut take_value = || -> Result<String> {
            if let Some(value) = inline_value {
                return Ok(value.to_string());
            }
            index += 1;
            arguments
                .get(index)
                .map(|value| value.to_string_lossy().into_owned())
                .with_context(|| format!("Compiler option '{raw_name}' expects an argument."))
        };
        match name.as_str() {
            "p" | "project" => invocation.project = Some(PathBuf::from(take_value()?)),
            "batch" => invocation.batch = true,
            "ignoreconfig" => invocation.ignore_config = true,
            "noemit" => invocation.options.no_emit = true,
            "noemitonerror" => invocation.options.no_emit_on_error = true,
            "nocheck" => invocation.options.no_check = true,
            "strict" => {
                invocation.options.strict = optional_bool(arguments, &mut index, inline_value, true)
            }
            "noimplicitany" => {
                invocation.options.no_implicit_any =
                    optional_bool(arguments, &mut index, inline_value, true);
            }
            "nolib" => {
                invocation.options.no_lib =
                    optional_bool(arguments, &mut index, inline_value, true);
            }
            "declaration" => {
                invocation.options.declaration =
                    optional_bool(arguments, &mut index, inline_value, true)
            }
            "declarationmap" => {
                invocation.options.declaration_map =
                    optional_bool(arguments, &mut index, inline_value, true)
            }
            "sourcemap" => {
                invocation.options.source_map =
                    optional_bool(arguments, &mut index, inline_value, true)
            }
            "inlinesourcemap" => {
                invocation.options.inline_source_map =
                    optional_bool(arguments, &mut index, inline_value, true)
            }
            "removecomments" => {
                invocation.options.remove_comments =
                    optional_bool(arguments, &mut index, inline_value, true)
            }
            "allowjs" => {
                invocation.allow_js = optional_bool(arguments, &mut index, inline_value, true)
            }
            "pretty" => {
                invocation.pretty = optional_bool(arguments, &mut index, inline_value, true)
            }
            "extendeddiagnostics" => invocation.extended_diagnostics = true,
            "target" => invocation.options.target = take_value()?,
            "module" => invocation.options.module = take_value()?,
            "lib" => {
                invocation.options.lib = Some(
                    take_value()?
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                        .collect(),
                );
            }
            "outdir" => invocation.options.out_dir = Some(PathBuf::from(take_value()?)),
            "declarationdir" => {
                invocation.options.declaration_dir = Some(PathBuf::from(take_value()?));
            }
            "diagnostics-json" => invocation.diagnostics_json = Some(PathBuf::from(take_value()?)),
            "perf-counters-json" => {
                invocation.perf_counters_json = Some(PathBuf::from(take_value()?));
            }
            // Accepted process-surface options. The R0 engine applies the ones
            // represented in `CompilerOptions`; unsupported transforms remain
            // visible as emit mismatches in the retained oracle harness.
            "alwaysstrict"
            | "downleveliteration"
            | "noemithelpers"
            | "importhelpers"
            | "esmoduleinterop"
            | "experimentaldecorators"
            | "emitdecoratormetadata"
            | "exactoptionalpropertytypes"
            | "preserveconstenums"
            | "verbatimmodulesyntax"
            | "rewriterelativeimportextensions"
            | "isolatedmodules"
            | "preservevalueimports"
            | "stripinternal" => {
                consume_optional_bool(arguments, &mut index, inline_value);
            }
            "jsx"
            | "jsxfactory"
            | "jsxfragmentfactory"
            | "jsximportsource"
            | "moduleresolution"
            | "moduledetection"
            | "importsnotusedasvalues"
            | "baseurl"
            | "outfile"
            | "rootdir"
            | "usedefineforclassfields"
            | "strictnullchecks" => {
                let _ = take_value()?;
            }
            _ => invocation.unknown_options.push(raw_name.to_string()),
        }
        index += 1;
    }
    Ok(invocation)
}

fn optional_bool(
    arguments: &[OsString],
    index: &mut usize,
    inline: Option<&str>,
    default: bool,
) -> bool {
    if let Some(value) = inline {
        return parse_bool(value).unwrap_or(default);
    }
    if let Some(value) = arguments.get(*index + 1).and_then(|value| value.to_str())
        && let Some(value) = parse_bool(value)
    {
        *index += 1;
        return value;
    }
    default
}

fn consume_optional_bool(arguments: &[OsString], index: &mut usize, inline: Option<&str>) {
    if inline.is_none()
        && arguments
            .get(*index + 1)
            .and_then(|value| value.to_str())
            .and_then(parse_bool)
            .is_some()
    {
        *index += 1;
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn run_batch(base: Invocation) -> Result<i32> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let project = line?;
        if project.trim().is_empty() {
            continue;
        }
        let mut invocation = base.clone();
        invocation.batch = false;
        invocation.project = Some(PathBuf::from(project.trim()));
        invocation.files.clear();
        invocation.options.no_emit = true;
        invocation.pretty = false;
        let _ = run_once_with_writer(invocation, &mut stdout)?;
        writeln!(stdout, "{BATCH_SENTINEL}")?;
        stdout.flush()?;
    }
    Ok(0)
}

fn run_once(invocation: Invocation) -> Result<i32> {
    run_once_with_writer(invocation, &mut std::io::stdout().lock())
}

fn run_once_with_writer(invocation: Invocation, writer: &mut impl Write) -> Result<i32> {
    if let Some(option) = invocation.unknown_options.first() {
        writeln!(writer, "error TS5023: Unknown compiler option '{option}'.")?;
        return Ok(1);
    }
    let (inputs, options) = load_invocation(&invocation)?;
    let output = Compiler::new().compile(inputs, &options);
    render_diagnostics(&output, writer)?;
    write_emitted_files(&output)?;
    if invocation.extended_diagnostics {
        render_extended_diagnostics(&output, writer)?;
    }
    if let Some(path) = &invocation.diagnostics_json {
        write_json(path, &output.diagnostics)?;
    }
    if let Some(path) = &invocation.perf_counters_json {
        write_json(
            path,
            &serde_json::json!({
                "schema_version": 1,
                "counters": {},
                "stats": output.stats,
            }),
        )?;
    }
    let has_errors = output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.category == tsz::diagnostics::DiagnosticCategory::Error);
    if !has_errors {
        Ok(0)
    } else if output.emitted_files.is_empty() {
        Ok(1)
    } else {
        Ok(2)
    }
}

fn load_invocation(invocation: &Invocation) -> Result<(Vec<SourceInput>, CompilerOptions)> {
    let mut options = invocation.options.clone();
    let mut files = invocation.files.clone();
    let project = if invocation.ignore_config {
        None
    } else if let Some(project) = &invocation.project {
        Some(resolve_config_path(project)?)
    } else if files.is_empty() {
        find_config(&std::env::current_dir()?)
    } else {
        None
    };
    if let Some(config_path) = project {
        let config = read_config(&config_path)?;
        apply_config_options(&mut options, config.get("compilerOptions"));
        if files.is_empty() {
            files = config_files(&config_path, &config, invocation.allow_js)?;
        }
    }
    if files.is_empty() {
        bail!("error TS18003: No inputs were found in config file.");
    }
    files.sort();
    files.dedup();
    let inputs = files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("Could not read '{}'.", path.display()))?;
            Ok(SourceInput::new(path, Arc::<str>::from(text)))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((inputs, options))
}

fn resolve_config_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_dir() {
        path.join("tsconfig.json")
    } else {
        path.to_path_buf()
    };
    if !path.is_file() {
        bail!(
            "error TS5057: Cannot find a tsconfig.json file at '{}'.",
            path.display()
        );
    }
    Ok(path)
}

fn find_config(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|directory| directory.join("tsconfig.json"))
        .find(|candidate| candidate.is_file())
}

fn read_config(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path)?;
    let text = strip_json_comments(&text);
    let text = strip_trailing_commas(&text);
    serde_json::from_str(&text).with_context(|| format!("Failed to parse '{}'.", path.display()))
}

fn apply_config_options(options: &mut CompilerOptions, value: Option<&Value>) {
    let Some(value) = value.and_then(Value::as_object) else {
        return;
    };
    options.strict = bool_option(value, "strict").unwrap_or(options.strict);
    options.no_implicit_any =
        bool_option(value, "noImplicitAny").unwrap_or(options.no_implicit_any);
    options.no_lib = bool_option(value, "noLib").unwrap_or(options.no_lib);
    options.no_check = bool_option(value, "noCheck").unwrap_or(options.no_check);
    options.no_emit = bool_option(value, "noEmit").unwrap_or(options.no_emit);
    options.no_emit_on_error =
        bool_option(value, "noEmitOnError").unwrap_or(options.no_emit_on_error);
    options.declaration = bool_option(value, "declaration").unwrap_or(options.declaration);
    options.source_map = bool_option(value, "sourceMap").unwrap_or(options.source_map);
    options.remove_comments =
        bool_option(value, "removeComments").unwrap_or(options.remove_comments);
    if let Some(target) = value.get("target").and_then(Value::as_str) {
        options.target = target.to_string();
    }
    if let Some(module) = value.get("module").and_then(Value::as_str) {
        options.module = module.to_string();
    }
    if let Some(libraries) = value.get("lib").and_then(Value::as_array) {
        options.lib = Some(
            libraries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        );
    }
    if let Some(out_dir) = value.get("outDir").and_then(Value::as_str) {
        options.out_dir = Some(PathBuf::from(out_dir));
    }
    if let Some(declaration_dir) = value.get("declarationDir").and_then(Value::as_str) {
        options.declaration_dir = Some(PathBuf::from(declaration_dir));
    }
}

fn bool_option(options: &serde_json::Map<String, Value>, name: &str) -> Option<bool> {
    options.get(name).and_then(Value::as_bool)
}

fn config_files(path: &Path, config: &Value, allow_js: bool) -> Result<Vec<PathBuf>> {
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    if let Some(files) = config.get("files").and_then(Value::as_array) {
        return Ok(files
            .iter()
            .filter_map(Value::as_str)
            .map(|file| root.join(file))
            .collect());
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        name != "node_modules" && name != ".git" && name != "target" && name != ".target"
    }) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str());
        // Match the TypeScript harness's implicit include defaults exactly:
        // module-specific extensions are roots only when explicitly listed.
        let supported = matches!(extension, Some("ts" | "tsx"))
            || (allow_js && matches!(extension, Some("js" | "jsx")));
        if supported {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

fn render_diagnostics(output: &CompileOutput, writer: &mut impl Write) -> Result<()> {
    for diagnostic in &output.diagnostics {
        let source = diagnostic
            .source_file_id()
            .and_then(|file| output.program.source(file));
        writeln!(writer, "{}", diagnostic.render(source))?;
    }
    Ok(())
}

fn write_emitted_files(output: &CompileOutput) -> Result<()> {
    for emitted in &output.emitted_files {
        if let Some(parent) = emitted.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&emitted.path, &emitted.text)?;
    }
    Ok(())
}

fn render_extended_diagnostics(output: &CompileOutput, writer: &mut impl Write) -> Result<()> {
    let stats = &output.stats;
    writeln!(writer, "Files:                         {}", stats.files)?;
    writeln!(writer, "Lines:                         {}", stats.lines)?;
    writeln!(
        writer,
        "Identifiers:                   {}",
        stats.identifiers
    )?;
    writeln!(writer, "Symbols:                       {}", stats.symbols)?;
    writeln!(writer, "Types:                         {}", stats.types)?;
    writeln!(
        writer,
        "Parse time:                    {:.2}s",
        stats.parse_time_ms / 1_000.0
    )?;
    writeln!(
        writer,
        "Bind time:                     {:.2}s",
        stats.bind_time_ms / 1_000.0
    )?;
    writeln!(
        writer,
        "Check time:                    {:.2}s",
        stats.check_time_ms / 1_000.0
    )?;
    writeln!(
        writer,
        "Emit time:                     {:.2}s",
        stats.emit_time_ms / 1_000.0
    )?;
    writeln!(
        writer,
        "Total time:                    {:.2}s",
        stats.total_time_ms / 1_000.0
    )?;
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn strip_json_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut quote = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            output.push(byte as char);
            if byte == b'\\' && index + 1 < bytes.len() {
                index += 1;
                output.push(bytes[index] as char);
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            quote = Some(byte);
            output.push(byte as char);
            index += 1;
        } else if bytes.get(index..index + 2) == Some(b"//") {
            while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index += 2;
            while index + 1 < bytes.len() && bytes.get(index..index + 2) != Some(b"*/") {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
        } else {
            output.push(byte as char);
            index += 1;
        }
    }
    output
}

fn strip_trailing_commas(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b',' {
            let mut lookahead = index + 1;
            while bytes
                .get(lookahead)
                .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                lookahead += 1;
            }
            if matches!(bytes.get(lookahead), Some(b'}' | b']')) {
                index += 1;
                continue;
            }
        }
        output.push(bytes[index] as char);
        index += 1;
    }
    output
}

fn print_help() {
    println!(
        "tsz {VERSION}\n\nUsage: tsz [options] [file ...]\n\nOptions:\n  -p, --project <path>       Compile a project\n      --noEmit               Type-check without writing output\n      --noCheck              Emit without semantic checking\n      --declaration          Emit declaration files\n      --strict               Enable strict checking\n      --pretty <bool>        Enable pretty output\n      --extendedDiagnostics  Print phase timings\n      --batch                Read project paths from stdin"
    );
}
