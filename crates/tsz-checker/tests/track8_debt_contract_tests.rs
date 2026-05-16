use std::fs;

const SOURCE_FILE_CHECKING: &str = "src/state/state_checking/source_file.rs";

const MAX_SOURCE_FILE_REWRITE_FINGERPRINT_HELPERS: usize = 9;
const MAX_SOURCE_FILE_SOURCE_TEXT_CONTAINS_DECISIONS: usize = 36;
const MAX_SOURCE_FILE_RENDERED_MESSAGE_DECISIONS: usize = 15;

#[test]
fn test_source_file_fingerprint_debt_does_not_increase() {
    let source = fs::read_to_string(SOURCE_FILE_CHECKING)
        .unwrap_or_else(|_| panic!("failed to read {SOURCE_FILE_CHECKING}"));

    let metrics = [
        (
            "rewrite fingerprint helpers",
            count_rewrite_fingerprint_helpers(&source),
            MAX_SOURCE_FILE_REWRITE_FINGERPRINT_HELPERS,
        ),
        (
            "source_text.contains decisions",
            source.matches("source_text.contains(").count(),
            MAX_SOURCE_FILE_SOURCE_TEXT_CONTAINS_DECISIONS,
        ),
        (
            "rendered diagnostic message decisions",
            count_rendered_message_decisions(&source),
            MAX_SOURCE_FILE_RENDERED_MESSAGE_DECISIONS,
        ),
    ];

    let violations = metrics
        .into_iter()
        .filter(|(_, actual, allowed)| actual > allowed)
        .map(|(name, actual, allowed)| format!("{name}: found {actual}, allowed {allowed}"))
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "Track 8 source-file diagnostic fingerprint debt must not increase:\n{}",
        violations.join("\n")
    );
}

fn count_rewrite_fingerprint_helpers(source: &str) -> usize {
    source
        .lines()
        .filter(|line| line.contains("fn rewrite_") && line.contains("_fingerprints"))
        .count()
}

fn count_rendered_message_decisions(source: &str) -> usize {
    source.matches("diag.message_text.contains(").count()
        + source.matches("diag.message_text.starts_with(").count()
        + source.matches("diag.message_text ==").count()
        + source.matches("existing.message_text ==").count()
        + source
            .matches("diagnostic.message_text.starts_with(")
            .count()
}
