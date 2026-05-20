use super::{
    InlineLink, LinkStyle, LinkUriResolver, expand_links_to_markdown, expand_links_to_plain,
    iter_inline_links,
};

/// Resolver that succeeds for any allow-listed target and returns a fixed URI.
struct StubResolver<'a> {
    allow: &'a [&'a str],
    uri: &'a str,
}

impl LinkUriResolver for StubResolver<'_> {
    fn resolve_link_uri(&mut self, target: &str) -> Option<String> {
        if self.allow.iter().any(|name| *name == target) {
            Some(self.uri.to_string())
        } else {
            None
        }
    }
}

fn links(text: &str) -> Vec<InlineLink<'_>> {
    iter_inline_links(text)
}

#[test]
fn iter_finds_bare_link_tag() {
    let text = "see {@link Foo}";
    let parsed = links(text);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].target, "Foo");
    assert_eq!(parsed[0].display, None);
    assert_eq!(parsed[0].style, LinkStyle::Plain);
    assert_eq!(&text[parsed[0].span.clone()], "{@link Foo}");
}

#[test]
fn iter_finds_link_with_display_after_whitespace() {
    let text = "use {@link Foo bar baz}";
    let parsed = links(text);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].target, "Foo");
    assert_eq!(parsed[0].display, Some("bar baz"));
}

#[test]
fn iter_finds_link_with_display_after_pipe() {
    let text = "use {@link Foo|bar baz}";
    let parsed = links(text);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].target, "Foo");
    assert_eq!(parsed[0].display, Some("bar baz"));
}

#[test]
fn iter_recognizes_linkcode_and_linkplain() {
    let text = "{@linkcode A} and {@linkplain B} together";
    let parsed = links(text);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].target, "A");
    assert_eq!(parsed[0].style, LinkStyle::Code);
    assert_eq!(parsed[1].target, "B");
    assert_eq!(parsed[1].style, LinkStyle::Plain);
}

#[test]
fn iter_does_not_match_unknown_tag() {
    // {@linkable …} is not a link variant; must be ignored.
    let text = "{@linkable Foo} {@param x} not links";
    assert!(links(text).is_empty());
}

#[test]
fn iter_handles_back_to_back_links() {
    let text = "{@link A}{@link B}";
    let parsed = links(text);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].target, "A");
    assert_eq!(parsed[1].target, "B");
}

#[test]
fn iter_ignores_unclosed_link_token() {
    // No closing brace anywhere — must not panic, must not produce a link.
    let text = "{@link Foo missing brace";
    assert!(links(text).is_empty());
}

#[test]
fn iter_skips_empty_target() {
    let text = "{@link } {@link  } {@link \t}";
    assert!(links(text).is_empty());
}

#[test]
fn iter_target_name_is_not_hardcoded() {
    // §25 anti-hardcoding gate: any user-chosen identifier must parse.
    for name in ["X", "FooBar", "f_oo$1", "Promise", "lib.es5"] {
        let text = format!("{{@link {name}}}");
        let parsed = links(&text);
        assert_eq!(parsed.len(), 1, "{name} should parse");
        assert_eq!(parsed[0].target, name);
    }
}

#[test]
fn markdown_renders_resolved_link_as_markdown_anchor() {
    let mut resolver = StubResolver {
        allow: &["Foo"],
        uri: "file:///path/test.ts#L1,1",
    };
    let out = expand_links_to_markdown("see {@link Foo}", &mut resolver);
    assert_eq!(out, "see [Foo](file:///path/test.ts#L1,1)");
}

#[test]
fn markdown_renders_linkcode_with_backticks() {
    let mut resolver = StubResolver {
        allow: &["Foo"],
        uri: "file:///x.ts#L1,1",
    };
    let out = expand_links_to_markdown("{@linkcode Foo}", &mut resolver);
    assert_eq!(out, "[`Foo`](file:///x.ts#L1,1)");
}

#[test]
fn markdown_uses_display_label_when_provided() {
    let mut resolver = StubResolver {
        allow: &["Foo"],
        uri: "file:///x.ts#L1,1",
    };
    let out = expand_links_to_markdown("{@link Foo|alias}", &mut resolver);
    assert_eq!(out, "[alias](file:///x.ts#L1,1)");
}

#[test]
fn markdown_falls_back_to_plain_text_when_unresolved() {
    let mut resolver = StubResolver {
        allow: &[],
        uri: "",
    };
    // No link target resolves; the output must contain the human-readable
    // label and no Markdown link syntax (no crash on broken @link).
    let out = expand_links_to_markdown("see {@link Missing}", &mut resolver);
    assert_eq!(out, "see Missing");
}

#[test]
fn markdown_falls_back_keeps_code_voice_for_linkcode() {
    let mut resolver = StubResolver {
        allow: &[],
        uri: "",
    };
    let out = expand_links_to_markdown("see {@linkcode Missing}", &mut resolver);
    assert_eq!(out, "see `Missing`");
}

#[test]
fn plain_drops_uri_and_keeps_label() {
    let out = expand_links_to_plain("see {@link Foo|alias} and {@linkcode Bar}");
    assert_eq!(out, "see alias and Bar");
}

#[test]
fn rewrite_preserves_surrounding_text_and_multiple_links() {
    let mut resolver = StubResolver {
        allow: &["A", "B"],
        uri: "uri",
    };
    let out = expand_links_to_markdown("pre {@link A} mid {@link B baz} end", &mut resolver);
    assert_eq!(out, "pre [A](uri) mid [baz](uri) end");
}

#[test]
fn rewrite_is_name_independent() {
    // Renaming the bound symbol must not change the rewrite behavior.
    let mut resolver = StubResolver {
        allow: &["Alpha", "Beta"],
        uri: "uri",
    };
    let cases = [
        ("see {@link Alpha}", "see [Alpha](uri)"),
        ("see {@link Beta}", "see [Beta](uri)"),
    ];
    for (input, expected) in cases {
        assert_eq!(expand_links_to_markdown(input, &mut resolver), expected);
    }
}
