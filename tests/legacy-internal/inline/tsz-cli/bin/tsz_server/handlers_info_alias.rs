//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_info_alias.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 791c2daa25b43aa16007ee3c0f83a9766d5f577d0789e01b7613c28fab2d269e 1813 native_worker_prefers_the_built_pinned_harness_runtime
    #[test]
    fn native_worker_prefers_the_built_pinned_harness_runtime() {
        let script = include_str!("native_ts_worker.js");
        let built_runtime = script
            .find("built\", \"local\", \"harness\", \"_namespaces\", \"ts.js")
            .expect("worker should probe the built pinned harness runtime");
        let bootstrap_runtime = script
            .find("TypeScript\", \"node_modules\", \"typescript")
            .expect("worker should retain the bootstrap runtime as a fallback");
        assert!(
            built_runtime < bootstrap_runtime,
            "the corpus bootstrap compiler must not shadow the pinned built language service"
        );
    }
// TSZ_INLINE_TEST_END 791c2daa25b43aa16007ee3c0f83a9766d5f577d0789e01b7613c28fab2d269e

// TSZ_INLINE_TEST_BEGIN 7068ed56336ab9b896540fbaf27a85daf278619e3471be78ac28c44e4fc3467e 1828 import_statement_context_span_accepts_export_specifier_lines
    #[test]
    fn import_statement_context_span_accepts_export_specifier_lines() {
        let source = "const foo = 1;\nexport { foo as \"__<alias>\" };\n";
        let anchor = source
            .find("__<alias>")
            .expect("expected alias literal in source") as u32;
        let span = Server::import_statement_context_span(source, anchor)
            .expect("expected context span for export specifier line");
        let line = &source[span.0 as usize..span.1 as usize];
        assert!(
            line.trim_start().starts_with("export "),
            "expected export statement context, got: {line:?}"
        );
    }
// TSZ_INLINE_TEST_END 7068ed56336ab9b896540fbaf27a85daf278619e3471be78ac28c44e4fc3467e
