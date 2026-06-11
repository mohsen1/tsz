//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.

#[path = "data/parts/part_000.rs"]
mod part_000;
#[path = "data/parts/part_001.rs"]
mod part_001;
#[path = "data/parts/part_002.rs"]
mod part_002;
#[path = "data/parts/part_003.rs"]
mod part_003;

pub static DIAGNOSTIC_MESSAGE_SECTIONS: &[&[crate::diagnostics::DiagnosticMessage]] = &[
    part_000::MESSAGES,
    part_001::MESSAGES,
    part_002::MESSAGES,
    part_003::MESSAGES,
];

pub fn iter_diagnostic_messages() -> impl Iterator<Item = crate::diagnostics::DiagnosticMessage> {
    DIAGNOSTIC_MESSAGE_SECTIONS
        .iter()
        .flat_map(|section| section.iter().copied())
}

/// Diagnostic message templates matching TypeScript exactly.
/// Use `format_message()` to fill in placeholders.
pub mod diagnostic_messages {
    pub use super::part_000::templates::*;
    pub use super::part_001::templates::*;
    pub use super::part_002::templates::*;
    pub use super::part_003::templates::*;
}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's `diagnosticMessages.json`.
pub mod diagnostic_codes {
    pub use super::part_000::codes::*;
    pub use super::part_001::codes::*;
    pub use super::part_002::codes::*;
    pub use super::part_003::codes::*;
}
