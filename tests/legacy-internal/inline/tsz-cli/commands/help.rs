//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/commands/help.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bbedb1ce9f43fd62b8545cac98db716d18032b4c4e8c206f8549f8f5cf9c5d09 878 render_help_starts_with_header
    #[test]
    fn render_help_starts_with_header() {
        let output = render_help("6.0.0-dev.20260306");
        assert!(output.starts_with("tsc: The TypeScript Compiler - Version 6.0.0-dev.20260306\n"));
    }
// TSZ_INLINE_TEST_END bbedb1ce9f43fd62b8545cac98db716d18032b4c4e8c206f8549f8f5cf9c5d09

// TSZ_INLINE_TEST_BEGIN 520af5900625c4c1fb00870223f41e7aecbea6727f846767106c7f29c363e24f 884 render_help_ends_with_footer
    #[test]
    fn render_help_ends_with_footer() {
        let output = render_help("6.0.0-dev.20260306");
        assert!(
            output.ends_with(
                "You can learn about all of the compiler options at https://aka.ms/tsc\n"
            )
        );
    }
// TSZ_INLINE_TEST_END 520af5900625c4c1fb00870223f41e7aecbea6727f846767106c7f29c363e24f

// TSZ_INLINE_TEST_BEGIN 1a46da1eed8366f6a23b56dfede777f4a776d421a7714bca2a0bac19bfc6306e 894 render_help_all_starts_with_header
    #[test]
    fn render_help_all_starts_with_header() {
        let output = render_help_all("6.0.0-dev.20260306");
        assert!(output.starts_with("tsc: The TypeScript Compiler - Version 6.0.0-dev.20260306\n"));
    }
// TSZ_INLINE_TEST_END 1a46da1eed8366f6a23b56dfede777f4a776d421a7714bca2a0bac19bfc6306e

// TSZ_INLINE_TEST_BEGIN 9fa3bdeb3d431c83df2b65f76ab70f7b94c5c7c5e2b75b1abe88ff0457db81b3 900 render_help_all_has_watch_options
    #[test]
    fn render_help_all_has_watch_options() {
        let output = render_help_all("6.0.0-dev.20260306");
        assert!(output.contains("WATCH OPTIONS"));
    }
// TSZ_INLINE_TEST_END 9fa3bdeb3d431c83df2b65f76ab70f7b94c5c7c5e2b75b1abe88ff0457db81b3

// TSZ_INLINE_TEST_BEGIN a0998cd02741c22c3952aca8efe934f9b80f48d3cb4fc27eab8cf53c17d7c062 906 render_help_all_has_build_options
    #[test]
    fn render_help_all_has_build_options() {
        let output = render_help_all("6.0.0-dev.20260306");
        assert!(output.contains("BUILD OPTIONS"));
    }
// TSZ_INLINE_TEST_END a0998cd02741c22c3952aca8efe934f9b80f48d3cb4fc27eab8cf53c17d7c062

// TSZ_INLINE_TEST_BEGIN 242197f27306f0a461002dcb5ff0bbffdc670d6672960aed4bf0238c4ed6bf20 912 colorize_help_adds_bold_to_headers
    #[test]
    fn colorize_help_adds_bold_to_headers() {
        let plain = render_help("6.0.0-dev.20260306");
        colored::control::set_override(true);
        let colorized = colorize_help(&plain);
        assert!(colorized.contains("\x1b[1mCOMMON COMMANDS\x1b[22m"));
        assert!(colorized.contains("\x1b[94m--help, -h\x1b[39m"));
        colored::control::unset_override();
    }
// TSZ_INLINE_TEST_END 242197f27306f0a461002dcb5ff0bbffdc670d6672960aed4bf0238c4ed6bf20

// TSZ_INLINE_TEST_BEGIN f3e71f79c0678f7ae5dea1f43579d1127236e37aeae4fdebd6cb8f2d1a22b2db 922 colorize_help_colors_tsc_examples_and_preserves_missing_trailing_newline
    #[test]
    fn colorize_help_colors_tsc_examples_and_preserves_missing_trailing_newline() {
        colored::control::set_override(true);
        let plain = "COMMON COMMANDS\n\n  tsc app.ts\nbody";
        let colorized = colorize_help(plain);

        assert!(!colorized.ends_with('\n'));
        assert!(colorized.contains("  \x1b[94mtsc app.ts\x1b[39m"));
        assert!(colorized.contains("body"));
        colored::control::unset_override();
    }
// TSZ_INLINE_TEST_END f3e71f79c0678f7ae5dea1f43579d1127236e37aeae4fdebd6cb8f2d1a22b2db

// TSZ_INLINE_TEST_BEGIN 773c3d6b17220258063ebd051a18544fca76f8a2be322b16456a5532c29ed48f 934 colorize_help_does_not_color_triple_dash_or_mixed_case_lines_as_headers
    #[test]
    fn colorize_help_does_not_color_triple_dash_or_mixed_case_lines_as_headers() {
        colored::control::set_override(true);
        let plain = "---\nMixed Case Header\n--flag\n";
        let colorized = colorize_help(plain);

        assert!(colorized.contains("---\n"));
        assert!(colorized.contains("Mixed Case Header\n"));
        assert!(colorized.contains("\x1b[94m--flag\x1b[39m"));
        assert!(!colorized.contains("\x1b[1mMixed Case Header\x1b[22m"));
        colored::control::unset_override();
    }
// TSZ_INLINE_TEST_END 773c3d6b17220258063ebd051a18544fca76f8a2be322b16456a5532c29ed48f
