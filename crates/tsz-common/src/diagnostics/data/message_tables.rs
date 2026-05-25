//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/gen_diagnostics.mjs` to regenerate.
use crate::diagnostics::DiagnosticMessage;

#[path = "messages/part_000.rs"]
pub mod part_000;
#[path = "messages/part_001.rs"]
pub mod part_001;
#[path = "messages/part_002.rs"]
pub mod part_002;
#[path = "messages/part_003.rs"]
pub mod part_003;
#[path = "messages/part_004.rs"]
pub mod part_004;
#[path = "messages/part_005.rs"]
pub mod part_005;
#[path = "messages/part_006.rs"]
pub mod part_006;
#[path = "messages/part_007.rs"]
pub mod part_007;

pub static DIAGNOSTIC_MESSAGE_SECTIONS: &[&[DiagnosticMessage]] = &[
    part_000::MESSAGES,
    part_001::MESSAGES,
    part_002::MESSAGES,
    part_003::MESSAGES,
    part_004::MESSAGES,
    part_005::MESSAGES,
    part_006::MESSAGES,
    part_007::MESSAGES,
];
