//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.

mod message_tables;

pub use message_tables::DIAGNOSTIC_MESSAGE_SECTIONS;

pub fn iter_diagnostic_messages() -> impl Iterator<Item = crate::diagnostics::DiagnosticMessage> {
    DIAGNOSTIC_MESSAGE_SECTIONS
        .iter()
        .flat_map(|section| section.iter().copied())
}

/// Diagnostic message templates matching TypeScript exactly.
/// Use `format_message()` to fill in placeholders.
pub mod diagnostic_messages {
    include!("data/diagnostic_messages/part_000.rs");
    include!("data/diagnostic_messages/part_001.rs");
    include!("data/diagnostic_messages/part_002.rs");
    include!("data/diagnostic_messages/part_003.rs");
}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's `diagnosticMessages.json`.
pub mod diagnostic_codes {
    include!("data/diagnostic_codes/part_000.rs");
    include!("data/diagnostic_codes/part_001.rs");
    include!("data/diagnostic_codes/part_002.rs");
}
