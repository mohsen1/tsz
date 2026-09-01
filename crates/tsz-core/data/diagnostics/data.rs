//! Auto-generated diagnostic message data.
//!
//! DO NOT EDIT MANUALLY - run `node scripts/setup/sync-typescript-diagnostics.mjs --write` to regenerate.

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

    // Pre-overlay identifiers retained for source compatibility.
    #[doc(hidden)]
    pub use super::part_001::templates::LOCALE_MUST_BE_AN_IETF_BCP_47_LANGUAGE_TAG_EXAMPLES as LOCALE_MUST_BE_OF_THE_FORM_LANGUAGE_OR_LANGUAGE_TERRITORY_FOR_EXAMPLE_OR;
    #[doc(hidden)]
    pub use super::part_001::templates::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_DID_YOU_FORGET_A_LEADING as NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD;
    #[doc(hidden)]
    pub use super::part_001::templates::OPTION_INCREMENTAL_IS_ONLY_VALID_WITH_A_KNOWN_CONFIGURATION_FILE_LIKE_TSCONFIG_J as OPTION_INCREMENTAL_CAN_ONLY_BE_SPECIFIED_USING_TSCONFIG_EMITTING_TO_SINGLE_FILE;
    #[doc(hidden)]
    pub use super::part_002::templates::A_JSDOC_TYPE_TAG_ON_A_FUNCTION_MUST_HAVE_A_SIGNATURE_WITH_THE_CORRECT_NUMBER_OF as THE_TYPE_OF_A_FUNCTION_DECLARATION_MUST_MATCH_THE_FUNCTIONS_SIGNATURE;
    #[doc(hidden)]
    pub use super::part_002::templates::FAILED_TO_DELETE_FILE as PROJECT_IS_OUT_OF_DATE_BECAUSE_ITS_DEPENDENCY_IS_OUT_OF_DATE;
    #[doc(hidden)]
    pub use super::part_002::templates::PROJECT_IS_OUT_OF_DATE_BECAUSE_CONFIG_FILE_DOES_NOT_EXIST as PROJECT_IS_OUT_OF_DATE_BECAUSE_THERE_WAS_ERROR_READING_FILE;
    #[doc(hidden)]
    pub use super::part_002::templates::PROJECT_IS_OUT_OF_DATE_BECAUSE_INPUT_DOES_NOT_EXIST as PROJECT_IS_OUT_OF_DATE_BECAUSE;
}

/// TypeScript diagnostic error codes.
/// Matches codes from TypeScript's merged diagnostic catalogs.
pub mod diagnostic_codes {
    pub use super::part_000::codes::*;
    pub use super::part_001::codes::*;
    pub use super::part_002::codes::*;
    pub use super::part_003::codes::*;

    // Pre-overlay identifiers retained for source compatibility.
    #[doc(hidden)]
    pub use super::part_001::codes::LOCALE_MUST_BE_AN_IETF_BCP_47_LANGUAGE_TAG_EXAMPLES as LOCALE_MUST_BE_OF_THE_FORM_LANGUAGE_OR_LANGUAGE_TERRITORY_FOR_EXAMPLE_OR;
    #[doc(hidden)]
    pub use super::part_001::codes::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_DID_YOU_FORGET_A_LEADING as NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_WHEN_BASEURL_IS_NOT_SET_DID_YOU_FORGET_A_LEAD;
    #[doc(hidden)]
    pub use super::part_001::codes::OPTION_INCREMENTAL_IS_ONLY_VALID_WITH_A_KNOWN_CONFIGURATION_FILE_LIKE_TSCONFIG_J as OPTION_INCREMENTAL_CAN_ONLY_BE_SPECIFIED_USING_TSCONFIG_EMITTING_TO_SINGLE_FILE;
    #[doc(hidden)]
    pub use super::part_002::codes::A_JSDOC_TYPE_TAG_ON_A_FUNCTION_MUST_HAVE_A_SIGNATURE_WITH_THE_CORRECT_NUMBER_OF as THE_TYPE_OF_A_FUNCTION_DECLARATION_MUST_MATCH_THE_FUNCTIONS_SIGNATURE;
    #[doc(hidden)]
    pub use super::part_002::codes::FAILED_TO_DELETE_FILE as PROJECT_IS_OUT_OF_DATE_BECAUSE_ITS_DEPENDENCY_IS_OUT_OF_DATE;
    #[doc(hidden)]
    pub use super::part_002::codes::PROJECT_IS_OUT_OF_DATE_BECAUSE_CONFIG_FILE_DOES_NOT_EXIST as PROJECT_IS_OUT_OF_DATE_BECAUSE_THERE_WAS_ERROR_READING_FILE;
    #[doc(hidden)]
    pub use super::part_002::codes::PROJECT_IS_OUT_OF_DATE_BECAUSE_INPUT_DOES_NOT_EXIST as PROJECT_IS_OUT_OF_DATE_BECAUSE;
}
