//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/parallel/lib_snapshot.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a55114bbaf62a0dc685cfd9c5af30939b11e592f3586a93e8dec46ce82f16508 414 snapshot_round_trips_via_bincode
    #[test]
    fn snapshot_round_trips_via_bincode() {
        let lib = parse_and_bind(
            "snap_test.d.ts",
            "interface Promise<T> { then(): Promise<T>; } export const x = 1;",
        );
        let snapshot = LibSnapshot {
            file_name: "snap_test.d.ts".to_string(),
            content_hash: 0xdeadbeef,
            arena: (*lib.arena).clone(),
            binder: (*lib.binder).clone(),
            root_index: lib.root_index,
        };
        let bytes = encode_snapshot(&snapshot).expect("encode");
        let decoded = decode_snapshot(&bytes).expect("decode");
        assert_eq!(decoded.file_name, "snap_test.d.ts");
        assert_eq!(decoded.content_hash, 0xdeadbeef);
        assert_eq!(decoded.root_index, lib.root_index);
        // Symbols round-tripped: re-look-up Promise should return same SymbolId.
        let original_promise = lib.binder.file_locals.get("Promise");
        let restored_promise = decoded.binder.file_locals.get("Promise");
        assert_eq!(original_promise, restored_promise);
    }
// TSZ_INLINE_TEST_END a55114bbaf62a0dc685cfd9c5af30939b11e592f3586a93e8dec46ce82f16508

// TSZ_INLINE_TEST_BEGIN 83653a08544f29fbaa8c75bde701688f7c48d7ab72ca92ebcf00e3cdde693b54 438 snapshot_set_round_trips_ordered_libs
    #[test]
    #[allow(unsafe_code)]
    fn snapshot_set_round_trips_ordered_libs() {
        // SAFETY: nextest runs each test in its own process, so the env
        // mutations don't race other threads.
        unsafe {
            std::env::set_var(ENV_VAR, "1");
        }
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        unsafe {
            std::env::set_var(ENV_DIR, tmp.path());
        }

        let first_name = "lib.first.d.ts";
        let first_source = "interface First { value: string; }";
        let second_name = "lib.second.d.ts";
        let second_source = "interface Second { value: number; }";
        let first = parse_and_bind(first_name, first_source);
        let second = parse_and_bind(second_name, second_source);
        let keys = vec![
            (first_name, content_hash(first_name, first_source)),
            (second_name, content_hash(second_name, second_source)),
        ];

        try_store_many(&keys, &[Arc::clone(&first), Arc::clone(&second)])
            .expect("set write should succeed");

        let restored = try_load_many(&keys).expect("set cache should hit");
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].file_name, first_name);
        assert_eq!(restored[1].file_name, second_name);
        assert_eq!(
            restored[0].binder.file_locals.get("First"),
            first.binder.file_locals.get("First")
        );
        assert_eq!(
            restored[1].binder.file_locals.get("Second"),
            second.binder.file_locals.get("Second")
        );

        let mut wrong_order = keys.clone();
        wrong_order.reverse();
        assert!(try_load_many(&wrong_order).is_none());

        let mut wrong_hash = keys;
        wrong_hash[0].1 ^= 1;
        assert!(try_load_many(&wrong_hash).is_none());
    }
// TSZ_INLINE_TEST_END 83653a08544f29fbaa8c75bde701688f7c48d7ab72ca92ebcf00e3cdde693b54

// TSZ_INLINE_TEST_BEGIN 04753772ef57ef3d10153f86754f7335deb7e5cd3bd08b4098bde5a0ea0d682e 487 snapshot_rejects_wrong_magic
    #[test]
    fn snapshot_rejects_wrong_magic() {
        let bad = b"XXXX\x00\x00\x00\x00rest";
        assert!(decode_snapshot(bad).is_err());
    }
// TSZ_INLINE_TEST_END 04753772ef57ef3d10153f86754f7335deb7e5cd3bd08b4098bde5a0ea0d682e

// TSZ_INLINE_TEST_BEGIN da3c11a9c9b455766a230712e86a1463f55fe46a5ff5d7cfd1508043a3c92f6b 493 cache_enablement_defaults_on_and_accepts_off_values
    #[test]
    fn cache_enablement_defaults_on_and_accepts_off_values() {
        assert!(enabled_from_env_value(None));
        assert!(enabled_from_env_value(Some("")));
        assert!(enabled_from_env_value(Some("1")));
        assert!(enabled_from_env_value(Some("on")));
        assert!(enabled_from_env_value(Some("true")));
        assert!(enabled_from_env_value(Some("yes")));

        assert!(!enabled_from_env_value(Some("0")));
        assert!(!enabled_from_env_value(Some("off")));
        assert!(!enabled_from_env_value(Some("false")));
        assert!(!enabled_from_env_value(Some("no")));
        assert!(!enabled_from_env_value(Some(" OFF ")));
    }
// TSZ_INLINE_TEST_END da3c11a9c9b455766a230712e86a1463f55fe46a5ff5d7cfd1508043a3c92f6b

// TSZ_INLINE_TEST_BEGIN e9603d2cd8ddab0d4f6ce1fbb622bb506cd161ad62ede2790d252eb50be01d19 509 snapshot_temp_paths_are_unique_siblings
    #[test]
    fn snapshot_temp_paths_are_unique_siblings() {
        let path = Path::new("/tmp/0123456789abcdef.bin");
        let first = snapshot_temp_path(path);
        let second = snapshot_temp_path(path);

        assert_ne!(first, second);
        assert_eq!(first.parent(), path.parent());
        assert_eq!(second.parent(), path.parent());
        assert!(
            first
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".tmp")
        );
        assert!(
            second
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(".tmp")
        );
    }
// TSZ_INLINE_TEST_END e9603d2cd8ddab0d4f6ce1fbb622bb506cd161ad62ede2790d252eb50be01d19

// TSZ_INLINE_TEST_BEGIN 9de7887427aca8eaf64531cc0885654a05c9d46bdc16f127bb574d0a678ce8bb 538 disk_round_trip_resolves_identifier_text_and_symbols
    /// End-to-end disk round-trip: write a snapshot, read it back, and
    /// verify identifier text resolves correctly through the
    /// reconstituted arena AND that bound symbols (Promise, greeting,
    /// declared modules) are intact.
    #[test]
    #[allow(unsafe_code)]
    fn disk_round_trip_resolves_identifier_text_and_symbols() {
        // SAFETY: nextest runs each test in its own process, so the env
        // mutations don't race other threads.
        unsafe {
            std::env::set_var(ENV_VAR, "1");
        }
        let tmp = tempfile::TempDir::new().expect("tmp dir");
        unsafe {
            std::env::set_var(ENV_DIR, tmp.path());
        }

        let file_name = "snapshot_e2e.d.ts";
        let source = "interface Promise<T> { then(): Promise<T>; }\nconst greeting = \"hi\";\ndeclare module \"virtual:env\" { export const VAL: string; }\n";

        let lib = parse_and_bind(file_name, source);
        let original_promise_id = lib.binder.file_locals.get("Promise");
        let original_greeting_id = lib.binder.file_locals.get("greeting");
        let original_module_count = lib.binder.declared_modules.len();
        assert!(original_promise_id.is_some());
        assert!(original_greeting_id.is_some());

        try_store(file_name, source, &lib).expect("first write should succeed");

        // Cache hit: round-trip through disk.
        let restored = try_load(file_name, source).expect("cache should hit");

        // Symbols match.
        assert_eq!(
            restored.binder.file_locals.get("Promise"),
            original_promise_id
        );
        assert_eq!(
            restored.binder.file_locals.get("greeting"),
            original_greeting_id
        );
        assert_eq!(
            restored.binder.declared_modules.len(),
            original_module_count
        );
        assert!(restored.binder.declared_modules.contains("virtual:env"));

        // Identifier text resolves through the restored arena.
        let mut found_promise = false;
        let mut found_greeting = false;
        for raw in 0..restored.arena.len() {
            let idx = tsz_parser::NodeIndex(u32::try_from(raw).expect("index fits"));
            let Some(node) = restored.arena.get(idx) else {
                continue;
            };
            if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                continue;
            }
            let Some(data) = restored.arena.get_identifier(node) else {
                continue;
            };
            let text = restored.arena.interner.resolve(data.atom);
            if text == "Promise" {
                found_promise = true;
            }
            if text == "greeting" {
                found_greeting = true;
            }
        }
        assert!(found_promise, "Promise identifier text round-tripped");
        assert!(found_greeting, "greeting identifier text round-tripped");

        // Negative-cache assertions.
        assert!(try_load("other_file.d.ts", source).is_none());
        assert!(try_load(file_name, "const z = 0;").is_none());
    }
// TSZ_INLINE_TEST_END 9de7887427aca8eaf64531cc0885654a05c9d46bdc16f127bb574d0a678ce8bb

// TSZ_INLINE_TEST_BEGIN 4a56acb6cb006c60c49421874494cfe94d0efe78871ff7cbb72cf3248505e87a 611 content_hash_is_stable_and_distinguishes_inputs
    #[test]
    fn content_hash_is_stable_and_distinguishes_inputs() {
        let h1 = content_hash("a.d.ts", "const x = 1;");
        let h2 = content_hash("a.d.ts", "const x = 1;");
        let h3 = content_hash("a.d.ts", "const x = 2;");
        let h4 = content_hash("b.d.ts", "const x = 1;");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h1, h4);
    }
// TSZ_INLINE_TEST_END 4a56acb6cb006c60c49421874494cfe94d0efe78871ff7cbb72cf3248505e87a
