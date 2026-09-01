//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_completions.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ba13a5333630abbcc630313e0374a9a9339e04cbe8ca838e38e643c574052f3f 1514 sort_tsserver_completion_items_prefers_direct_source_over_index_for_same_symbol
    #[test]
    fn sort_tsserver_completion_items_prefers_direct_source_over_index_for_same_symbol() {
        let mut items = vec![
            CompletionItem::new("Thing2A".to_string(), CompletionItemKind::Class)
                .with_source("./index".to_string()),
            CompletionItem::new("Thing2A".to_string(), CompletionItemKind::Class)
                .with_source("./thing2A".to_string()),
        ];

        Server::sort_tsserver_completion_items(&mut items);

        assert_eq!(items[0].source.as_deref(), Some("./thing2A"));
        assert_eq!(items[1].source.as_deref(), Some("./index"));
    }
// TSZ_INLINE_TEST_END ba13a5333630abbcc630313e0374a9a9339e04cbe8ca838e38e643c574052f3f

// TSZ_INLINE_TEST_BEGIN 7f12f60285ecc2349df3b741e161b702c7439b94bf30cbf5a57812478e56b3cb 1529 sort_tsserver_completion_items_prefers_bare_package_source_over_parent_relative
    #[test]
    fn sort_tsserver_completion_items_prefers_bare_package_source_over_parent_relative() {
        let mut items = vec![
            CompletionItem::new("MyClass".to_string(), CompletionItemKind::Class)
                .with_sort_text("16".to_string())
                .with_has_action()
                .with_source("../packages/mylib".to_string()),
            CompletionItem::new("MyClass".to_string(), CompletionItemKind::Class)
                .with_sort_text("16".to_string())
                .with_has_action()
                .with_source("mylib".to_string()),
        ];

        Server::sort_tsserver_completion_items(&mut items);

        let ordered_sources: Vec<Option<&str>> =
            items.iter().map(|item| item.source.as_deref()).collect();
        assert_eq!(
            ordered_sources,
            vec![Some("mylib"), Some("../packages/mylib")]
        );
    }
// TSZ_INLINE_TEST_END 7f12f60285ecc2349df3b741e161b702c7439b94bf30cbf5a57812478e56b3cb

// TSZ_INLINE_TEST_BEGIN 68dbda4bf4463c095b7237ee164776148b67dd582cdfa3d0f7bb82da1b216105 1552 sort_tsserver_completion_items_prefers_package_root_over_deep_package_subpath
    #[test]
    fn sort_tsserver_completion_items_prefers_package_root_over_deep_package_subpath() {
        let mut items = vec![
            CompletionItem::new("PatternValidator".to_string(), CompletionItemKind::Class)
                .with_sort_text("16".to_string())
                .with_has_action()
                .with_source("@angular/forms/forms".to_string()),
            CompletionItem::new("PatternValidator".to_string(), CompletionItemKind::Class)
                .with_sort_text("16".to_string())
                .with_has_action()
                .with_source("@angular/forms".to_string()),
        ];

        Server::sort_tsserver_completion_items(&mut items);

        let ordered_sources: Vec<Option<&str>> =
            items.iter().map(|item| item.source.as_deref()).collect();
        assert_eq!(
            ordered_sources,
            vec![Some("@angular/forms"), Some("@angular/forms/forms")]
        );
    }
// TSZ_INLINE_TEST_END 68dbda4bf4463c095b7237ee164776148b67dd582cdfa3d0f7bb82da1b216105

// TSZ_INLINE_TEST_BEGIN 12a95fdc29fb6f857c4f1527698e1c32fcc9ca792188c1d4a12d3d21cd80b63d 1575 sort_tsserver_completion_items_uses_numeric_aware_ui_order
    #[test]
    fn sort_tsserver_completion_items_uses_numeric_aware_ui_order() {
        let mut items = vec![
            CompletionItem::new("Int16Array".to_string(), CompletionItemKind::Variable)
                .with_sort_text("15".to_string()),
            CompletionItem::new("Int8Array".to_string(), CompletionItemKind::Variable)
                .with_sort_text("15".to_string()),
            CompletionItem::new("Int32Array".to_string(), CompletionItemKind::Variable)
                .with_sort_text("15".to_string()),
        ];

        Server::sort_tsserver_completion_items(&mut items);

        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, vec!["Int8Array", "Int16Array", "Int32Array"]);
    }
// TSZ_INLINE_TEST_END 12a95fdc29fb6f857c4f1527698e1c32fcc9ca792188c1d4a12d3d21cd80b63d

// TSZ_INLINE_TEST_BEGIN 8671040665751d52ec43575377d00ff6ac72e81613575e325504d81d6a5088a6 1592 sort_tsserver_completion_items_matches_ui_name_sort_across_kinds
    #[test]
    fn sort_tsserver_completion_items_matches_ui_name_sort_across_kinds() {
        let mut items = vec![
            CompletionItem::new("as".to_string(), CompletionItemKind::Keyword),
            CompletionItem::new("Array".to_string(), CompletionItemKind::Class),
        ];
        items[0].sort_text = Some("15".to_string());
        items[1].sort_text = Some("15".to_string());

        Server::sort_tsserver_completion_items(&mut items);

        assert_eq!(items[0].label, "Array");
        assert_eq!(items[1].label, "as");
    }
// TSZ_INLINE_TEST_END 8671040665751d52ec43575377d00ff6ac72e81613575e325504d81d6a5088a6

// TSZ_INLINE_TEST_BEGIN d9f4a9972c2a5c41a4ccb2d4b0adb1d78272778bbee7861ff9cd6cfda87a33e5 1607 prune_deeper_auto_import_duplicates_keeps_shallow_relative_source
    #[test]
    fn prune_deeper_auto_import_duplicates_keeps_shallow_relative_source() {
        let items = vec![
            CompletionItem::new("Button".to_string(), CompletionItemKind::Function)
                .with_has_action()
                .with_source("./lib/main".to_string()),
            CompletionItem::new("Button".to_string(), CompletionItemKind::Function)
                .with_has_action()
                .with_source("./lib/components/button/Button".to_string()),
            CompletionItem::new("foo".to_string(), CompletionItemKind::Function)
                .with_has_action()
                .with_source("./a".to_string()),
            CompletionItem::new("foo".to_string(), CompletionItemKind::Function)
                .with_has_action()
                .with_source("./b".to_string()),
        ];

        let pruned = Server::prune_deeper_auto_import_duplicates(items);
        let button_sources: Vec<&str> = pruned
            .iter()
            .filter(|item| item.label == "Button")
            .filter_map(|item| item.source.as_deref())
            .collect();
        let foo_sources: Vec<&str> = pruned
            .iter()
            .filter(|item| item.label == "foo")
            .filter_map(|item| item.source.as_deref())
            .collect();

        assert_eq!(button_sources, vec!["./lib/main"]);
        assert_eq!(foo_sources, vec!["./a", "./b"]);
    }
// TSZ_INLINE_TEST_END d9f4a9972c2a5c41a4ccb2d4b0adb1d78272778bbee7861ff9cd6cfda87a33e5

// TSZ_INLINE_TEST_BEGIN 3ee9f8bf7cd212da30b345d776220b692a264a53ed74e1a99e83c938e26b14cd 1640 normalize_mts_auto_import_edit_text_appends_existing_type_only_members
    #[test]

    fn normalize_mts_auto_import_edit_text_appends_existing_type_only_members() {
        let source_text = "import type { I } from \"./mod.js\";\n\nconst x: I = new ";
        let normalized = Server::normalize_mts_auto_import_edit_text(
            "/a.mts",
            CompletionItemKind::Class,
            source_text,
            "import { C } from \"./mod.js\";\n",
        );

        assert!(
            normalized.contains("import { C, type I } from \"./mod.js\";"),
            "expected normalize_mts_auto_import_edit_text to keep existing type-only imports, got: {normalized}"
        );
    }
// TSZ_INLINE_TEST_END 3ee9f8bf7cd212da30b345d776220b692a264a53ed74e1a99e83c938e26b14cd

// TSZ_INLINE_TEST_BEGIN 641c1bbcbd579d909babecf5877c915ce7ef8d04690f27d34484a8dc195814b0 1657 merged_class_member_context_detects_uppercase_receiver_before_dot
    #[test]
    fn merged_class_member_context_detects_uppercase_receiver_before_dot() {
        let source_text = "Foo.";
        let offset = source_text.len() as u32;
        assert!(Server::looks_like_merged_class_member_completion_context(
            source_text,
            offset
        ));

        let lower_source = "foo.";
        let lower_offset = lower_source.len() as u32;
        assert!(!Server::looks_like_merged_class_member_completion_context(
            lower_source,
            lower_offset
        ));
    }
// TSZ_INLINE_TEST_END 641c1bbcbd579d909babecf5877c915ce7ef8d04690f27d34484a8dc195814b0

// TSZ_INLINE_TEST_BEGIN 8a610f6cdee29d2192a7c65a2983fcadab7cad45ffeafa106ed7bdc898a48b02 1674 maybe_add_merged_class_function_members_populates_missing_function_surface
    #[test]
    fn maybe_add_merged_class_function_members_populates_missing_function_surface() {
        let items = vec![
            CompletionItem::new("prototype".to_string(), CompletionItemKind::Property),
            CompletionItem::new("x".to_string(), CompletionItemKind::Variable),
        ];

        let merged = Server::maybe_add_merged_class_function_members(items, "Foo.", 4, true);
        let labels: FxHashSet<&str> = merged.iter().map(|item| item.label.as_str()).collect();

        assert!(labels.contains("prototype"));
        assert!(labels.contains("x"));
        assert!(labels.contains("apply"));
        assert!(labels.contains("call"));
        assert!(labels.contains("bind"));
        assert!(labels.contains("arguments"));
        assert!(labels.contains("caller"));
    }
// TSZ_INLINE_TEST_END 8a610f6cdee29d2192a7c65a2983fcadab7cad45ffeafa106ed7bdc898a48b02

// TSZ_INLINE_TEST_BEGIN 5915fed0f74f211230644728f97dce581c24c7204bee15ddf97304861232a9af 1693 completion_sources_match_normalizes_extensions_index_and_node_prefix
    #[test]
    fn completion_sources_match_normalizes_extensions_index_and_node_prefix() {
        assert!(Server::completion_sources_match(
            Some("./local.ts"),
            "./local.js"
        ));
        assert!(Server::completion_sources_match(
            Some("./pkg/index.d.ts"),
            "./pkg"
        ));
        assert!(Server::completion_sources_match(Some("node:path"), "path"));
        assert!(Server::completion_sources_match(
            Some("./decl.d.mts"),
            "./decl.js"
        ));
        assert!(!Server::completion_sources_match(
            Some("./other"),
            "./local.js"
        ));
    }
// TSZ_INLINE_TEST_END 5915fed0f74f211230644728f97dce581c24c7204bee15ddf97304861232a9af

// TSZ_INLINE_TEST_BEGIN 6742f53131010a5eb5f8f33ea415bfb398732a8dbeba50c18a4d9e0f5873bb61 1722 should_include_completion_project_path_root_workspace_includes_sibling_files
    // Regression: when the active file sits at the filesystem root (e.g.
    // fourslash tests that name files `/main.ts`, `/Component.tsx`), the
    // computed `workspace_prefix` is "/" — every sibling file under "/"
    // must still be fed to the auto-import project. Previously the
    // filter produced the prefix "//" and dropped all sibling source
    // files (only node_modules survived), so Component.tsx / local.ts
    // never showed up in completion auto-imports and details requests
    // for them returned no codeActions.
    #[test]
    fn should_include_completion_project_path_root_workspace_includes_sibling_files() {
        // Root workspace: active file is /main.ts -> workspace_prefix = "/".
        assert_eq!(
            Server::path_workspace_prefix("/main.ts").as_deref(),
            Some("/")
        );

        // Sibling source files under "/" must be included.
        assert!(Server::should_include_completion_project_path(
            "/Component.tsx",
            "/main.ts",
            Some("/"),
            None,
        ));
        assert!(Server::should_include_completion_project_path(
            "/local.ts",
            "/main.ts",
            Some("/"),
            None,
        ));
        // Same file passes via the path == current_file early-return.
        assert!(Server::should_include_completion_project_path(
            "/main.ts",
            "/main.ts",
            Some("/"),
            None,
        ));
        // node_modules paths go through the allowed_packages gate and are
        // unaffected by the workspace_prefix fix: when no allowlist is
        // configured, node_modules paths are permitted.
        assert!(Server::should_include_completion_project_path(
            "/node_modules/bar/index.d.ts",
            "/main.ts",
            Some("/"),
            None,
        ));
    }
// TSZ_INLINE_TEST_END 6742f53131010a5eb5f8f33ea415bfb398732a8dbeba50c18a4d9e0f5873bb61

// TSZ_INLINE_TEST_BEGIN 680605c8a6649f169f81092e6261f088a48cea1f748a78c60a70abe8386b032d 1764 should_include_completion_project_path_non_root_workspace_respects_prefix
    // Non-root workspace prefix behavior (/project/...) is unchanged by the
    // root-workspace fix: siblings under the workspace are still included,
    // and files outside it are still excluded.
    #[test]
    fn should_include_completion_project_path_non_root_workspace_respects_prefix() {
        assert!(Server::should_include_completion_project_path(
            "/project/src/foo.ts",
            "/project/src/main.ts",
            Some("/project"),
            None,
        ));
        assert!(!Server::should_include_completion_project_path(
            "/other/foo.ts",
            "/project/src/main.ts",
            Some("/project"),
            None,
        ));
    }
// TSZ_INLINE_TEST_END 680605c8a6649f169f81092e6261f088a48cea1f748a78c60a70abe8386b032d
