//! Architecture gate for the central diagnostic emission sink.
//!
//! `context/diagnostic_push.rs` is the single sink every diagnostic flows
//! through. Display and suppression decisions belong to the type-level owner
//! (relation, inference, union ordering, tuple normalization) — never to
//! post-hoc surgery on formatted message text. This gate keeps the sink free
//! of message-string predicates and rewrites so the fixture-shaped hacks
//! removed by issue #13057 cannot silently return.

use std::fs;

fn sink_source() -> String {
    fs::read_to_string("src/context/diagnostic_push.rs")
        .expect("failed to read context/diagnostic_push.rs")
}

/// Strip line comments, doc comments, and string literals so the gate only
/// inspects executable code (messages quoted in comments stay allowed).
fn executable_code(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let line = line.split("//").next().unwrap_or(line);
        let mut in_string = false;
        for ch in line.chars() {
            if ch == '"' {
                in_string = !in_string;
                continue;
            }
            if !in_string {
                out.push(ch);
            }
        }
        out.push('\n');
    }
    out
}

/// The sink must not branch on the *content* of a rendered message: no
/// substring probes, prefix/suffix parsing, splitting, or in-place rewrites
/// of `message`/`message_text`. Hashing the message for dedup keys is the
/// only sanctioned read.
#[test]
fn emission_sink_does_not_inspect_or_rewrite_message_text() {
    let code = executable_code(&sink_source());
    // String-surgery methods with no sanctioned use anywhere in the sink.
    for forbidden in [
        ".strip_prefix(",
        ".strip_suffix(",
        ".split_once(",
        ".rsplit_once(",
        ".replace(",
        ".starts_with(",
        ".ends_with(",
    ] {
        assert!(
            !code.contains(forbidden),
            "diagnostic_push.rs must not use `{forbidden}` — message-text \
             predicates and rewrites in the emission sink mask upstream \
             display/relation defects; fix the type-level owner instead \
             (see issue #13057)",
        );
    }
    // `contains` is sanctioned on the dedup sets, but never on message text.
    for forbidden in [
        "message.contains(",
        "message_text.contains(",
        "message.find(",
        "message_text.find(",
    ] {
        assert!(
            !code.contains(forbidden),
            "diagnostic_push.rs must not probe message text via `{forbidden}` \
             (see issue #13057)",
        );
    }
}

/// The TS2301/TS2304/TS2552/TS2663 precedence rules must stay shared between
/// the two emission entry points instead of being copy-pasted back into them.
#[test]
fn name_resolution_precedence_is_single_sourced() {
    let source = sink_source();
    assert_eq!(
        source
            .matches("fn reconcile_name_resolution_precedence")
            .count(),
        1,
        "expected exactly one definition of the shared precedence reconciler"
    );
    assert_eq!(
        source
            .matches("self.reconcile_name_resolution_precedence(")
            .count(),
        2,
        "both `error` and `push_diagnostic` must route through the shared \
         precedence reconciler"
    );
}
