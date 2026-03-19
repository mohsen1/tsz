use super::*;
use std::fs;
use tempfile::TempDir;

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

#[test]
fn test_trace_span_records_end_event_on_drop() {
    let mut tracer = Tracer::new();
    {
        let _span = TraceSpan::new(&mut tracer, "Emit", categories::EMIT);
    }

    let events = tracer.events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].name, "Emit");
    assert!(matches!(events[0].ph, Phase::Begin));
    assert!(matches!(events[1].ph, Phase::End));
}

#[test]
fn test_tracer_clear_removes_events() {
    let mut tracer = Tracer::new();
    tracer.instant("Parse", categories::PARSE);
    assert_eq!(tracer.events().len(), 1);

    tracer.clear();
    assert!(tracer.events().is_empty());
}

#[test]
fn test_phase_serializes_to_chrome_trace_codes() {
    let begin = serde_json::to_value(Phase::Begin).expect("serialize begin");
    let end = serde_json::to_value(Phase::End).expect("serialize end");
    let complete = serde_json::to_value(Phase::Complete).expect("serialize complete");
    let instant = serde_json::to_value(Phase::Instant).expect("serialize instant");
    let metadata = serde_json::to_value(Phase::Metadata).expect("serialize metadata");

    assert_eq!(begin, serde_json::json!("B"));
    assert_eq!(end, serde_json::json!("E"));
    assert_eq!(complete, serde_json::json!("X"));
    assert_eq!(instant, serde_json::json!("i"));
    assert_eq!(metadata, serde_json::json!("M"));
}

#[test]
fn test_trace_event_skips_empty_optional_fields() {
    let event = TraceEvent {
        name: "Parse".to_string(),
        cat: categories::PARSE.to_string(),
        ph: Phase::Begin,
        ts: 7,
        dur: None,
        pid: 11,
        tid: 3,
        args: FxHashMap::default(),
    };

    let json = serde_json::to_value(event).expect("serialize event");
    let obj = json.as_object().expect("object");

    assert_eq!(obj.get("name"), Some(&serde_json::json!("Parse")));
    assert_eq!(obj.get("cat"), Some(&serde_json::json!(categories::PARSE)));
    assert_eq!(obj.get("ph"), Some(&serde_json::json!("B")));
    assert_eq!(obj.get("ts"), Some(&serde_json::json!(7)));
    assert_eq!(obj.get("pid"), Some(&serde_json::json!(11)));
    assert_eq!(obj.get("tid"), Some(&serde_json::json!(3)));
    assert!(!obj.contains_key("dur"));
    assert!(!obj.contains_key("args"));
}

#[test]
fn test_tracer_complete_with_args_and_metadata_record_expected_payloads() {
    let mut tracer = Tracer::new();
    let start = Instant::now();
    let duration = Duration::from_micros(42);

    let mut complete_args = FxHashMap::default();
    complete_args.insert("phase".to_string(), serde_json::json!("parse"));
    tracer.complete_with_args("Parse", categories::PARSE, start, duration, complete_args);

    let mut metadata_args = FxHashMap::default();
    metadata_args.insert("name".to_string(), serde_json::json!("tsz"));
    tracer.metadata("thread_name", metadata_args);

    let events = tracer.events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].name, "Parse");
    assert!(matches!(events[0].ph, Phase::Complete));
    assert_eq!(events[0].dur, Some(42));
    assert_eq!(events[0].args.get("phase"), Some(&serde_json::json!("parse")));
    assert_eq!(events[1].cat, "__metadata");
    assert!(matches!(events[1].ph, Phase::Metadata));
    assert_eq!(events[1].args.get("name"), Some(&serde_json::json!("tsz")));
}

#[test]
fn test_write_to_file_creates_parent_dir_and_round_trips_json() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("nested/trace.json");

    let mut tracer = Tracer::new();
    tracer.instant("Emit", categories::EMIT);
    tracer.write_to_file(&path).expect("write trace");

    let contents = fs::read_to_string(&path).expect("read trace");
    let json: serde_json::Value = serde_json::from_str(&contents).expect("parse trace json");
    let events = json.as_array().expect("trace array");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["name"], serde_json::json!("Emit"));
    assert_eq!(events[0]["cat"], serde_json::json!(categories::EMIT));
    assert_eq!(events[0]["ph"], serde_json::json!("i"));
    assert!(path.exists());
}
