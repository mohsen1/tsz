# The CLI Surface and Diagnostic Reporting

This deep-dive fills a long-tail gap the boundary-level docs leave open: how a
raw `argv` becomes a command decision, and how a `Vec<Diagnostic>` becomes the
exact bytes `tsc` prints to a terminal (or pipe). The siblings
[End-to-End Compilation Timeline](end-to-end-timeline.md) and
[The Module-Resolution Engine and Re-Export Validation](module-resolution-engine.md)
describe what happens *inside* a compilation; this document is about the shell
that wraps it: command selection and `tsc`-compatible argument preprocessing
(`commands/args.rs`, `bin/tsz/arg_preprocess.rs`, `commands/help.rs`), the
reporter that renders pretty vs machine diagnostics and the summary line
(`reporting/reporter.rs`), watch-console rendering (`commands/watch.rs`), the
`--diagnostics`/`--extendedDiagnostics`/`--generateTrace` perf surfaces
(`bin/tsz/diagnostics_report.rs`, `reporting/trace.rs`,
`reporting/tracing_config.rs`), and locale message translation
(`localization/locale.rs`, `locales/*.json`). It also covers the family of
process exit codes that match TypeScript's `ExitStatus` enum.

The CLI is a *driving layer*. It owns program construction inputs, file
ordering at the argv level, the reporter, and process lifetime — but it runs no
type kernels. It calls `tsz_cli::driver::compile` to get a `CompilationResult`
(a `Vec<Diagnostic>` plus emit/file-list metadata) and then renders it. The
checker owns *which* diagnostics exist and *what order* they are in (via
`Diagnostic::compare`, see [Checker: The Error Reporter and Diagnostic
Construction](checker-error-reporter-diagnostics.md)); the reporter owns *how
they look on screen*. Those two responsibilities never cross: the reporter does
not invent, suppress, reorder, or recompute diagnostics, and the checker never
formats ANSI color or computes a relative file path. Where this doc touches a
diagnostic code, it is always a *CLI-emitted* code (argument errors, no-input
help, config-file-not-found) that the checker/solver never see — `TS5023`,
`TS5025`, `TS5042`, `TS5093`, `TS5112`, `TS6044`, `TS6046`, `TS6369`.

---

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| `argv` → command decision (`select_command`) | Type semantics, relations, inference (solver) |
| `tsc`-compat argv rewrites (case/kebab, `@file`, `--flag false`) | Diagnostic *content* and *ordering* (checker `Diagnostic::compare`) |
| CLI-only diagnostics (`TS5023`/`5025`/`5042`/`5093`/`5112`/`6044`/`6046`/`6369`) | Semantic diagnostics (`TS2xxx`/`TS1xxx`) — produced by checker |
| Pretty vs plain rendering, ANSI color, summary line | Source-position computation policy (uses `tsz::lsp::position::LineMap`) |
| Process exit codes (`ExitStatus` parity) | Emit transforms / output surgery (emitter) |
| `--diagnostics`/`--extendedDiagnostics`/`--generateTrace` report rendering | Phase-timing/cache *measurement* (driver/solver provide the numbers) |
| Locale selection + message-template substitution | Diagnostic message *templates* (`tsz_common::diagnostics`) |
| Watch-console framing (clear-screen, TS6031/6032/6194 lines, debounce) | File-graph re-resolution on change (driver `compile_with_cache_*`) |

---

## Module map

| Path | Role |
| --- | --- |
| `crates/tsz-cli/src/bin/tsz.rs` | Entry point: `main`, `select_command`, `Command` enum, `--showConfig`/`--listFilesOnly`/`--build` handlers, `--batch` mode |
| `crates/tsz-cli/src/bin/tsz/arg_preprocess.rs` | Pre-clap `argv` rewrite + early-exit directives (`PreprocessOutcome`, `EarlyExit`) |
| `crates/tsz-cli/src/bin/tsz/clap_errors.rs` | Reformat clap parse failures as `TS5023`/`TS5025`/`TS6044`/`TS6046`; edit-distance "did you mean" |
| `crates/tsz-cli/src/bin/tsz/run.rs` | Default compile command (`run_compile`): trace, file lists, render, exit code |
| `crates/tsz-cli/src/bin/tsz/diagnostics_report.rs` | `--diagnostics`/`--extendedDiagnostics` report build + render + RSS probe |
| `crates/tsz-cli/src/bin/tsz/init.rs` | `--init` tsconfig.json scaffolding (`TS5054`) |
| `crates/tsz-cli/src/bin/tsz/show_config.rs` | `--showConfig` resolved-config JSON |
| `crates/tsz-cli/src/commands/args.rs` | `CliArgs` clap struct + value enums (`Target`, `Module`, `JsxEmit`, …) |
| `crates/tsz-cli/src/commands/help.rs` | `render_help`/`render_help_all`/`colorize_help`, `TSC_VERSION` |
| `crates/tsz-cli/src/commands/build.rs` | `--build` up-to-date checks, build-info paths |
| `crates/tsz-cli/src/commands/watch.rs` | Watch loop, debouncer, file filter, console framing |
| `crates/tsz-cli/src/reporting/reporter.rs` | `Reporter`: pretty/plain render, snippet, underline, summary |
| `crates/tsz-cli/src/reporting/trace.rs` | `--generateTrace` Chrome trace-event writer (`Tracer`) |
| `crates/tsz-cli/src/reporting/tracing_config.rs` | `TSZ_LOG`/`TSZ_LOG_FORMAT` subscriber init |
| `crates/tsz-cli/src/localization/locale.rs` | Locale load/normalize, message-template parameter substitution |
| `crates/tsz-cli/src/locales/*.json` | TypeScript-derived translation tables (13 locales) |
| `crates/tsz-cli/src/perf_json.rs` | Perf-tools-only schema-versioned diagnostics JSON (`--diagnostics-json`) |
| `crates/tsz-cli/src/try_tsz/mod.rs` | `try-tsz` binary: run tsz + the project's `tsc`, diff diagnostics |

---

## Startup data flow

```
 OS argv (Vec<OsString>)
        │
        ▼
 init_tracing()                       (reporting/tracing_config.rs)
   TSZ_LOG / RUST_LOG / TSZ_PERF set?  → install stderr subscriber, else no-op
        │
        ▼
 preprocess_args(argv)                (bin/tsz/arg_preprocess.rs)
   • @file response-file expansion
   • canonicalize_long_flags (case/kebab → canonical --camelCase)
   • preparse_exit_directive  → --all / --help / --version  → EarlyExit code 0
   • preparse_unknown_rejection → "--", "-", --boolFlag=value → TS5023 code 1
   • remap_build_mode_flags    (-v→--build-verbose, -d→--dry, -f→--force)
   • normalize_bool_values_and_deduplicate (--flag false handling, dedup)
   • build_position_rejection  → --build/-b not first → TS6369/TS5023
        │   PreprocessOutcome::EarlyExit → println! + exit
        ▼   PreprocessOutcome::Continue(normalized argv)
 extract_batch_residency_budget_arg   (strip internal --batchResidencyBudget)
        │
        ▼
 CliArgs::try_parse_from(normalized)   (clap, commands/args.rs)
   Err → handle_clap_error             (bin/tsz/clap_errors.rs) → TS5023/5025/6044/6046
        │ Ok(args)
        ▼
 [optional] large-stack thread hop     (should_use_large_stack_thread)
        │
        ▼
 actual_main: validate_locale_or_exit → init_locale → select_command → dispatch
        │
        ▼
 Command::{Batch|Init|ShowConfig|ListFilesOnly|Build|Watch|Compile}
```

Three properties of this pipeline are worth stating precisely.

**Preprocessing is side-effect free.** `preprocess_args` never writes to stdout
or calls `process::exit`. Every quirk that requires printing-and-exiting before
clap (help, version, `--all`, `TS5023`, `TS6369`) is returned as a
`PreprocessOutcome::EarlyExit { message, code }`, and the entrypoint owns the
I/O (`println!("{message}")` + `std::process::exit(code)` at `bin/tsz.rs`
`main`). The doc comment calls this out as exactly what makes each rewrite and
pre-parse diagnostic unit-testable (`arg_preprocess.rs`, `into_continue`,
`early_exit` test helpers).

**The large-stack thread hop is selective.** `should_use_large_stack_thread`
returns true only for project-sized or multi-file workflows
(`--project`/`--build`/`--watch`/`--batch`, or any positional file). A bare
single-file probe runs on the original thread to shave startup latency; project
work re-spawns onto a thread with `tsz_common::limits::THREAD_STACK_SIZE_BYTES`
so deep recursive type evaluation does not overflow the default stack.

**Locale is validated before it is initialized.** `validate_locale_or_exit`
runs `locale::is_valid_locale_shape` first and, on a malformed `--locale`,
prints `TS` for the
`LOCALE_MUST_BE_OF_THE_FORM_LANGUAGE_OR_LANGUAGE_TERRITORY_FOR_EXAMPLE_OR`
diagnostic and exits 1 — matching `tsc`, which rejects shape before doing any
translation work.

---

## Argument preprocessing: the `tsc`-compatibility rewrites

`clap` cannot natively reproduce several `tsc` v6 quirks, so
`bin/tsz/arg_preprocess.rs` runs a fixed-order rewrite before clap sees the
arguments (`preprocess_args`):

1. **Response files** (`expand_response_files`). A token `@path` is replaced by
   the whitespace/quote-split contents of that file. `split_response_line`
   honors both `'` and `"` quoting and supports adjacent quoted regions
   (`foo"bar"baz` → `foobarbaz`). A read failure leaves the `@path` token
   untouched.

2. **Flag canonicalization** (`canonicalize_long_flags` →
   `canonicalize_long_flag`). `tsc` v6 accepts case-insensitive and kebab-case
   long flags. `flag_key_matches` compares against `KNOWN_TSC_OPTIONS`
   (`clap_errors.rs`) after stripping `-` and lowercasing both sides, so
   `--No-Emit` and `--noemit` both canonicalize to `--noEmit`. A small extra
   table maps internal spellings (`--verbose`→`--build-verbose`, `--batch`,
   `--diagnostics-json`, the hidden `--__explicitly-disabled-bool-flag`).

3. **Early-exit directives** (`preparse_exit_directive`). `--all` (precedence
   over `--help`), `--help`/`-h`/`-?`, then `--version`/`-V`/`-v`. `-v` is
   version *only outside build mode* — `is_build_mode` checks whether
   `argv[1]` is `--build`/`-b`; in build mode `-v` means `--build-verbose`. The
   directive returns rendered help text from `help::render_help`/
   `render_help_all` already passed through `help::colorize_help`.

4. **Unknown-rejection** (`preparse_unknown_rejection`). Bare `--` and `-`, and
   any `--boolFlag=value` (a boolean flag given in `--flag=value` form), are
   rejected as `TS5023` with the *whole token* echoed back, matching `tsc`
   (`--noEmit=true` → `error TS5023: Unknown compiler option '--noEmit=true'.`).

5. **Build-mode short-flag remap** (`remap_build_mode_flags`). When `argv[1]` is
   `--build`/`-b`, later `-v`/`-d`/`-f` become `--build-verbose`/`--dry`/
   `--force`.

6. **Boolean value normalization + dedup**
   (`normalize_bool_values_and_deduplicate`). This is the subtlest step:
   - For a tri-state `Option<bool>` flag (`OPTION_BOOL_FLAGS`, e.g.
     `--strictNullChecks`, `--pretty`), `--flag` alone or `--flag true`/`--flag
     false` is rewritten to `--flag=true`/`--flag=false` so clap can represent
     the explicit value.
   - For a plain `bool` flag (clap cannot store an explicit `false`), `--flag
     false` is *stripped* and a hidden side-channel token
     `--__explicitly-disabled-bool-flag=<name>` is appended. The override
     pipeline later reads `args.explicitly_disabled_bool_flags` to flip a
     `true` value loaded from `tsconfig.json` back to `false` — without this, a
     CLI `--noEmit false` could not override a config `"noEmit": true`.
   - Duplicate flags are deduplicated keeping the *last* occurrence
     (`--target ES2020 --target ES2022` → `--target ES2022`), with both the
     flag and its value token skipped for valued flags.

7. **Build position** (`build_position_rejection`). `--build` not first →
   `TS6369` ("must be the first command line argument"); `-b` not first →
   `TS5023`.

The three lookup tables that drive this — `BOOLEAN_FLAGS`, `VALUED_FLAGS`,
`OPTION_BOOL_FLAGS` — are hand-maintained alongside the `CliArgs` clap struct.
They are the source of truth for "does this flag consume the next token" before
clap exists to answer that.

### When clap still fails: `handle_clap_error`

If preprocessing passes but clap rejects (e.g. an unknown flag that survived
canonicalization, or an enum value out of range), `handle_clap_error`
(`clap_errors.rs`) translates the `clap::error::ErrorKind` into `tsc` codes:

- `UnknownArgument` → `extract_all_unknown_flags` re-scans the args against
  `collect_known_flags()` (built from `CliArgs::command()`), and for each
  unknown flag emits `TS5025` with a Levenshtein "did you mean" suggestion
  (`find_closest_option`, edit-distance ≤ ~40% of the longer name) or `TS5023`
  if nothing is close.
- `MissingRequiredArgument` / `InvalidValue` → `TS6044` ("expects an argument")
  and, for enum options, `TS6046` listing the valid values in `tsc`'s exact
  display order (`get_valid_values_for_option` — e.g. `target` lists
  `'es6', 'es2015', …, 'esnext'`).

All of these exit with code 1.

---

## Command selection

`select_command` (`bin/tsz.rs`) maps validated `CliArgs` to a `Command` value,
performing the same ordered validation `tsc` does and exiting in place on
invalid combinations. The order matters and is fixed:

```
--batch?               → Command::Batch
--init?                → Command::Init
reject_tsconfig_only_cli_options   (paths/plugins on CLI → TS6064)
reject_build_only_cli_options      (--dry/--force/--clean/… w/o --build → TS5093)
--showConfig?          → Command::ShowConfig
TS5112?  (implicit tsconfig + explicit files, no --ignoreConfig) → exit 1
--listFilesOnly + no input + no discoverable tsconfig → print help, exit 1
--listFilesOnly?       → Command::ListFilesOnly
--build?               → Command::Build
--watch?               → Command::Watch
no input + no --project + no discoverable tsconfig → print help, exit 1
lone directory positional → promote to --project  (#6002)
--project + files       → TS5042 ("cannot be mixed"), exit 1
merge_output_only_options_from_tsconfig  (#3860)
                       → Command::Compile
```

Several behaviors here are direct parity fixes:

- **No-input help** (`print_no_input_help_and_exit`): when there are no files,
  no `--project`, and `driver::find_tsconfig(cwd)` (a walk-up search) finds
  nothing, `tsc` v6 prints `Version <X>` followed by full help and exits 1.

- **Directory-positional promotion**: `tsz <dir>` is rewritten to
  `tsz --project <dir>` when `<dir>` is an existing directory and no
  `--project` was given. Without this, `<dir>` would be classified as a file
  input and trigger a spurious `TS5112` (#6002).

- **`TS5112`** (`should_report_ts5112_for_command_line_files`): a `tsconfig.json`
  in `cwd` is present but will not be loaded when explicit files are given;
  `--ignoreConfig` opts out.

- **Output-only option merge** (`merge_output_only_options_from_tsconfig`,
  #3860): `tsc` honors `listFiles`, `listEmittedFiles`, `explainFiles`,
  `diagnostics`, `extendedDiagnostics`, `traceResolution` from `tsconfig.json`
  even though they are normally CLI-side. This is a best-effort pre-read of the
  top-level `compilerOptions` (tolerating JSONC via
  `tsz_cli::config::normalize_jsonc`) that ORs the config values into `args`
  before the CLI gates in `run_compile` consult them. The full config resolver
  still runs later inside the driver.

---

## The default compile command: `run_compile`

`run_compile` (`bin/tsz/run.rs`) is the execution half of the parse→execute
split. By the time it runs, arguments are validated and normalized. It:

1. Rejects `--generateCpuProfile` with an explicit "not supported; use
   `--generateTrace`" message and exit 1 (tsz cannot emit V8 CPU profiles).
2. Optionally constructs a `Tracer` (if `--generateTrace`), times the compile
   with `std::time::Instant`, and calls `driver::compile(args, cwd)`.
3. Maps driver `Err` strings prefixed `TS6053:`/`TS6231:` (root-file resolution
   failures) into the `tsc` "file is in the program because: Root file specified
   for compilation" format via `report_root_file_diagnostic`.
4. Writes the trace file (per-file `FileProcessed` instant events plus a
   `Compile` complete event with file/error/emit counts).
5. Honors the post-compile listing flags: `--listFiles`
   (`result.files_read`), `--listEmittedFiles` (`TSFILE: <path>` lines),
   `--explainFiles` (`result.file_infos` with inclusion reasons),
   `--traceDependencies`.
6. Prints the `--diagnostics`/`--extendedDiagnostics` report.
7. Renders `result.diagnostics` through a `Reporter` (see below).
8. Computes the exit code.

### Exit codes — `ExitStatus` parity

The three status constants in `bin/tsz.rs` mirror TypeScript's `ExitStatus`:

| Constant | Value | Meaning |
| --- | --- | --- |
| `EXIT_SUCCESS` | 0 | No errors (or `--soundReportOnly`, which always exits 0) |
| `EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED` | 1 | Errors present, no outputs generated |
| `EXIT_DIAGNOSTICS_OUTPUTS_GENERATED` | 2 | Errors present but outputs were still generated |

`run_compile`'s exit logic:

```rust
if args.sound_report_only { exit(EXIT_SUCCESS); }   // audit mode never blocks
let has_errors = result.diagnostics.iter().any(|d| d.category == Error);
if has_errors {
    if args.no_emit_on_error          { exit(1); }  // emit suppressed
    else if result.no_emit
         || !result.emitted_files.is_empty() { exit(2); }  // outputs exist / noEmit
    else                               { exit(1); }
}
exit(EXIT_SUCCESS);
```

`result.no_emit` reflects the *resolved* option (CLI + tsconfig), so a
config-only `noEmit` selects exit 2 just like the CLI flag. `--build` uses a
parallel computation per project (`bin/tsz.rs` `handle_build`): exit 2 if any
project built and had errors, exit 1 if nothing built.

---

## The reporter

`Reporter` (`reporting/reporter.rs`) is the single owner of diagnostic
rendering. It holds two booleans (`pretty`, `color`), a `cwd` for relative-path
computation, and per-file caches of source text (`sources`) and `LineMap`s
(`line_maps`). It does **not** receive diagnostics until the driver has already
ordered them via `Diagnostic::compare` (see
[Checker: The Error Reporter and Diagnostic Construction](checker-error-reporter-diagnostics.md));
`render` walks them in the given order.

### Pretty vs plain selection

The pretty/color decision lives at every call site, not inside `Reporter`:

```rust
let pretty = args.pretty.unwrap_or_else(|| std::io::stdout().is_terminal());
if args.pretty == Some(true) { Reporter::force_colors(true); }
let mut reporter = Reporter::new(pretty);
```

`--pretty` is a tri-state `Option<bool>`. When unset, the default tracks TTY
detection (`IsTerminal`). When explicitly `true`, `Reporter::force_colors(true)`
calls `colored::control::set_override(true)` so ANSI codes are emitted **even
when piped** — matching `tsc` v6, which treats `--pretty true` as a hard
override of TTY detection. `Reporter::new(color)` sets both `pretty` and `color`
to the same value initially; `set_pretty` can decouple them.

### Plain format

`render_plain` produces `file(line,col): error TScode: message`, one diagnostic
per line, no source snippet, no summary, and always a trailing newline when any
diagnostic was printed. Related information is indented two spaces per nesting
level (`format_related_plain`, `2 * (depth + 1)`), so a deep elaboration chain
stays visible as structure even in machine output. This is the format the
conformance batch worker uses (`--pretty false`).

### Pretty format

`render_pretty` produces:

```
file:line:col - error TScode: message

{n} {source line}
    {spaces}{tildes}

<blank>
Found N errors …
```

Notable details, all matched to `tsc`'s exact bytes:

- The header uses `bright_cyan` for the file, `bright_yellow` for line/col,
  `bright_black` for the `TScode:` segment, and a category color from
  `format_category_label` (red error / yellow-bold warning / blue-bold
  suggestion / cyan-bold message).
- `format_snippet_pretty` prints the offending source line with a
  reverse-video line number, then an underline row. `build_underline` walks the
  line char-by-char, emitting a space (or four spaces for a tab) before the
  span and `~` (or `~~~~` for a tab) within it — so tabs expand consistently in
  both the source row and the underline. If the computed underline is empty but
  `length > 0`, it falls back to a single `~` at the column.
- Related information (`format_related_pretty`) is indented two spaces for the
  location line and four for the snippet/message, with a `bright_cyan`
  underline rather than red.
- `format_summary` renders `tsc`'s three summary shapes: `Found 1 error in
  file:line`, `Found N errors in the same file, starting at: file:line`, or
  `Found N errors in M files.` followed by an `Errors  Files` table (right-
  aligned count, `:line` in `bright_black`). Only `DiagnosticCategory::Error`
  counts toward the summary; warnings/suggestions are printed but not summed.

### Relative paths

`relative_path` reproduces `tsc` v6's behavior of producing `../../path` for
files outside `cwd`. The subtlety is macOS `/tmp` → `/private/tmp` symlink
mismatch: the compiler stores canonical file paths, but a caller may set `cwd`
to the symlink form. `relative_path` first diffs as-is; if the result starts
with `..` (the symptom of a canonical-vs-symlink mismatch), it retries with the
file's `canonicalize()`d form and prefers it only if it is genuinely shorter.
`set_cwd` canonicalizes the override path for the same reason.

### Source loading and encoding

`ensure_source` reads each referenced file lazily and decodes via
`decode_source_bytes`, which handles UTF-16 LE/BE BOM files (TypeScript test
fixtures) by reconstructing `u16` words, falling back to UTF-8. Position
computation (`position_for`) builds a `tsz::lsp::position::LineMap` per file and
converts a byte offset to a 1-based `(line, col)` — the reporter consumes the
shared LSP position machinery rather than reimplementing line counting.

---

## Watch mode console rendering

`commands/watch.rs` `run` owns the watch loop. The console framing is the
CLI-surface concern; the actual recompilation goes through the incremental
driver (`driver::compile_with_cache`/`compile_with_cache_and_changes`, see
[Driver: Incremental and Watch](driver-incremental-and-watch.md)).

```
print_watch_start (TS6031: "Starting compilation in watch mode...")
compile_and_report
loop:
  rx.recv_timeout(DEBOUNCE_TICK=50ms)
  Debouncer.flush_ready (200ms quiet window) → changed paths
    print_watch_change (TS6032: "File change detected...")
    [clear screen unless --preserveWatchOutput]
    compile_and_report
    print_watch_complete (TS6194: "Found N errors. Watching for file changes.")
```

Key rendering details:

- **Timestamps**: `format_watch_timestamp` formats `h:mm:ss tt` (12-hour clock
  with AM/PM) using C `localtime_r` on Unix; `format_colored_timestamp` wraps it
  in `[...]` and grays it with `\x1b[90m…\x1b[0m` when color is on, matching
  `tsc` v6's bracketed timestamp.
- **Screen clearing**: unless `--preserveWatchOutput`, each rebuild prints
  `\x1B[2J\x1B[3J\x1B[H` (clear screen + scrollback + home cursor), the exact
  sequence `tsc` v6 uses.
- **Completion line**: `print_watch_complete` pluralizes "error"/"errors" and is
  the watch analog of the one-shot summary; the diagnostics themselves are still
  rendered through the same `Reporter`.

The watch *filter* (`WatchFilter::should_record`) and `Debouncer` decide which
filesystem events trigger a recompile: tsconfig changes always force a full
rebuild (`needs_full_rebuild` → `type_cache.clear()`); emitted files are
suppressed (`last_emitted`) so the compiler does not loop on its own output;
`node_modules`/`dist`/out-dirs and `--excludeDirectories`/`--excludeFiles` are
filtered out. The native-vs-poll watcher choice (`create_watcher`) maps
`--watchFile`/`--fallbackPolling` to a `notify` `RecommendedWatcher` or
`PollWatcher`, falling back to polling if the native watcher fails to
initialize.

---

## The `--diagnostics` / `--extendedDiagnostics` report

`bin/tsz/diagnostics_report.rs` renders `tsc`'s post-compile performance
report. It is structured as a strict collect → render split so the renderer is
pure and golden-testable:

- `collect_file_lines` is the only I/O step: it reads each file in
  `result.files_read` and tallies line counts into `FileLinesStats`, categorized
  by extension (library `lib.*.d.ts`, definitions `.d.ts`/`.d.mts`/`.d.cts`,
  TypeScript, JavaScript, JSON, other).
- `build_diagnostics_report` flattens a `CompilationResult` into a
  `DiagnosticsReport` of primitives. The *basic* section is always populated;
  the *extended* section pulls cache counters
  (`result.request_cache_counters`), interner stats, solver relation-cache
  stats (`query_cache_stats`: subtype/assignability/eval/application-eval/
  instantiation/property/variance entries + hit rates), definition-store stats
  (`def_store_stats`), AST residency stats (`residency_stats`), module
  dependency-graph stats (`module_dep_stats`), and a perf-counter dump (only
  non-empty when `TSZ_PERF_COUNTERS` is set).
- `render_diagnostics_report` writes the column-aligned text. The basic block
  always emits `Files`, the six `Lines of …` rows, `Errors`, phase timings (when
  present), and `Total time`; the extended block appends cache/memory/residency
  rows. Phase-timing fields come from `result.phase_timings` (`io_read_ms`,
  `load_libs_ms + parse_bind_ms`, `check_ms`, `emit_ms`) — the CLI does not
  measure phases, it renders numbers the driver supplied.
- `get_memory_usage_kb` reports *peak* RSS via `getrusage(RUSAGE_SELF)` (bytes
  on macOS, KB on Linux), matching `tsc`'s `--extendedDiagnostics` memory line;
  0 on non-Unix.

The golden tests (`basic_golden_full_output`, `extended_golden_full_output`)
pin the exact byte layout, including the 31-column field padding.

### Perf-tools-only JSON (`--diagnostics-json`)

Under `--features perf-tools`, `run_compile` can also write a schema-versioned
`PerfDiagnosticsReport` (`perf_json.rs`, `SCHEMA_VERSION = 2`) consumed by the
bench harness via `jq`. The flag, the module, and the call site all compile out
of default release builds, so there is no runtime cost for normal users.

---

## `--generateTrace` and `TSZ_LOG` tracing

These are two unrelated trace surfaces.

**`--generateTrace`** (`reporting/trace.rs`) writes a Chrome Trace Event Format
JSON array loadable in `chrome://tracing` or Perfetto. `Tracer` records
`TraceEvent`s with phases `B`/`E`/`X`/`i`/`M` (begin/end/complete/instant/
metadata). `run_compile` records a `process_name` metadata event up front, a
`Compile` complete event with file/error/emit counts, and a `FileProcessed`
instant per file. `write_to_file` serializes pretty JSON, creating the parent
directory; if `--generateTrace` points at a directory, the file is `trace.json`
inside it. This is the substitute the CLI offers for `tsc`'s
`--generateCpuProfile` (which tsz explicitly rejects).

**`TSZ_LOG` / `TSZ_LOG_FORMAT`** (`reporting/tracing_config.rs`,
`init_tracing`) is the developer-debugging subscriber, installed *only* when
`TSZ_LOG`, `RUST_LOG`, or `TSZ_PERF` is set — zero cost otherwise. `TSZ_LOG`
takes precedence over `RUST_LOG`; `TSZ_LOG_FORMAT` selects `text` (flat fmt),
`tree` (`tracing-tree` hierarchical), or `json` (per-event JSON with thread ids,
for parallel-check attribution). All output goes to **stderr** so it never
contaminates stdout — which carries diagnostics, `--showConfig` JSON, or LSP
JSON-RPC. This is the surface the `tsz-tracing` skill drives; see also
[Driver: Parallelism and Determinism](driver-parallelism-and-determinism.md)
for why JSON mode tags thread ids.

---

## Localization

`localization/locale.rs` provides `--locale` message translation against 13
embedded TypeScript locale tables (`locales/*.json`, `include_str!`'d at build
time). The flow:

```
init_locale(Some("ja"))            → LocaleMessages::load → static LOCALE OnceLock
Reporter::translate_message(code, message_text)
   → locale::translate(code, fallback)
       if default locale or no translation for code → return fallback verbatim
       else fetch translated template, substitute parameters, return
```

`normalize_locale` maps many spellings/aliases to a canonical id (`ja`,
`ja-jp`, `japanese` → `ja`; `zh`, `zh-hans`, `chinese` → `zh-cn`).
`parse_locale_json` keys each entry by the trailing numeric segment of the
TypeScript message key (`Cannot_find_name_0_2304` → code `2304`), via
`extract_code_from_key`.

The non-trivial part is **parameter substitution**. Translated templates carry
positional `{0}`, `{1}` placeholders, but the diagnostic that reaches the
reporter is already *formatted English* (e.g. `Type 'string' is not assignable
to type 'number'.`). `substitute_params_from_english` recovers the parameters by
matching the formatted English against the *English template* obtained from
`tsz_common::diagnostics::get_message_template` (`parse_template_parts` splits
the template into `Literal`/`Placeholder` parts and
`extract_params_from_template` reads back the spans between literals). This
recovers even unquoted parameters such as `TS2554`'s numeric argument counts.
When the template lookup fails, it falls back to `extract_quoted_strings` (the
classic single-quote heuristic). The recovered parameters are then substituted
into the translated template.

`is_valid_locale_shape` enforces `tsc`'s accepted form before any of this runs:
one or two `-`/`_`-separated ASCII-alphabetic segments (`en`, `ja-jp`), nothing
else.

---

## `--init`, `--showConfig`, `--listFilesOnly`, `--batch`

A handful of commands never reach `run_compile`:

- **`--init`** (`bin/tsz/init.rs` `handle_init`): scaffolds a `tsconfig.json`
  from a template, refusing with `TS5054` ("A 'tsconfig.json' file is already
  defined at …") if one exists, and prints `Created a new tsconfig.json` on
  success. CLI flags are folded into the template via `collect_init_overrides`.

- **`--showConfig`** (`bin/tsz.rs` `handle_show_config`): loads the tsconfig
  through the diagnostic loader (`load_tsconfig_with_diagnostics`), reports
  config errors (`TS5057`/`TS5058`/`TS5081`/`TS5112`) through a `Reporter`, then
  prints the resolved compiler-options map and file/exclude lists
  (`show_config::*`).

- **`--listFilesOnly`** (`bin/tsz.rs` `handle_list_files_only`): resolves config
  + CLI overrides, discovers files (`discover_ts_files`), prints lib files first
  (matching `tsc` order) then the discovered files. It surfaces the
  `allowJs`-disabled JS-root diagnostic
  (`FILE_IS_A_JAVASCRIPT_FILE_DID_YOU_MEAN_TO_ENABLE_THE_ALLOWJS_OPTION`) with a
  "file is in the program because" related chain, exiting 1.

- **`--batch`** (`bin/tsz.rs` `run_batch_mode`): the conformance runner's
  process-pool protocol. It reads project paths from stdin one per line,
  compiles each with `--noEmit --pretty false`, renders diagnostics through a
  `Reporter` whose `cwd` is set to the project path, and prints a
  `---TSZ-BATCH-DONE---` sentinel per project. Between iterations it calls
  `clear_batch_iteration_state` to drop **all** thread-local caches (solver
  construction cache, subtype state, checker thread-locals, and the module
  resolver's `is_file`/`is_dir` existence caches) — otherwise a reused worker
  could read a stale `TypeId → TypeData` mapping or a stale filesystem answer
  from a prior compilation (#13368/#13255 family). The hidden
  `--batchResidencyBudget` probe optionally appends residency-pressure lines and
  forces an extra cache clear under medium/high pressure.

---

## `try-tsz`: the parity-audit binary

`try_tsz/mod.rs` is a separate binary (`TryTszArgs`, entered via
`bin/try_tsz.rs`) that runs both compilers on a real project and diffs them.
`run_tsc` launches the bundled `tsc_diagnostics_helper.js` under Node. The npm
launcher points it at the package's pinned TypeScript `7.0.2`; a local build may
instead resolve a project installation at `7.0.2` or newer. The root
`typescript` export supplies the version, while the helper dynamically imports
`typescript/unstable/sync` because TypeScript 7 no longer exports the stable
root compiler API.

The helper reads the root config as JSONC through `jsonc-parser`. If needed, it
creates an in-memory edit that sets `compilerOptions.noEmit` to `true`, rejecting
duplicate or non-object/non-boolean shapes whose meaning would be ambiguous.
That edited text is exposed only for the root config through the sync API's
virtual `readFile` callback; all other reads return `undefined` and fall back to
the real filesystem. Nothing is written to the user's config, source tree,
dependency directory, lockfile, or build-info path. Config diagnostic positions
are mapped back through the inverse edit so they still identify the original
JSONC.

The helper opens the project with
`API.updateSnapshot({ openProjects: [configPath] })`, collects the compiler's
structured diagnostic stages, deduplicates them, converts TypeScript 7's UTF-16
offsets to UTF-8 byte offsets, then disposes the snapshot and closes the API.
The resulting JSON is the `tsc --pretty false --noEmit -p <config>` oracle; the
CLI adapter owns orchestration only, while TypeScript owns the semantic result.

`run_tsz` executes tsz **as an isolated subprocess** (`--try-tsz-worker`) so a
tsz crash/timeout/OOM is captured as a `ResultState` rather than killing the
harness. The worker (`run_tsz_worker`) runs `driver::compile` under
`catch_unwind`, returning panics as exit 102 and errors as exit 101.
`diff_diagnostics` produces a multiset diff
(`extra_tsz`/`missing_tsc`/`order_mismatches`) over a `ComparableDiagnostic`
projection, with config-deprecation codes (`5101`/`5107`) normalized to drop
their location so the diff is position-insensitive
(`normalize_config_deprecation_location`). This binary is a *consumer* of the
same driver and diagnostics, not part of the compile path.

---

## Caches and invariants

- **Reporter per-file caches** (`sources`, `line_maps`): populated lazily on
  first reference, never invalidated within a single `render` (the files do not
  change mid-render). A fresh `Reporter` is constructed per render call, so
  there is no cross-render staleness.
- **`static LOCALE: OnceLock<LocaleMessages>`** (`locale.rs`): set exactly once
  by `init_locale` at startup; `get_locale` reads it. A process compiles in one
  locale.
- **`colored::control` override** (`Reporter::force_colors`): a global ANSI
  toggle. `--pretty true` sets it; `--pretty false`/default leaves TTY detection
  in charge. This is process-global state, which is why batch mode constructs a
  plain `Reporter::new(false)` rather than relying on the override.
- **Batch thread-local reset invariant**: every batch iteration must call
  `clear_batch_iteration_state` because `TypeId`/`NodeIndex`/`Atom` handles are
  per-compilation dense indices that a reused worker would otherwise alias
  across compilations. This is the same worker-reuse isolation contract that
  governs the parallel checker.
- **Side-effect-free preprocessing invariant**: `preprocess_args` and the
  `arg_preprocess` helpers never print or exit; all early-exit I/O is owned by
  `main`. Tests assert both the rewritten argv and the `EarlyExit` directives.
- **stdout/stderr separation**: diagnostics, `--showConfig`, `--listFilesOnly`,
  and LSP JSON-RPC go to **stdout**; the `TSZ_LOG` tracing subscriber goes to
  **stderr**. This split is load-bearing for machine consumers.

---

## Edge cases and `tsc` parity

- **`--pretty true` forces color when piped.** TTY detection is the default,
  but explicit `--pretty true` overrides it via `colored::control::set_override`
  — `tsc` v6 behavior.
- **`-v` is context-dependent.** Version outside build mode, `--build-verbose`
  inside it. `is_build_mode` keys off `argv[1]`.
- **`--build` must be first.** `--build` not first → `TS6369`; `-b` not first →
  `TS5023`. The asymmetry (different codes for long vs short) mirrors `tsc`.
- **`--boolFlag=value` is an unknown option.** `tsc` does not accept
  `--noEmit=true`; it reports the whole token as `TS5023`.
- **`--flag false` for a plain bool must override config.** The hidden
  `--__explicitly-disabled-bool-flag` side channel exists solely so a CLI
  `--noEmit false` can flip a `tsconfig` `"noEmit": true` to false (clap cannot
  store an explicit `false` for a plain flag).
- **Lone directory positional becomes `--project`.** `tsz <dir>` ≡
  `tsz --project <dir>` (#6002), avoiding a spurious `TS5112`.
- **No-input prints version + full help and exits 1**, not a terse usage line.
- **`--soundReportOnly` always exits 0.** Sound-mode audit prints every
  diagnostic but never blocks the build.
- **Exit-2 vs exit-1 distinction.** Errors *with* generated outputs (or
  `noEmit`) → 2; errors with *no* outputs (or `--noEmitOnError`) → 1.
- **UTF-16 BOM source files** still get correct positions because
  `decode_source_bytes` reconstructs them; otherwise the reporter would omit the
  `(line,col)` prefix.
- **Tabs expand to four columns** consistently in both the source line and the
  underline (`build_underline`), so the `~` markers line up the way `tsc`
  renders them.
- **Config-deprecation codes are position-normalized in `try-tsz`** (`5101`,
  `5107`) so a location difference between tsz and `tsc` does not register as a
  diagnostic mismatch.

---

## Related reading

- [End-to-End Compilation Timeline](end-to-end-timeline.md) — what `driver::compile` does between argv and `CompilationResult`.
- [Checker: The Error Reporter and Diagnostic Construction](checker-error-reporter-diagnostics.md) — diagnostic content, codes, and `Diagnostic::compare` ordering the reporter renders.
- [The Module-Resolution Engine and Re-Export Validation](module-resolution-engine.md) — how `files_read`/`file_infos`/inclusion reasons (consumed by `--explainFiles`) are produced.
- [Driver: Incremental and Watch](driver-incremental-and-watch.md) — the incremental recompile behind the watch console.
- [Driver: Project References and Build Mode](driver-project-references-and-build-mode.md) — `--build` graph, up-to-date checks, and per-project exit codes.
- [Driver: Parallelism and Determinism](driver-parallelism-and-determinism.md) — why JSON tracing tags thread ids and why batch mode resets thread-locals.
- [LSP and WASM Surfaces](lsp-and-wasm-surfaces.md) — the other driving-layer consumers that share the diagnostics and position machinery.
