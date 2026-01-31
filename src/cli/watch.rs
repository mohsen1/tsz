use anyhow::{Context, Result, bail};
use notify::{Config, Event, EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{BTreeSet, HashSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::cli::args::{CliArgs, PollingWatchKind, WatchFileKind};
use crate::cli::config::{ResolvedCompilerOptions, resolve_compiler_options};
use crate::cli::driver::{self, CompilationCache};
use crate::cli::fs::{DEFAULT_EXCLUDES, is_ts_file};
use crate::cli::reporter::Reporter;

const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(200);
const DEBOUNCE_TICK: Duration = Duration::from_millis(50);

/// Polling intervals for different strategies (matching tsc)
const FIXED_POLLING_INTERVAL: Duration = Duration::from_millis(250);
#[allow(dead_code)]
const PRIORITY_POLLING_INTERVAL_HIGH: Duration = Duration::from_millis(250);
const PRIORITY_POLLING_INTERVAL_MEDIUM: Duration = Duration::from_millis(500);
#[allow(dead_code)]
const PRIORITY_POLLING_INTERVAL_LOW: Duration = Duration::from_millis(2000);
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
            WatcherImpl::Native(w) => w.watch(path, mode),
            WatcherImpl::Poll(w) => w.watch(path, mode),
        }
    }
}

pub fn run(args: &CliArgs, cwd: &Path) -> Result<()> {
    let cwd = canonicalize_or_owned(cwd);
    let color = std::io::stdout().is_terminal();
    let mut reporter = Reporter::new(color);
    let mut state = WatchState::new(args, &cwd);

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
            Ok(Err(err)) => eprintln!("watch error: {err}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("watch channel disconnected");
            }
        }

        if let Some(changed) = state.debouncer.flush_ready(Instant::now()) {
            state.compile_and_report(args, &cwd, &mut reporter, Some(changed))?;
        }
    }
}

/// Create a watcher based on the specified watch strategy
fn create_watcher(args: &CliArgs, tx: mpsc::Sender<notify::Result<Event>>) -> Result<WatcherImpl> {
    // Determine polling interval for polling mode
    let poll_interval = match args.fallback_polling {
        Some(PollingWatchKind::FixedInterval) => FIXED_POLLING_INTERVAL,
        Some(PollingWatchKind::PriorityInterval) => PRIORITY_POLLING_INTERVAL_MEDIUM,
        Some(PollingWatchKind::DynamicPriority) => DYNAMIC_PRIORITY_POLLING_DEFAULT,
        Some(PollingWatchKind::FixedChunkSize) => FIXED_CHUNK_SIZE_POLLING,
        None => FIXED_POLLING_INTERVAL,
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
                    eprintln!(
                        "Warning: Native file watcher failed ({}), falling back to polling",
                        e
                    );
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
            eprintln!("{err}");
            ProjectState {
                base_dir: canonicalize_or_owned(cwd),
                resolved: ResolvedCompilerOptions::default(),
                tsconfig_path: None,
            }
        });

        let explicit_files = resolve_explicit_files(&base_dir, &args.files);
        let watch_roots = collect_watch_roots(&base_dir, explicit_files.as_ref());
        let ignore_dirs = compute_ignore_dirs(&base_dir, &resolved);
        let project_config = if args.project.is_some() {
            tsconfig_path.clone()
        } else {
            None
        };

        WatchState {
            base_dir,
            watch_roots,
            filter: WatchFilter::new(explicit_files, ignore_dirs, project_config),
            debouncer: Debouncer::new(DEFAULT_DEBOUNCE),
            type_cache: CompilationCache::default(),
        }
    }

    fn handle_event(&mut self, event: Event) {
        if !is_relevant_event(&event.kind) {
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
        let needs_full_rebuild = changed_paths_ref
            .map(|paths| self.needs_full_rebuild(paths))
            .unwrap_or(false);
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

        match result {
            Ok(result) => {
                if !result.diagnostics.is_empty() {
                    let output = reporter.render(&result.diagnostics);
                    if !output.is_empty() {
                        eprintln!("{output}");
                    }
                }
                self.update_emitted(result.emitted_files);
            }
            Err(err) => eprintln!("{err}"),
        }

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
    let tsconfig_path = driver::resolve_tsconfig_path(cwd, args.project.as_deref())?;
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

fn collect_watch_roots(base_dir: &Path, explicit_files: Option<&HashSet<PathBuf>>) -> Vec<PathBuf> {
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

fn resolve_explicit_files(base_dir: &Path, files: &[PathBuf]) -> Option<HashSet<PathBuf>> {
    if files.is_empty() {
        return None;
    }

    let mut resolved = HashSet::new();
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

fn is_relevant_event(kind: &EventKind) -> bool {
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

fn canonicalize_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) struct WatchFilter {
    explicit_files: Option<HashSet<PathBuf>>,
    ignore_dirs: Vec<PathBuf>,
    last_emitted: HashSet<PathBuf>,
    project_config: Option<PathBuf>,
}

impl WatchFilter {
    pub(crate) fn new(
        explicit_files: Option<HashSet<PathBuf>>,
        ignore_dirs: Vec<PathBuf>,
        project_config: Option<PathBuf>,
    ) -> Self {
        WatchFilter {
            explicit_files,
            ignore_dirs,
            last_emitted: HashSet::new(),
            project_config,
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
    pending: HashSet<PathBuf>,
    last_event_at: Option<Instant>,
}

impl Debouncer {
    pub(crate) fn new(delay: Duration) -> Self {
        Debouncer {
            delay,
            pending: HashSet::new(),
            last_event_at: None,
        }
    }

    pub(crate) fn record_at(&mut self, now: Instant, path: PathBuf) {
        self.pending.insert(path);
        self.last_event_at = Some(now);
    }

    pub(crate) fn flush_ready(&mut self, now: Instant) -> Option<Vec<PathBuf>> {
        let Some(last) = self.last_event_at else {
            return None;
        };

        if now.duration_since(last) < self.delay || self.pending.is_empty() {
            return None;
        }

        self.last_event_at = None;
        Some(self.pending.drain().collect())
    }

    pub(crate) fn remove_paths(&mut self, paths: &HashSet<PathBuf>) {
        for path in paths {
            self.pending.remove(path);
        }

        if self.pending.is_empty() {
            self.last_event_at = None;
        }
    }
}
