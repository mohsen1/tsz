//! Performance tracing support for the `--generateTrace` flag.
//!
//! Generates Chrome DevTools compatible trace files that can be loaded in
//! chrome://tracing or the Perfetto UI (https://ui.perfetto.dev/).
//!
//! # Trace Format
//!
//! The trace file is a JSON array of trace events following the Chrome Trace
//! Event Format specification.
//!
//! # Usage
//!
//! ```ignore
//! use tsz::cli::trace::Tracer;
//!
//! let mut tracer = Tracer::new();
//! tracer.begin("Parse", "file.ts");
//! // ... do parsing ...
//! tracer.end("Parse", "file.ts");
//! tracer.write_to_file("trace/trace.json")?;
//! ```

use rustc_hash::FxHashMap;
use serde::Serialize;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

/// Trace event phases (Chrome Trace Event Format)
#[derive(Debug, Clone, Copy, Serialize)]
pub enum Phase {
    /// Duration event begin
    #[serde(rename = "B")]
    Begin,
    /// Duration event end
    #[serde(rename = "E")]
    End,
    /// Complete event (duration with explicit duration)
    #[serde(rename = "X")]
    Complete,
    /// Instant event
    #[serde(rename = "i")]
    Instant,
    /// Metadata event
    #[serde(rename = "M")]
    Metadata,
}

/// A single trace event
#[derive(Debug, Clone, Serialize)]
pub struct TraceEvent {
    /// Event name
    pub name: String,
    /// Event category
    pub cat: String,
    /// Phase (B=begin, E=end, X=complete, i=instant, M=metadata)
    pub ph: Phase,
    /// Timestamp in microseconds
    pub ts: u64,
    /// Duration in microseconds (for complete events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur: Option<u64>,
    /// Process ID
    pub pid: u32,
    /// Thread ID
    pub tid: u32,
    /// Additional arguments
    #[serde(skip_serializing_if = "FxHashMap::is_empty")]
    pub args: FxHashMap<String, serde_json::Value>,
}

/// Categories for trace events
pub mod categories {
    pub const PROGRAM: &str = "program";
    pub const PARSE: &str = "parse";
    pub const BIND: &str = "bind";
    pub const CHECK: &str = "check";
    pub const EMIT: &str = "emit";
    pub const IO: &str = "io";
    pub const MODULE_RESOLUTION: &str = "moduleResolution";
}

/// Performance tracer that collects timing information
#[derive(Debug)]
pub struct Tracer {
    events: Vec<TraceEvent>,
    start_time: Instant,
    active_spans: FxHashMap<String, Instant>,
    pid: u32,
    tid: u32,
}

impl Tracer {
    /// Create a new tracer
    pub fn new() -> Self {
        Tracer {
            events: Vec::new(),
            start_time: Instant::now(),
            active_spans: FxHashMap::default(),
            pid: std::process::id(),
            tid: 1, // Main thread
        }
    }

    /// Get timestamp in microseconds since tracer start
    fn timestamp(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }

    /// Begin a duration event
    pub fn begin(&mut self, name: &str, category: &str) {
        let ts = self.timestamp();
        let key = format!("{}:{}", category, name);
        self.active_spans.insert(key, Instant::now());

        self.events.push(TraceEvent {
            name: name.to_string(),
            cat: category.to_string(),
            ph: Phase::Begin,
            ts,
            dur: None,
            pid: self.pid,
            tid: self.tid,
            args: FxHashMap::default(),
        });
    }

    /// Begin a duration event with arguments
    pub fn begin_with_args(
        &mut self,
        name: &str,
        category: &str,
        args: FxHashMap<String, serde_json::Value>,
    ) {
        let ts = self.timestamp();
        let key = format!("{}:{}", category, name);
        self.active_spans.insert(key, Instant::now());

        self.events.push(TraceEvent {
            name: name.to_string(),
            cat: category.to_string(),
            ph: Phase::Begin,
            ts,
            dur: None,
            pid: self.pid,
            tid: self.tid,
            args,
        });
    }

    /// End a duration event
    pub fn end(&mut self, name: &str, category: &str) {
        let ts = self.timestamp();
        let key = format!("{}:{}", category, name);
        self.active_spans.remove(&key);

        self.events.push(TraceEvent {
            name: name.to_string(),
            cat: category.to_string(),
            ph: Phase::End,
            ts,
            dur: None,
            pid: self.pid,
            tid: self.tid,
            args: FxHashMap::default(),
        });
    }

    /// Record a complete event with known duration
    pub fn complete(&mut self, name: &str, category: &str, start: Instant, duration: Duration) {
        let ts = (start.duration_since(self.start_time)).as_micros() as u64;
        let dur = duration.as_micros() as u64;

        self.events.push(TraceEvent {
            name: name.to_string(),
            cat: category.to_string(),
            ph: Phase::Complete,
            ts,
            dur: Some(dur),
            pid: self.pid,
            tid: self.tid,
            args: FxHashMap::default(),
        });
    }

    /// Record a complete event with arguments
    pub fn complete_with_args(
        &mut self,
        name: &str,
        category: &str,
        start: Instant,
        duration: Duration,
        args: FxHashMap<String, serde_json::Value>,
    ) {
        let ts = (start.duration_since(self.start_time)).as_micros() as u64;
        let dur = duration.as_micros() as u64;

        self.events.push(TraceEvent {
            name: name.to_string(),
            cat: category.to_string(),
            ph: Phase::Complete,
            ts,
            dur: Some(dur),
            pid: self.pid,
            tid: self.tid,
            args,
        });
    }

    /// Record an instant event
    pub fn instant(&mut self, name: &str, category: &str) {
        let ts = self.timestamp();

        self.events.push(TraceEvent {
            name: name.to_string(),
            cat: category.to_string(),
            ph: Phase::Instant,
            ts,
            dur: None,
            pid: self.pid,
            tid: self.tid,
            args: FxHashMap::default(),
        });
    }

    /// Record an instant event with arguments
    pub fn instant_with_args(
        &mut self,
        name: &str,
        category: &str,
        args: FxHashMap<String, serde_json::Value>,
    ) {
        let ts = self.timestamp();

        self.events.push(TraceEvent {
            name: name.to_string(),
            cat: category.to_string(),
            ph: Phase::Instant,
            ts,
            dur: None,
            pid: self.pid,
            tid: self.tid,
            args,
        });
    }

    /// Add metadata event (e.g., process/thread names)
    pub fn metadata(&mut self, name: &str, args: FxHashMap<String, serde_json::Value>) {
        self.events.push(TraceEvent {
            name: name.to_string(),
            cat: "__metadata".to_string(),
            ph: Phase::Metadata,
            ts: 0,
            dur: None,
            pid: self.pid,
            tid: self.tid,
            args,
        });
    }

    /// Write the trace to a file
    pub fn write_to_file(&self, path: &Path) -> std::io::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);

        // Write as JSON array (Chrome Trace Event Format)
        serde_json::to_writer_pretty(&mut writer, &self.events)?;
        writer.flush()?;

        Ok(())
    }

    /// Get all recorded events
    pub fn events(&self) -> &[TraceEvent] {
        &self.events
    }

    /// Clear all events
    pub fn clear(&mut self) {
        self.events.clear();
        self.active_spans.clear();
    }
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for tracing a span
pub struct TraceSpan<'a> {
    tracer: &'a mut Tracer,
    name: String,
    category: String,
    #[allow(dead_code)]
    start: Instant,
}

impl<'a> TraceSpan<'a> {
    /// Create a new trace span
    pub fn new(tracer: &'a mut Tracer, name: &str, category: &str) -> Self {
        tracer.begin(name, category);
        TraceSpan {
            tracer,
            name: name.to_string(),
            category: category.to_string(),
            start: Instant::now(),
        }
    }
}

impl Drop for TraceSpan<'_> {
    fn drop(&mut self) {
        self.tracer.end(&self.name, &self.category);
    }
}

/// Macro to trace a scope
#[macro_export]
macro_rules! trace_span {
    ($tracer:expr, $name:expr, $category:expr) => {
        let _span = $crate::trace::TraceSpan::new($tracer, $name, $category);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracer_basic() {
        let mut tracer = Tracer::new();

        tracer.begin("Parse", categories::PARSE);
        std::thread::sleep(Duration::from_millis(1));
        tracer.end("Parse", categories::PARSE);

        assert_eq!(tracer.events().len(), 2);
        assert_eq!(tracer.events()[0].name, "Parse");
        assert!(matches!(tracer.events()[0].ph, Phase::Begin));
        assert!(matches!(tracer.events()[1].ph, Phase::End));
    }

    #[test]
    fn test_tracer_complete_event() {
        let mut tracer = Tracer::new();
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(10));
        let duration = start.elapsed();

        tracer.complete("Check", categories::CHECK, start, duration);

        assert_eq!(tracer.events().len(), 1);
        assert!(tracer.events()[0].dur.is_some());
        assert!(tracer.events()[0].dur.unwrap() >= 10000); // At least 10ms in microseconds
    }

    #[test]
    fn test_tracer_with_args() {
        let mut tracer = Tracer::new();
        let mut args = FxHashMap::default();
        args.insert("file".to_string(), serde_json::json!("test.ts"));

        tracer.instant_with_args("FileRead", categories::IO, args);

        assert_eq!(tracer.events().len(), 1);
        assert!(tracer.events()[0].args.contains_key("file"));
    }
}
