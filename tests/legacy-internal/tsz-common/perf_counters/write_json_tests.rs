mod write_json_tests {
    use super::*;

    #[test]
    fn write_json_to_writes_valid_json_with_atomic_rename() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tsz-perf-counter-snap-{}.json", std::process::id()));
        // Clean up beforehand if a stale file is sitting around.
        let _ = std::fs::remove_file(&path);
        PerfCounters::write_json_to(&path).expect("write succeeds");
        let raw = std::fs::read_to_string(&path).expect("read back");
        // Round-trip through serde to confirm structure.
        let value: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        assert_eq!(
            value["schema_version"],
            PERF_COUNTER_SNAPSHOT_SCHEMA_VERSION
        );
        assert!(value["wired"].is_object());
        // The atomic-rename `.json.tmp` should not be left behind.
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists(), "tmp file leaked: {tmp:?}");
        let _ = std::fs::remove_file(&path);
    }
}
