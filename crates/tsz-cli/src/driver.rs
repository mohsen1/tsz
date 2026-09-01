use anyhow::{Context, Result};
use serde::Serialize;
use std::ffi::OsString;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
pub use tsz::config::CompilerOptionPatch;
use tsz::config::{
    CompilerOptionKey, ProjectRequest, ProjectSelection, TargetValueOutcome, classify_target_value,
    find_config_file, resolve_project,
};
use tsz::host::SystemHost;
use tsz::{CompileOutput, Compiler, DeferredCompilerOption, SemanticCompletion};
const VERSION: &str = env!("CARGO_PKG_VERSION");
const BATCH_SENTINEL: &str = "---TSZ-BATCH-DONE---";
const SEMANTIC_COMPLETION_MARKER_PREFIX: &str = "---TSZ-SEMANTIC-COMPLETION:";
#[derive(Debug, Default, Clone)]
pub struct Invocation {
    pub options: CompilerOptionPatch,
    pub project: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub ignore_config: bool,
    pub pretty: bool,
    pub batch: bool,
    pub extended_diagnostics: bool,
    pub diagnostics_json: Option<PathBuf>,
    pub perf_counters_json: Option<PathBuf>,
    pub command_line_diagnostics: Vec<(u32, String)>,
    help: bool,
    version: bool,
}
pub fn main_entry(arguments: impl IntoIterator<Item = OsString>) -> Result<i32> {
    let arguments: Vec<OsString> = arguments.into_iter().collect();
    let invocation = parse_arguments(&arguments)?;
    if invocation.batch
        && invocation.command_line_diagnostics.is_empty()
        && !invocation.help
        && !invocation.version
    {
        run_batch(invocation)
    } else {
        Ok(run_once_with_writer(invocation, &mut std::io::stdout().lock())?.0)
    }
}
pub fn main_exit_code() -> i32 {
    main_entry(std::env::args_os().skip(1)).unwrap_or_else(|error| {
        println!("{error:#}");
        1
    })
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
        if let Some(key) = CompilerOptionKey::from_cli_name(&name) {
            let mut value = if key.takes_value() {
                take_value()?
            } else {
                optional_bool(arguments, &mut index, inline_value, true).to_string()
            };
            if key == CompilerOptionKey::Target {
                value = value.trim().to_string();
            }
            if key == CompilerOptionKey::Target
                && let TargetValueOutcome::Invalid { message, code } = classify_target_value(&value)
            {
                invocation
                    .command_line_diagnostics
                    .push((code, message.to_string()));
                index += 1;
                continue;
            }
            invocation.options.set_cli_value(key, &value);
            index += 1;
            continue;
        }
        if let Some(key) = DeferredCompilerOption::from_cli_name(&name) {
            let value = if key.takes_value() {
                take_value()?
            } else {
                optional_bool(arguments, &mut index, inline_value, true).to_string()
            };
            if let Some((code, message)) = deferred_option_diagnostic(key, &value) {
                invocation.command_line_diagnostics.push((code, message));
                index += 1;
                continue;
            }
            invocation.options.set_deferred_cli_value(key, &value);
            index += 1;
            continue;
        }
        match name.as_str() {
            "p" | "project" => invocation.project = Some(PathBuf::from(take_value()?)),
            "h" | "help" if inline_value.is_none() => invocation.help = true,
            "v" | "version" if inline_value.is_none() => invocation.version = true,
            "batch" => invocation.batch = true,
            "ignoreconfig" => {
                invocation.ignore_config = optional_bool(arguments, &mut index, inline_value, true);
            }
            "pretty" => {
                invocation.pretty = optional_bool(arguments, &mut index, inline_value, true)
            }
            "extendeddiagnostics" => invocation.extended_diagnostics = true,
            "diagnostics-json" => invocation.diagnostics_json = Some(PathBuf::from(take_value()?)),
            "perf-counters-json" => {
                invocation.perf_counters_json = Some(PathBuf::from(take_value()?));
            }
            _ => invocation
                .command_line_diagnostics
                .push((5023, format!("Unknown compiler option '{argument}'."))),
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
fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
fn deferred_option_diagnostic(
    option: DeferredCompilerOption,
    value: &str,
) -> Option<(u32, String)> {
    use DeferredCompilerOption::{AlwaysStrict, BaseUrl, DownlevelIteration, EsModuleInterop};
    let (code, authored) = match option {
        BaseUrl => (5102, "baseUrl".to_string()),
        DownlevelIteration if value.eq_ignore_ascii_case("false") => {
            (5102, "downlevelIteration".to_string())
        }
        AlwaysStrict if value.eq_ignore_ascii_case("false") => {
            (5108, "alwaysStrict=false".to_string())
        }
        EsModuleInterop if value.eq_ignore_ascii_case("false") => {
            (5108, "esModuleInterop=false".to_string())
        }
        DeferredCompilerOption::ModuleResolution if value.eq_ignore_ascii_case("node10") => {
            (5108, "moduleResolution=node10".to_string())
        }
        _ => return None,
    };
    Some((
        code,
        format!("Option '{authored}' has been removed. Please remove it from your configuration."),
    ))
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
        invocation.options.no_emit = Some(true);
        invocation.pretty = false;
        let (_, completion) = run_once_with_writer(invocation, &mut stdout)?;
        writeln!(
            stdout,
            "{SEMANTIC_COMPLETION_MARKER_PREFIX}{}---",
            completion.as_str()
        )?;
        writeln!(stdout, "{BATCH_SENTINEL}")?;
        stdout.flush()?;
    }
    Ok(0)
}
fn run_once_with_writer(
    invocation: Invocation,
    writer: &mut impl Write,
) -> Result<(i32, SemanticCompletion)> {
    if !invocation.command_line_diagnostics.is_empty() {
        for (code, message) in &invocation.command_line_diagnostics {
            writeln!(writer, "error TS{code}: {message}")?;
        }
        return Ok((1, SemanticCompletion::Complete));
    }
    if invocation.version {
        writeln!(writer, "Version {VERSION}")?;
        return Ok((0, SemanticCompletion::Complete));
    }
    if invocation.help {
        render_help(writer)?;
        return Ok((0, SemanticCompletion::Complete));
    }
    let output = match prepare_invocation(&invocation)? {
        PreparedInvocation::Compile(output) => *output,
        PreparedInvocation::Diagnostic { code, message } => {
            writeln!(writer, "error TS{code}: {message}")?;
            return Ok((1, SemanticCompletion::Complete));
        }
        PreparedInvocation::Help => {
            render_help(writer)?;
            return Ok((1, SemanticCompletion::Complete));
        }
    };
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
                "schema_version": 2,
                "counters": {},
                "stats": output.stats,
            }),
        )?;
    }
    Ok((output.exit_status.code(), output.semantic_completion))
}
enum PreparedInvocation {
    Compile(Box<CompileOutput>),
    Diagnostic { code: u32, message: &'static str },
    Help,
}
fn prepare_invocation(invocation: &Invocation) -> Result<PreparedInvocation> {
    if invocation.project.is_some() && !invocation.files.is_empty() {
        return Ok(PreparedInvocation::Diagnostic {
            code: 5042,
            message: "Option 'project' cannot be mixed with source files on a command line.",
        });
    }
    let current_directory = std::env::current_dir()?;
    let host = SystemHost::new(current_directory.clone());
    if !invocation.ignore_config
        && !invocation.files.is_empty()
        && find_config_file(&host, &current_directory).is_some()
    {
        return Ok(PreparedInvocation::Diagnostic {
            code: 5112,
            message: concat!(
                "tsconfig.json is present but will not be loaded if files are specified on ",
                "commandline. Use '--ignoreConfig' to skip this error."
            ),
        });
    }
    let selection = if !invocation.files.is_empty() || invocation.ignore_config {
        ProjectSelection::Files(invocation.files.clone())
    } else if let Some(project) = &invocation.project {
        ProjectSelection::Project(project.clone())
    } else {
        ProjectSelection::Search(current_directory.clone())
    };
    let request = ProjectRequest {
        selection,
        overrides: invocation.options.clone(),
    };
    let mut resolved = resolve_project(&host, &request);
    if invocation.project.is_none()
        && invocation.files.is_empty()
        && resolved.entry_config.is_none()
        && resolved.diagnostics.is_empty()
    {
        return Ok(PreparedInvocation::Help);
    }
    let mut options = resolved.apply_option_patch(&invocation.options);
    if let Some(path) = &mut options.root_dir
        && path.is_relative()
    {
        *path = current_directory.join(&*path);
    }
    Ok(PreparedInvocation::Compile(Box::new(
        Compiler::new().compile_resolved(resolved, &options),
    )))
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
    macro_rules! write_stats {
        ($format:literal; $(($label:literal, $value:expr)),+ $(,)?) => {
            $(writeln!(writer, concat!("{:<31}", $format), $label, $value)?;)+
        };
    }
    write_stats!("{}";
        ("Root files:", stats.root_files), ("Source files:", stats.source_files),
        ("Project configs:", stats.project_configs),
        ("Project references:", stats.project_references), ("Files:", stats.files),
        ("Lines:", stats.lines), ("Identifiers:", stats.identifiers),
        ("Symbols:", stats.symbols), ("Types:", stats.types),
    );
    write_stats!("{:.2}s";
        ("Parse time:", stats.parse_time_ms / 1_000.0),
        ("Bind time:", stats.bind_time_ms / 1_000.0),
        ("Check time:", stats.check_time_ms / 1_000.0),
        ("Emit time:", stats.emit_time_ms / 1_000.0),
        ("Total time:", stats.total_time_ms / 1_000.0),
    );
    Ok(())
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn render_help(writer: &mut impl Write) -> Result<()> {
    writeln!(
        writer,
        "tsz {VERSION}\n\nUsage: tsz [options] [file ...]\n\nOptions:\n  -p, --project <path>       Compile a project\n      --noEmit               Type-check without writing output\n      --noCheck              Emit without semantic checking\n      --declaration          Emit declaration files\n      --strict               Enable strict checking\n      --pretty <bool>        Enable pretty output\n      --extendedDiagnostics  Print phase timings\n      --batch                Read project paths from stdin"
    )?;
    Ok(())
}
#[cfg(test)]
#[path = "../rewrite-tests/driver_options_unit.rs"]
mod tests;
