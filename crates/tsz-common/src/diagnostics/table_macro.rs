//! Single-declaration expansion for the generated diagnostic table.
//!
//! Each generated part file declares every diagnostic exactly once as
//! `(NAME, code, Category, "message")`; this macro expands that single
//! declaration into the three views consumers use:
//!
//! - `codes::NAME: u32` — the numeric diagnostic code,
//! - `templates::NAME: &str` — the message template,
//! - `MESSAGES` — the `DiagnosticMessage` table slice for code lookup.
//!
//! Because all three views expand from the same token list, they cannot
//! drift apart the way separately generated tables historically did.

macro_rules! define_diagnostics {
    ($(($name:ident, $code:literal, $category:ident, $message:literal)),* $(,)?) => {
        /// `u32` diagnostic codes, one constant per diagnostic.
        pub mod codes {
            $(pub const $name: u32 = $code;)*
        }

        /// Message template strings, one constant per diagnostic. Placeholders
        /// (`{0}`, `{1}`, ...) are filled by `format_message()`.
        pub mod templates {
            $(pub const $name: &str = $message;)*
        }

        /// Table entries for this part, sorted by code.
        pub static MESSAGES: &[crate::diagnostics::DiagnosticMessage] = &[
            $(crate::diagnostics::DiagnosticMessage {
                code: $code,
                category: crate::diagnostics::DiagnosticCategory::$category,
                message: $message,
            },)*
        ];
    };
}

pub(crate) use define_diagnostics;
