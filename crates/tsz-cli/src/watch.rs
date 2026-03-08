use anyhow::{Context, Result, bail};
use notify::{Config, Event, EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeSet;

use rustc_hash::FxHashSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tsz::checker::diagnostics::DiagnosticCategory;

use crate::args::{CliArgs, PollingWatchKind, WatchFileKind};
use crate::config::{ResolvedCompilerOptions, resolve_compiler_options};
use crate::driver::resolution::canonicalize_or_owned;
use crate::driver::{self, CompilationCache};
use crate::fs::{DEFAULT_EXCLUDES, is_ts_file};
use crate::reporter::Reporter;

/// Format a timestamp in tsc's `h:mm:ss tt` format (12-hour clock with AM/PM).
///
/// Uses local time via C `localtime_r` on Unix.
#[allow(unsafe_code)]
pub(crate) fn format_watch_timestamp() -> String {
    use std::time::SystemTime;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;

    // Use C localtime_r to get local time components
    #[cfg(unix)]
    {
        #[repr(C)]
        struct Tm {
            tm_sec: i32,
            tm_min: i32,
            tm_hour: i32,
            _tm_mday: i32,
            _tm_mon: i32,
            _tm_year: i32,
            _tm_wday: i32,
            _tm_yday: i32,
            _tm_isdst: i32,
            _tm_gmtoff: i64,
            _tm_zone: *const i8,
        }

        unsafe extern "C" {
            fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
        }

        let mut tm: Tm = unsafe { std::mem::zeroed() };
        unsafe {
            localtime_r(&secs, &mut tm);
        }
        let hour24 = tm.tm_hour as u32;
        let min = tm.tm_min as u32;
        let sec = tm.tm_sec as u32;
        format_12h(hour24, min, sec)
    }

    #[cfg(not(unix))]
    {
        // Fallback: use UTC (acceptable on non-Unix platforms for now)
        let total_secs = secs as u64;
        let hour24 = ((total_secs % 86400) / 3600) as u32;
        let min = ((total_secs % 3600) / 60) as u32;
        let sec = (total_secs % 60) as u32;
        format_12h(hour24, min, sec)
    }
}

/// Format hour/minute/second as `h:mm:ss AM/PM`.
pub(crate) fn format_12h(hour24: u32, min: u32, sec: u32) -> String {
    let (period, hour12) = if hour24 == 0 {
        ("AM", 12)
    } else if hour24 < 12 {
        ("AM", hour24)
    } else if hour24 == 12 {
        ("PM", 12)
    } else {
        ("PM", hour24 - 12)
    };
    format!("{hour12}:{min:02}:{sec:02} {period}")
}

/// Print the TS6031 watch start message to stdout.
fn print_watch_start() {
    let ts = format_watch_timestamp();
    println!("[{ts}] Starting compilation in watch mode...");
}

/// Print the TS6032 file change detected message to stdout.
fn print_watch_change() {
    let ts = format_watch_timestamp();
    println!("[{ts}] File change detected. Starting incremental compilation...");
}

/// Print the TS6194 watch completion message to stdout.
fn print_watch_complete(error_count: usize) {
    let ts = format_watch_timestamp();
    let error_word = if error_count == 1 { "error" } else { "errors" };
    println!("[{ts}] Found {error_count} {error_word}. Watching for file changes.");
}

const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(200);
const DEBOUNCE_TICK: Duration = Duration::from_millis(50);

/// Polling intervals for different strategies (matching tsc)
const FIXED_POLLING_INTERVAL: Duration = Duration::from_millis(250);
const PRIORITY_POLLING_INTERVAL_MEDIUM: Duration = Duration::from_millis(500);
const DYNAMIC_PRIORITY_POLLING_DEFAULT: Duration = Duration::from_millis(500);
const FIXED_CHUNK_SIZE_POLLING: Duration = Duration::from_millis(2000);

/// Wrapper for different watcher types
enum WatcherImpl {
    Native(RecommendedWatcher),
    Poll(PollWatcher),
}

impl WatcherImpl {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        match self {
            Self::Native(w) => w.watch(path, mode),
            Self::Poll(w) => w.watch(path, mode),
        }
    }
}

pub fn run(args: &CliArgs, cwd: &Path) -> Result<()> {
    let cwd = canonicalize_or_owned(cwd);
    let color = std::io::stdout().is_terminal();
    let mut reporter = Reporter::new(color);
    let mut state = WatchState::new(args, &cwd);

    print_watch_start();
    state.compile_and_report(args, &cwd, &mut reporter, None)?;

    let (tx, rx) = mpsc::channel();
    let mut watcher = create_watcher(args, tx)?;

    for root in &state.watch_roots {
        watcher
            .watch(root, RecursiveMode::Recursive)
            .with_context(|| format!("failed to watch {}", root.display()))?;
    }

    loop {
        match rx.recv_timeout(DEBOUNCE_TICK) {
            Ok(Ok(event)) => state.handle_event(event),
            Ok(Err(err)) => println!("watch error: {err}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("watch channel disconnected");
            }
        }

        if let Some(changed) = state.debouncer.flush_ready(Instant::now()) {
            print_watch_change();
            state.compile_and_report(args, &cwd, &mut reporter, Some(changed))?;
        }
    }
}

/// Create a watcher based on the specified watch strategy
fn create_watcher(args: &CliArgs, tx: mpsc::Sender<notify::Result<Event>>) -> Result<WatcherImpl> {
    // Determine polling interval for polling mode
    let poll_interval = match args.fallback_polling {
        Some(PollingWatchKind::FixedInterval) | None => FIXED_POLLING_INTERVAL,
        Some(PollingWatchKind::PriorityInterval) => PRIORITY_POLLING_INTERVAL_MEDIUM,
        Some(PollingWatchKind::DynamicPriority) => DYNAMIC_PRIORITY_POLLING_DEFAULT,
        Some(PollingWatchKind::FixedChunkSize) => FIXED_CHUNK_SIZE_POLLING,
    };

    // Determine which watcher to use based on watch_file strategy
    match args.watch_file {
        // Use polling for these strategies
        Some(WatchFileKind::FixedPollingInterval)
        | Some(WatchFileKind::PriorityPollingInterval)
        | Some(WatchFileKind::DynamicPriorityPolling)
        | Some(WatchFileKind::FixedChunkSizePolling) => {
            let config = Config::default().with_poll_interval(poll_interval);
            let watcher =
                PollWatcher::new(tx, config).context("failed to initialize poll watcher")?;
            Ok(WatcherImpl::Poll(watcher))
        }
        // Use native file system events (default and UseFsEvents strategies)
        Some(WatchFileKind::UseFsEvents)
        | Some(WatchFileKind::UseFsEventsOnParentDirectory)
        | None => {
            // Try native watcher first, fall back to polling if it fails
            match RecommendedWatcher::new(tx.clone(), Config::default()) {
                Ok(watcher) => Ok(WatcherImpl::Native(watcher)),
                Err(e) => {
                    println!("Warning: Native file watcher failed ({e}), falling back to polling");
                    let config = Config::default().with_poll_interval(poll_interval);
                    let watcher = PollWatcher::new(tx, config)
                        .context("failed to initialize fallback poll watcher")?;
                    Ok(WatcherImpl::Poll(watcher))
                }
            }
        }
    }
}

struct WatchState {
    base_dir: PathBuf,
    watch_roots: Vec<PathBuf>,
    filter: WatchFilter,
    debouncer: Debouncer,
    type_cache: CompilationCache,
}

impl WatchState {
    fn new(args: &CliArgs, cwd: &Path) -> Self {
        let ProjectState {
            base_dir,
            resolved,
            tsconfig_path,
        } = load_project_state(args, cwd).unwrap_or_else(|err| {
            println!("{err}");
            ProjectState {
                base_dir: canonicalize_or_owned(cwd),
                resolved: ResolvedCompilerOptions::default(),
                tsconfig_path: None,
            }
        });

        let explicit_files = resolve_explicit_files(&base_dir, &args.files);
        let watch_roots = collect_watch_roots(&base_dir, explicit_files.as_ref());
        let mut ignore_dirs = compute_ignore_dirs(&base_dir, &resolved);

        // Wire --excludeDirectories into the ignore list
        if let Some(ref exclude_dirs) = args.exclude_directories {
            for dir in exclude_dirs {
                let path = if dir.is_absolute() {
                    dir.clone()
                } else {
                    base_dir.join(dir)
                };
                ignore_dirs.push(canonicalize_or_owned(&path));
            }
        }

        // Collect --excludeFiles into an exclusion set
        let exclude_files = args.exclude_files.as_ref().map(|files| {
            let mut set = FxHashSet::default();
            for file in files {
                let path = if file.is_absolute() {
                    file.clone()
                } else {
                    base_dir.join(file)
                };
                set.insert(canonicalize_or_owned(&path));
            }
            set
        });

        let project_config = if args.project.is_some() {
            tsconfig_path
        } else {
            None
        };

        Self {
            base_dir,
            watch_roots,
            filter: WatchFilter::new(explicit_files, ignore_dirs, project_config, exclude_files),
            debouncer: Debouncer::new(DEFAULT_DEBOUNCE),
            type_cache: CompilationCache::default(),
        }
    }

    fn handle_event(&mut self, event: Event) {
        if !is_relevant_event(event.kind) {
            return;
        }

        let now = Instant::now();
        for path in event.paths {
            let path = canonicalize_or_owned(&normalize_event_path(&self.base_dir, &path));
            if self.filter.should_record(&path) {
                self.debouncer.record_at(now, path);
            }
        }
    }

    fn compile_and_report(
        &mut self,
        args: &CliArgs,
        cwd: &Path,
        reporter: &mut Reporter,
        changed_paths: Option<Vec<PathBuf>>,
    ) -> Result<()> {
        let changed_paths_ref = changed_paths.as_deref();
        let needs_full_rebuild =
            changed_paths_ref.is_some_and(|paths| self.needs_full_rebuild(paths));
        if needs_full_rebuild {
            self.type_cache.clear();
        }

        let result = if needs_full_rebuild || changed_paths_ref.is_none() {
            driver::compile_with_cache(args, cwd, &mut self.type_cache)
        } else if let Some(changed_paths) = changed_paths_ref {
            driver::compile_with_cache_and_changes(args, cwd, &mut self.type_cache, changed_paths)
        } else {
            driver::compile_with_cache(args, cwd, &mut self.type_cache)
        };

        // Clear console unless --preserveWatchOutput is set
        if !args.preserve_watch_output {
            // Clear screen (ANSI escape sequence)
            print!("\x1B[2J\x1B[H");
        }

        let error_count = match result {
            Ok(result) => {
                let count = result
                    .diagnostics
                    .iter()
                    .filter(|d| d.category == DiagnosticCategory::Error)
                    .count();

                if !result.diagnostics.is_empty() {
                    let output = reporter.render(&result.diagnostics);
                    if !output.is_empty() {
                        println!("{output}");
                    }
                }
                self.update_emitted(result.emitted_files);
                count
            }
            Err(err) => {
                println!("{err}");
                1
            }
        };

        print_watch_complete(error_count);

        if let Ok(project) = load_project_state(args, cwd) {
            self.filter.ignore_dirs = compute_ignore_dirs(&project.base_dir, &project.resolved);
            if args.project.is_some() {
                self.filter.project_config = project.tsconfig_path;
            }
        }

        Ok(())
    }

    fn needs_full_rebuild(&self, paths: &[PathBuf]) -> bool {
        paths
            .iter()
            .map(|path| canonicalize_or_owned(path))
            .any(|path| self.is_config_path(&path))
    }

    fn is_config_path(&self, path: &Path) -> bool {
        if let Some(project_config) = &self.filter.project_config {
            path == project_config
        } else {
            is_tsconfig_path(path)
        }
    }

    fn update_emitted(&mut self, emitted_files: Vec<PathBuf>) {
        let mut normalized = Vec::with_capacity(emitted_files.len());
        for path in emitted_files {
            normalized.push(normalize_event_path(&self.base_dir, &path));
        }
        self.filter.set_last_emitted(normalized);
        self.debouncer.remove_paths(&self.filter.last_emitted);
    }
}

struct ProjectState {
    base_dir: PathBuf,
    resolved: ResolvedCompilerOptions,
    tsconfig_path: Option<PathBuf>,
}

fn load_project_state(args: &CliArgs, cwd: &Path) -> Result<ProjectState> {
    let tsconfig_path = if args.ignore_config {
        None
    } else {
        driver::resolve_tsconfig_path(cwd, args.project.as_deref())?
    };
    let config = driver::load_config(tsconfig_path.as_deref())?;

    let mut resolved = resolve_compiler_options(
        config
            .as_ref()
            .and_then(|cfg| cfg.compiler_options.as_ref()),
    )?;
    driver::apply_cli_overrides(&mut resolved, args)?;

    let base_dir = driver::config_base_dir(cwd, tsconfig_path.as_deref());
    let base_dir = canonicalize_or_owned(&base_dir);

    Ok(ProjectState {
        base_dir,
        resolved,
        tsconfig_path,
    })
}

fn compute_ignore_dirs(base_dir: &Path, resolved: &ResolvedCompilerOptions) -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();
    for name in DEFAULT_EXCLUDES {
        dirs.insert(base_dir.join(name));
    }
    if let Some(out_dir) = driver::normalize_output_dir(base_dir, resolved.out_dir.clone()) {
        dirs.insert(out_dir);
    }
    if let Some(declaration_dir) =
        driver::normalize_output_dir(base_dir, resolved.declaration_dir.clone())
    {
        dirs.insert(declaration_dir);
    }
    dirs.into_iter().collect()
}

fn collect_watch_roots(
    base_dir: &Path,
    explicit_files: Option<&FxHashSet<PathBuf>>,
) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();
    roots.insert(base_dir.to_path_buf());

    if let Some(files) = explicit_files {
        for file in files {
            if let Some(parent) = file.parent() {
                roots.insert(parent.to_path_buf());
            }
        }
    }

    roots.into_iter().collect()
}

fn resolve_explicit_files(base_dir: &Path, files: &[PathBuf]) -> Option<FxHashSet<PathBuf>> {
    if files.is_empty() {
        return None;
    }

    let mut resolved = FxHashSet::default();
    for file in files {
        let path = if file.is_absolute() {
            file.to_path_buf()
        } else {
            base_dir.join(file)
        };
        resolved.insert(path);
    }

    Some(resolved)
}

const fn is_relevant_event(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    )
}

fn is_tsconfig_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "tsconfig.json")
}

fn is_default_excluded(path: &Path) -> bool {
    path.components().any(|component| {
        let std::path::Component::Normal(name) = component else {
            return false;
        };
        DEFAULT_EXCLUDES
            .iter()
            .any(|exclude| name == std::ffi::OsStr::new(exclude))
    })
}

fn normalize_event_path(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

pub(crate) struct WatchFilter {
    explicit_files: Option<FxHashSet<PathBuf>>,
    ignore_dirs: Vec<PathBuf>,
    last_emitted: FxHashSet<PathBuf>,
    project_config: Option<PathBuf>,
    exclude_files: Option<FxHashSet<PathBuf>>,
}

impl WatchFilter {
    pub(crate) fn new(
        explicit_files: Option<FxHashSet<PathBuf>>,
        ignore_dirs: Vec<PathBuf>,
        project_config: Option<PathBuf>,
        exclude_files: Option<FxHashSet<PathBuf>>,
    ) -> Self {
        Self {
            explicit_files,
            ignore_dirs,
            last_emitted: FxHashSet::default(),
            project_config,
            exclude_files,
        }
    }

    pub(crate) fn set_last_emitted<I>(&mut self, emitted: I)
    where
        I: IntoIterator<Item = PathBuf>,
    {
        self.last_emitted.clear();
        for path in emitted {
            self.last_emitted.insert(path);
        }
    }

    pub(crate) fn should_record(&self, path: &Path) -> bool {
        if self.last_emitted.contains(path) {
            return false;
        }

        if let Some(project_config) = &self.project_config {
            if path == project_config {
                return true;
            }
        } else if is_tsconfig_path(path) {
            return true;
        }

        if self.ignore_dirs.iter().any(|dir| path.starts_with(dir)) {
            return false;
        }

        if is_default_excluded(path) {
            return false;
        }

        // Check --excludeFiles
        if let Some(ref exclude_files) = self.exclude_files {
            if exclude_files.contains(path) {
                return false;
            }
        }

        if !is_ts_file(path) {
            return false;
        }

        if let Some(explicit) = &self.explicit_files {
            return explicit.contains(path);
        }

        true
    }
}

pub(crate) struct Debouncer {
    delay: Duration,
    pending: FxHashSet<PathBuf>,
    last_event_at: Option<Instant>,
}

impl Debouncer {
    pub(crate) fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: FxHashSet::default(),
            last_event_at: None,
        }
    }

    pub(crate) fn record_at(&mut self, now: Instant, path: PathBuf) {
        self.pending.insert(path);
        self.last_event_at = Some(now);
    }

    pub(crate) fn flush_ready(&mut self, now: Instant) -> Option<Vec<PathBuf>> {
        let last = self.last_event_at?;

        if now.duration_since(last) < self.delay || self.pending.is_empty() {
            return None;
        }

        self.last_event_at = None;
        Some(self.pending.drain().collect())
    }

    pub(crate) fn remove_paths(&mut self, paths: &FxHashSet<PathBuf>) {
        for path in paths {
            self.pending.remove(path);
        }

        if self.pending.is_empty() {
            self.last_event_at = None;
        }
    }
}
