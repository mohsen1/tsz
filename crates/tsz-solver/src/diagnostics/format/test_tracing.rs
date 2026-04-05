//! Test utilities for capturing tracing spans and events in unit tests.
//!
//! Uses `tracing::subscriber::with_default()` for per-test subscriber isolation,
//! which is safe for parallel test execution — each test gets its own subscriber
//! scoped to its closure.
//!
//! # Example
//!
//! ```rust,ignore
//! use tsz_solver::test_tracing::with_test_tracing;
//!
//! let (result, capture) = with_test_tracing(|| {
//!     let mut checker = SubtypeChecker::new(&interner);
//!     checker.check_subtype(source, target)
//! });
//! assert!(capture.has_span("check_subtype"));
//! assert!(capture.span_count("check_subtype") > 0);
//! ```

use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

/// A captured tracing span.
#[derive(Debug, Clone)]
pub struct CapturedSpan {
    /// The span name (e.g., "check_subtype", "evaluate_type").
    pub name: String,
    /// The tracing level (TRACE, DEBUG, INFO, etc.).
    pub level: tracing::Level,
    /// Key-value fields recorded on the span.
    pub fields: Vec<(String, String)>,
}

/// A captured tracing event.
#[derive(Debug, Clone)]
pub struct CapturedEvent {
    /// The event message (from the `message` field).
    pub message: String,
    /// The tracing level.
    pub level: tracing::Level,
    /// All key-value fields on the event.
    pub fields: Vec<(String, String)>,
}

/// Buffer for captured tracing output during a test.
///
/// Thread-safe via `Arc<Mutex<...>>` — can be cloned into the capture layer
/// and queried from the test after the closure completes.
#[derive(Debug, Clone, Default)]
pub struct TracingCapture {
    spans: Arc<Mutex<Vec<CapturedSpan>>>,
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl TracingCapture {
    /// Create a new empty capture buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all captured spans.
    pub fn spans(&self) -> Vec<CapturedSpan> {
        self.spans.lock().unwrap().clone()
    }

    /// Get all captured events.
    pub fn events(&self) -> Vec<CapturedEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Check if any span with the given name was entered.
    pub fn has_span(&self, name: &str) -> bool {
        self.spans.lock().unwrap().iter().any(|s| s.name == name)
    }

    /// Check if any event message contains the given substring.
    pub fn has_event_containing(&self, substring: &str) -> bool {
        self.events
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.message.contains(substring))
    }

    /// Count spans matching a name.
    pub fn span_count(&self, name: &str) -> usize {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.name == name)
            .count()
    }

    /// Find all spans matching a name and return their fields.
    pub fn spans_named(&self, name: &str) -> Vec<CapturedSpan> {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.name == name)
            .cloned()
            .collect()
    }

    /// Get the value of a field from the first span with the given name.
    pub fn span_field(&self, span_name: &str, field_name: &str) -> Option<String> {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.name == span_name)
            .and_then(|s| {
                s.fields
                    .iter()
                    .find(|(k, _)| k == field_name)
                    .map(|(_, v)| v.clone())
            })
    }
}

/// A tracing `Layer` that captures spans and events into a `TracingCapture` buffer.
struct CaptureLayer {
    capture: TracingCapture,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = FieldCollector(&mut fields);
        attrs.record(&mut visitor);
        self.capture.spans.lock().unwrap().push(CapturedSpan {
            name: attrs.metadata().name().to_string(),
            level: *attrs.metadata().level(),
            fields,
        });
    }

    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = FieldCollector(&mut fields);
        event.record(&mut visitor);
        let message = fields
            .iter()
            .find(|(k, _)| k == "message")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        self.capture.events.lock().unwrap().push(CapturedEvent {
            message,
            level: *event.metadata().level(),
            fields,
        });
    }
}

/// Visitor that collects span/event fields into a `Vec<(String, String)>`.
struct FieldCollector<'a>(&'a mut Vec<(String, String)>);

impl tracing::field::Visit for FieldCollector<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .push((field.name().to_string(), format!("{value:?}")));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

/// Run a closure with a test-scoped tracing subscriber that captures all
/// spans and events.
///
/// Returns `(closure_result, capture)` so you can inspect both the return
/// value and the tracing output.
///
/// Safe for parallel tests because `with_default` sets the subscriber
/// only for the current thread during the closure.
pub fn with_test_tracing<F, R>(f: F) -> (R, TracingCapture)
where
    F: FnOnce() -> R,
{
    let capture = TracingCapture::new();
    let layer = CaptureLayer {
        capture: capture.clone(),
    };
    let subscriber = Registry::default().with(layer);
    let result = with_default(subscriber, f);
    (result, capture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_basic_span() {
        let ((), capture) = with_test_tracing(|| {
            let _span = tracing::debug_span!("test_span", x = 42).entered();
        });
        assert!(capture.has_span("test_span"));
        assert_eq!(capture.span_count("test_span"), 1);
        assert_eq!(capture.span_field("test_span", "x"), Some("42".to_string()));
    }

    #[test]
    fn capture_basic_event() {
        let ((), capture) = with_test_tracing(|| {
            tracing::debug!("hello world");
        });
        assert!(capture.has_event_containing("hello world"));
    }

    #[test]
    fn capture_nested_spans() {
        let ((), capture) = with_test_tracing(|| {
            let _outer = tracing::debug_span!("outer").entered();
            let _inner = tracing::trace_span!("inner", depth = 1).entered();
        });
        assert!(capture.has_span("outer"));
        assert!(capture.has_span("inner"));
        assert_eq!(capture.span_field("inner", "depth"), Some("1".to_string()));
    }

    #[test]
    fn capture_subtype_check_span() {
        use crate::intern::TypeInterner;
        use crate::relations::subtype::SubtypeChecker;
        use crate::types::TypeId;

        let interner = TypeInterner::new();
        let (result, capture) = with_test_tracing(|| {
            let mut checker = SubtypeChecker::new(&interner);
            checker.check_subtype(TypeId::STRING, TypeId::NUMBER)
        });

        // string is not a subtype of number
        assert!(result.is_false());

        // The check_subtype span should have been captured
        assert!(capture.has_span("check_subtype"));

        // Verify the span recorded the correct type IDs
        let spans = capture.spans_named("check_subtype");
        assert!(!spans.is_empty());
        let first = &spans[0];
        assert_eq!(
            first
                .fields
                .iter()
                .find(|(k, _)| k == "src")
                .map(|(_, v)| v.as_str()),
            Some("10") // TypeId::STRING = 10
        );
    }

    #[test]
    fn capture_is_isolated_between_tests() {
        // First capture
        let ((), capture1) = with_test_tracing(|| {
            let _span = tracing::debug_span!("test_a").entered();
        });

        // Second capture — should NOT see test_a's span
        let ((), capture2) = with_test_tracing(|| {
            let _span = tracing::debug_span!("test_b").entered();
        });

        assert!(capture1.has_span("test_a"));
        assert!(!capture1.has_span("test_b"));
        assert!(capture2.has_span("test_b"));
        assert!(!capture2.has_span("test_a"));
    }
}
