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
