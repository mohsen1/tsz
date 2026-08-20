//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_completions_snippets.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 36722a42ca62a4036584ebe812581c7c1bd55efba219e0b1b6ae245915404d99 1445 prioritize_class_member_snippet_items_keeps_snippet_variant_for_same_label
    #[test]
    fn prioritize_class_member_snippet_items_keeps_snippet_variant_for_same_label() {
        let items = vec![
            CompletionItem::new("container".to_string(), CompletionItemKind::Property)
                .with_source("@sapphire/pieces".to_string()),
            CompletionItem::new("container".to_string(), CompletionItemKind::Property)
                .with_source("ClassMemberSnippet/".to_string())
                .with_has_action()
                .as_snippet()
                .with_insert_text("container: Container;".to_string()),
            CompletionItem::new("other".to_string(), CompletionItemKind::Property),
        ];

        let prioritized = Server::prioritize_class_member_snippet_items(items);

        let container_sources: Vec<Option<&str>> = prioritized
            .iter()
            .filter(|item| item.label == "container")
            .map(|item| item.source.as_deref())
            .collect();
        assert_eq!(container_sources, vec![Some("ClassMemberSnippet/")]);
        assert!(
            prioritized.iter().any(|item| item.label == "other"),
            "non-colliding entries should be preserved"
        );
    }
// TSZ_INLINE_TEST_END 36722a42ca62a4036584ebe812581c7c1bd55efba219e0b1b6ae245915404d99

// TSZ_INLINE_TEST_BEGIN 5babfc068d01b5808dad884cca4b91221ce23c23209b3ff6fc7a4ac97f80b1dc 1472 normalize_class_member_snippet_items_sets_snippet_flags_and_insert_text
    #[test]
    fn normalize_class_member_snippet_items_sets_snippet_flags_and_insert_text() {
        let items = vec![
            CompletionItem::new("container".to_string(), CompletionItemKind::Property)
                .with_detail("Container".to_string())
                .with_source("ClassMemberSnippet/".to_string()),
        ];

        let normalized = Server::normalize_class_member_snippet_items(items);
        let item = normalized
            .first()
            .expect("expected normalized class member snippet item");

        assert!(item.has_action);
        assert!(item.is_snippet);
        assert_eq!(item.insert_text.as_deref(), Some("container: Container;"));
    }
// TSZ_INLINE_TEST_END 5babfc068d01b5808dad884cca4b91221ce23c23209b3ff6fc7a4ac97f80b1dc

// TSZ_INLINE_TEST_BEGIN f2ea5155003aeca68d5adcf5053db44e1472c351cf16fa18db12881acaa7c5c5 1490 merge_class_member_snippet_candidates_prefers_fallback_when_primary_is_not_snippet_ready
    #[test]
    fn merge_class_member_snippet_candidates_prefers_fallback_when_primary_is_not_snippet_ready() {
        let provider = vec![
            CompletionItem::new(
                "execActionWithCount".to_string(),
                CompletionItemKind::Method,
            )
            .with_detail("(count: number): void".to_string()),
        ];
        let fallback = vec![
            CompletionItem::new(
                "execActionWithCount".to_string(),
                CompletionItemKind::Method,
            )
            .with_detail("(count: number) => void".to_string()),
        ];

        let merged = Server::merge_class_member_snippet_candidates(provider, fallback);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].detail.as_deref(),
            Some("(count: number) => void"),
            "fallback candidate should replace non-snippet-ready primary candidate"
        );
    }
// TSZ_INLINE_TEST_END f2ea5155003aeca68d5adcf5053db44e1472c351cf16fa18db12881acaa7c5c5

// TSZ_INLINE_TEST_BEGIN 8e0870a494797ed4cf2c79e975ba73bb901d3745aecfe94631a738633e9c7542 1516 synthesized_class_member_snippet_candidates_uses_auto_import_items_when_primary_empty
    #[test]
    fn synthesized_class_member_snippet_candidates_uses_auto_import_items_when_primary_empty() {
        let project_items = vec![
            CompletionItem::new(
                "execActionWithCount".to_string(),
                CompletionItemKind::Function,
            )
            .with_has_action()
            .with_source("@pkg/mod".to_string())
            .with_detail("(count: number) => void".to_string()),
            CompletionItem::new("Container".to_string(), CompletionItemKind::Class)
                .with_has_action()
                .with_source("@pkg/mod".to_string()),
        ];

        let synthesized =
            Server::synthesized_class_member_snippet_candidates_from_project_items(&project_items);
        assert_eq!(synthesized.len(), 1);
        assert_eq!(synthesized[0].label, "execActionWithCount");
        assert_eq!(synthesized[0].kind, CompletionItemKind::Method);
    }
// TSZ_INLINE_TEST_END 8e0870a494797ed4cf2c79e975ba73bb901d3745aecfe94631a738633e9c7542

// TSZ_INLINE_TEST_BEGIN 9a5e075e055432801ba4f06025cd45faaea7c0bdc3ed567103b791070c588306 1538 class_member_snippet_synthesized_text_changes_updates_existing_named_import
    #[test]
    fn class_member_snippet_synthesized_text_changes_updates_existing_named_import() {
        let source_text = "import { Piece } from \"@sapphire/pieces\";\nclass C extends Piece {}\n";
        let project_items = vec![
            CompletionItem::new("Container".to_string(), CompletionItemKind::Interface)
                .with_has_action()
                .with_source("@sapphire/pieces".to_string()),
        ];

        let changes = Server::class_member_snippet_synthesized_text_changes(
            source_text,
            "container: Container;",
            "container",
            &project_items,
        );

        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(
            change.get("newText").and_then(serde_json::Value::as_str),
            Some("import { Container, Piece } from \"@sapphire/pieces\";\n")
        );
    }
// TSZ_INLINE_TEST_END 9a5e075e055432801ba4f06025cd45faaea7c0bdc3ed567103b791070c588306

// TSZ_INLINE_TEST_BEGIN 9040f434cbd7db507496484715cb41dd24daf507f20a51c5f678e203e5a636c2 1562 class_member_snippet_synthesized_text_changes_inserts_after_import_block_for_side_effect_import
    #[test]
    fn class_member_snippet_synthesized_text_changes_inserts_after_import_block_for_side_effect_import()
     {
        let source_text = "import \"@sapphire/pieces\";\nimport { Command } from \"@sapphire/framework\";\nclass PingCommand extends Command {\n}\n";
        let project_items = vec![
            CompletionItem::new("Container".to_string(), CompletionItemKind::Interface)
                .with_has_action()
                .with_source("@sapphire/pieces".to_string()),
        ];

        let changes = Server::class_member_snippet_synthesized_text_changes(
            source_text,
            "get container(): Container {\n}",
            "container",
            &project_items,
        );

        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        let expected_start = source_text
            .find("class PingCommand")
            .expect("expected class declaration");
        assert_eq!(
            change
                .get("span")
                .and_then(|span| span.get("start"))
                .and_then(serde_json::Value::as_u64),
            Some(expected_start as u64)
        );
        assert_eq!(
            change.get("newText").and_then(serde_json::Value::as_str),
            Some("import { Container } from \"@sapphire/pieces\";\n")
        );
    }
// TSZ_INLINE_TEST_END 9040f434cbd7db507496484715cb41dd24daf507f20a51c5f678e203e5a636c2

// TSZ_INLINE_TEST_BEGIN 068d10d0b28da10841311ec4b29e353f04766e62e93dec91ad48c913b6eacaab 1597 class_member_snippet_additional_edits_rewrite_default_import_for_underscored_alias
    #[test]
    fn class_member_snippet_additional_edits_rewrite_default_import_for_underscored_alias() {
        let project_items = vec![
            CompletionItem::new("Document".to_string(), CompletionItemKind::Class)
                .with_has_action()
                .with_source("./document.js".to_string())
                .with_additional_edits(vec![tsz::lsp::rename::TextEdit::new(
                    tsz::lsp::position::Range::new(
                        tsz::lsp::position::Position::new(0, 0),
                        tsz::lsp::position::Position::new(0, 0),
                    ),
                    "import Document from \"./document.js\";\n".to_string(),
                )]),
        ];

        let edits = Server::class_member_snippet_additional_edits(
            "parent: Document_ | undefined;",
            "parent",
            &project_items,
        );
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0].new_text,
            "import Document_ from \"./document.js\";\n"
        );
    }
// TSZ_INLINE_TEST_END 068d10d0b28da10841311ec4b29e353f04766e62e93dec91ad48c913b6eacaab

// TSZ_INLINE_TEST_BEGIN 22cdee322c107e71745ec8fe9389c3b32aaafb4734c26106e3d88532fa6f975c 1624 fallback_class_member_scan_paths_include_external_project_file_paths
    #[test]

    fn fallback_class_member_scan_paths_include_external_project_file_paths() {
        let mut open_files = FxHashMap::default();
        open_files.insert("/src/current.ts".to_string(), "class C {}".to_string());
        let mut external_project_files = FxHashMap::default();
        external_project_files.insert(
            "project:/virtual".to_string(),
            vec!["/src/base.ts".to_string(), "/src/current.ts".to_string()],
        );

        let paths = Server::fallback_class_member_scan_paths(&open_files, &external_project_files);

        assert!(paths.iter().any(|path| path == "/src/current.ts"));
        assert!(paths.iter().any(|path| path == "/src/base.ts"));
        assert!(
            !paths.iter().any(|path| path == "project:/virtual"),
            "project names should not be treated as source file paths"
        );
    }
// TSZ_INLINE_TEST_END 22cdee322c107e71745ec8fe9389c3b32aaafb4734c26106e3d88532fa6f975c

// TSZ_INLINE_TEST_BEGIN ab466b3174b7ab6dfd5787c449c987f37646430d93a652b4d56a1362fed4ed0a 1645 resolve_imported_module_files_finds_relative_and_package_targets_from_open_files
    #[test]
    fn resolve_imported_module_files_finds_relative_and_package_targets_from_open_files() {
        let mut open_files = FxHashMap::default();
        open_files.insert(
            "/workspace/src/base.ts".to_string(),
            "export class Base {}".to_string(),
        );
        open_files.insert(
            "/workspace/node_modules/@scope/pkg/index.d.ts".to_string(),
            "export declare class Piece {}".to_string(),
        );
        let source = "import { Base } from \"./base\";\nimport { Piece } from \"@scope/pkg\";\n";

        let resolved =
            Server::resolve_imported_module_files("/workspace/src/current.ts", source, &open_files);

        assert!(
            resolved.iter().any(|path| path == "/workspace/src/base.ts"),
            "expected relative module candidate to resolve from open files: {resolved:?}"
        );
        assert!(
            resolved
                .iter()
                .any(|path| path == "/workspace/node_modules/@scope/pkg/index.d.ts"),
            "expected package module candidate to resolve from open files: {resolved:?}"
        );
    }
// TSZ_INLINE_TEST_END ab466b3174b7ab6dfd5787c449c987f37646430d93a652b4d56a1362fed4ed0a

// TSZ_INLINE_TEST_BEGIN e9c33010fc8cc68ad6d4c4abbfe8c783a6212659cdf2117bd9369ea089ac6edf 1673 resolve_imported_module_files_maps_js_specifier_to_ts_source
    #[test]
    fn resolve_imported_module_files_maps_js_specifier_to_ts_source() {
        let mut open_files = FxHashMap::default();
        open_files.insert(
            "/workspace/src/node.ts".to_string(),
            "export class Node {}".to_string(),
        );
        let source = "import Node from \"./node.js\";\n";

        let resolved = Server::resolve_imported_module_files(
            "/workspace/src/container.ts",
            source,
            &open_files,
        );

        assert!(
            resolved.iter().any(|path| path == "/workspace/src/node.ts"),
            "expected explicit .js import to resolve sibling TypeScript source: {resolved:?}"
        );
    }
// TSZ_INLINE_TEST_END e9c33010fc8cc68ad6d4c4abbfe8c783a6212659cdf2117bd9369ea089ac6edf
