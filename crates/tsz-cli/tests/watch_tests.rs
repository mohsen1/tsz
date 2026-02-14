use super::watch::{Debouncer, WatchFilter};
use rustc_hash::FxHashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn debouncer_flushes_after_delay() {
    let mut debouncer = Debouncer::new(Duration::from_millis(100));
    let now = Instant::now();

    debouncer.record_at(now, PathBuf::from("src/a.ts"));
    assert!(
        debouncer
            .flush_ready(now + Duration::from_millis(99))
            .is_none()
    );

    let flushed = debouncer
        .flush_ready(now + Duration::from_millis(101))
        .expect("should flush after delay");

    assert_eq!(flushed.len(), 1);
    assert!(flushed.contains(&PathBuf::from("src/a.ts")));
}

#[test]
fn debouncer_resets_timer_on_new_event() {
    let mut debouncer = Debouncer::new(Duration::from_millis(100));
    let now = Instant::now();

    debouncer.record_at(now, PathBuf::from("src/a.ts"));
    debouncer.record_at(now + Duration::from_millis(50), PathBuf::from("src/b.ts"));

    assert!(
        debouncer
            .flush_ready(now + Duration::from_millis(120))
            .is_none()
    );

    let flushed = debouncer
        .flush_ready(now + Duration::from_millis(160))
        .expect("should flush after last event delay");

    assert_eq!(flushed.len(), 2);
}

#[test]
fn watch_filter_ignores_outputs_and_excludes() {
    let base_dir = std::env::temp_dir().join("tsz_watch_filter");
    let out_dir = base_dir.join("dist");

    let explicit = base_dir.join("src/index.ts");
    let other = base_dir.join("src/other.ts");
    let node_module = base_dir.join("node_modules/pkg/index.ts");
    let output_js = out_dir.join("index.js");
    let tsconfig = base_dir.join("tsconfig.json");

    let mut explicit_set = FxHashSet::default();
    explicit_set.insert(explicit.clone());

    let filter = WatchFilter::new(Some(explicit_set), vec![out_dir], None);

    assert!(filter.should_record(&explicit));
    assert!(!filter.should_record(&other));
    assert!(!filter.should_record(&node_module));
    assert!(!filter.should_record(&output_js));
    assert!(filter.should_record(&tsconfig));
}

#[test]
fn watch_filter_respects_emitted_files() {
    let base_dir = std::env::temp_dir().join("tsz_watch_filter_emitted");
    let emitted = base_dir.join("types/index.d.ts");

    let mut filter = WatchFilter::new(None, Vec::new(), None);
    filter.set_last_emitted(vec![emitted.clone()]);

    assert!(!filter.should_record(&emitted));
}

#[test]
fn watch_filter_records_project_config() {
    let base_dir = std::env::temp_dir().join("tsz_watch_filter_project");
    let config = base_dir.join("configs/tsconfig.build.json");
    let other_config = base_dir.join("tsconfig.json");

    let filter = WatchFilter::new(None, Vec::new(), Some(config.clone()));

    assert!(filter.should_record(&config));
    assert!(!filter.should_record(&other_config));
}
