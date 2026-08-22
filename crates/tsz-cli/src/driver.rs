use std::ffi::OsString;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use tsz::config::{
    CompilerOptionKey, ProjectRequest, ProjectSelection, find_config_file, resolve_project,
};
use tsz::host::SystemHost;
use tsz::{CompileOutput, Compiler, CompilerOptions, SemanticCompletion};

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
    pub unknown_options: Vec<String>,
}

/// Only command-line options the caller explicitly supplied.
///
/// Configuration is resolved by `tsz-core` first. Keeping absence distinct
/// from a false/default value lets the command line override configuration
/// without its defaults accidentally doing so.
#[derive(Debug, Default, Clone)]
pub struct CompilerOptionPatch {
    pub strict: Option<bool>,
    pub strict_null_checks: Option<bool>,
    pub strict_property_initialization: Option<bool>,
    pub no_implicit_any: Option<bool>,
    pub no_lib: Option<bool>,
    pub lib: Option<Vec<String>>,
    pub no_check: Option<bool>,
    pub no_emit: Option<bool>,
    pub no_emit_on_error: Option<bool>,
    pub declaration: Option<bool>,
    pub declaration_map: Option<bool>,
    pub source_map: Option<bool>,
    pub inline_source_map: Option<bool>,
    pub remove_comments: Option<bool>,
    pub allow_js: Option<bool>,
    pub target: Option<String>,
    pub module: Option<String>,
    pub root_dir: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub declaration_dir: Option<PathBuf>,
}

impl CompilerOptionPatch {
    fn apply_to(&self, options: &mut CompilerOptions) {
        macro_rules! apply_copy {
            ($($field:ident),* $(,)?) => {
                $(if let Some(value) = self.$field {
                    options.$field = value;
                })*
            };
        }
        apply_copy!(
            strict,
            no_lib,
            no_check,
            no_emit,
            no_emit_on_error,
            declaration,
            declaration_map,
            source_map,
            inline_source_map,
            remove_comments,
            allow_js,
        );
        if let Some(value) = self.strict_null_checks {
            options.strict_null_checks = Some(value);
        }
        if let Some(value) = self.strict_property_initialization {
            options.strict_property_initialization = Some(value);
        }
        if let Some(value) = self.no_implicit_any {
            options.no_implicit_any = Some(value);
        }
        if let Some(value) = &self.lib {
            options.lib = Some(value.clone());
        }
        if let Some(value) = &self.target {
            options.target.clone_from(value);
        }
        if let Some(value) = &self.module {
            options.module.clone_from(value);
        }
        if let Some(value) = &self.root_dir {
            options.root_dir = Some(value.clone());
        }
        if let Some(value) = &self.out_dir {
            options.out_dir = Some(value.clone());
        }
        if let Some(value) = &self.declaration_dir {
            options.declaration_dir = Some(value.clone());
        }
    }

    fn clear_overridden_origins(&self, project: &mut tsz::config::ResolvedProject) {
        macro_rules! clear_present {
            ($(($field:ident, $key:ident)),* $(,)?) => {
                $(if self.$field.is_some() {
                    project.mark_command_line_option(CompilerOptionKey::$key);
                })*
            };
        }
        clear_present!(
            (strict, Strict),
            (strict_null_checks, StrictNullChecks),
            (strict_property_initialization, StrictPropertyInitialization),
            (no_implicit_any, NoImplicitAny),
            (no_lib, NoLib),
            (lib, Lib),
            (no_check, NoCheck),
            (no_emit, NoEmit),
            (no_emit_on_error, NoEmitOnError),
            (declaration, Declaration),
            (declaration_map, DeclarationMap),
            (source_map, SourceMap),
            (inline_source_map, InlineSourceMap),
            (remove_comments, RemoveComments),
            (allow_js, AllowJs),
            (target, Target),
            (module, Module),
            (root_dir, RootDir),
            (out_dir, OutDir),
            (declaration_dir, DeclarationDir),
        );
    }
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
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        render_help(&mut std::io::stdout().lock())?;
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
            "ignoreconfig" => {
                invocation.ignore_config = optional_bool(arguments, &mut index, inline_value, true);
            }
            "noemit" => {
                invocation.options.no_emit =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "noemitonerror" => {
                invocation.options.no_emit_on_error =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "nocheck" => {
                invocation.options.no_check =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "strict" => {
                invocation.options.strict =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "strictnullchecks" => {
                invocation.options.strict_null_checks =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "strictpropertyinitialization" => {
                invocation.options.strict_property_initialization =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "noimplicitany" => {
                invocation.options.no_implicit_any =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "nolib" => {
                invocation.options.no_lib =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "declaration" => {
                invocation.options.declaration =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "declarationmap" => {
                invocation.options.declaration_map =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "sourcemap" => {
                invocation.options.source_map =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "inlinesourcemap" => {
                invocation.options.inline_source_map =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "removecomments" => {
                invocation.options.remove_comments =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "allowjs" => {
                invocation.options.allow_js =
                    Some(optional_bool(arguments, &mut index, inline_value, true));
            }
            "pretty" => {
                invocation.pretty = optional_bool(arguments, &mut index, inline_value, true)
            }
            "extendeddiagnostics" => invocation.extended_diagnostics = true,
            "target" => invocation.options.target = Some(take_value()?),
            "module" => invocation.options.module = Some(take_value()?),
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
            "rootdir" => invocation.options.root_dir = Some(PathBuf::from(take_value()?)),
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
            | "usedefineforclassfields" => {
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
        invocation.options.no_emit = Some(true);
        invocation.pretty = false;
        let outcome = run_once_with_writer(invocation, &mut stdout)?;
        writeln!(
            stdout,
            "{SEMANTIC_COMPLETION_MARKER_PREFIX}{}---",
            outcome.semantic_completion.as_str()
        )?;
        writeln!(stdout, "{BATCH_SENTINEL}")?;
        stdout.flush()?;
    }
    Ok(0)
}

fn run_once(invocation: Invocation) -> Result<i32> {
    Ok(run_once_with_writer(invocation, &mut std::io::stdout().lock())?.exit_code)
}

struct ProcessOutcome {
    exit_code: i32,
    semantic_completion: SemanticCompletion,
}

impl ProcessOutcome {
    const fn complete(exit_code: i32) -> Self {
        Self {
            exit_code,
            semantic_completion: SemanticCompletion::Complete,
        }
    }
}

fn run_once_with_writer(invocation: Invocation, writer: &mut impl Write) -> Result<ProcessOutcome> {
    if let Some(option) = invocation.unknown_options.first() {
        writeln!(writer, "error TS5023: Unknown compiler option '{option}'.")?;
        return Ok(ProcessOutcome::complete(1));
    }
    let output = match prepare_invocation(&invocation)? {
        PreparedInvocation::Compile(output) => *output,
        PreparedInvocation::Diagnostic { code, message } => {
            writeln!(writer, "error TS{code}: {message}")?;
            return Ok(ProcessOutcome::complete(1));
        }
        PreparedInvocation::Help => {
            render_help(writer)?;
            return Ok(ProcessOutcome::complete(1));
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
    Ok(ProcessOutcome {
        exit_code: output.exit_status.code(),
        semantic_completion: output.semantic_completion,
    })
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
    let mut request = ProjectRequest::new(selection);
    if let Some(allow_js) = invocation.options.allow_js {
        request = request.with_allow_js(allow_js);
    }
    if let Some(out_dir) = &invocation.options.out_dir {
        request = request.with_out_dir(out_dir.clone());
    }
    if let Some(declaration_dir) = &invocation.options.declaration_dir {
        request = request.with_declaration_dir(declaration_dir.clone());
    }
    let mut resolved = resolve_project(&host, &request);
    if invocation.project.is_none()
        && invocation.files.is_empty()
        && resolved.graph.entry.is_none()
        && resolved.diagnostics.is_empty()
    {
        return Ok(PreparedInvocation::Help);
    }
    invocation.options.clear_overridden_origins(&mut resolved);
    let mut options = resolved.options.clone();
    invocation.options.apply_to(&mut options);
    if options
        .root_dir
        .as_ref()
        .is_some_and(|path| path.is_relative())
    {
        options.root_dir = options
            .root_dir
            .as_ref()
            .map(|path| current_directory.join(path));
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
    writeln!(
        writer,
        "Root files:                    {}",
        stats.root_files
    )?;
    writeln!(
        writer,
        "Source files:                  {}",
        stats.source_files
    )?;
    writeln!(
        writer,
        "Project configs:               {}",
        stats.project_configs
    )?;
    writeln!(
        writer,
        "Project references:            {}",
        stats.project_references
    )?;
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

fn render_help(writer: &mut impl Write) -> Result<()> {
    writeln!(
        writer,
        "tsz {VERSION}\n\nUsage: tsz [options] [file ...]\n\nOptions:\n  -p, --project <path>       Compile a project\n      --noEmit               Type-check without writing output\n      --noCheck              Emit without semantic checking\n      --declaration          Emit declaration files\n      --strict               Enable strict checking\n      --pretty <bool>        Enable pretty output\n      --extendedDiagnostics  Print phase timings\n      --batch                Read project paths from stdin"
    )?;
    Ok(())
}
