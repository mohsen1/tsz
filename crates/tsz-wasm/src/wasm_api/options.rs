//! Compiler option DTO conversion helpers.
//!
//! JS callers pass numeric `target` and `module` values as `Option<u8>`
//! through `wasm_bindgen` boundaries. The conversion to canonical
//! `ScriptTarget` / `ModuleKind` enums is identical across `program.rs`
//! and `emit.rs`, so it lives here in one place.
//!
//! # Defaults
//!
//! The defaults match TypeScript's `tsc`:
//! - `target = None` → `ScriptTarget::ES5` (numeric `1`).
//! - `module = None` → `ModuleKind::None` (numeric `0`).
//!
//! Out-of-range numeric values (a JS caller could send anything) fall back
//! to the same defaults rather than panicking.

use tsz::emitter::{ModuleKind, ScriptTarget};

/// Convert a JS-supplied numeric `target` option to a canonical
/// [`ScriptTarget`].
///
/// `None` means "the option was not supplied"; the result is `ES5` to
/// match `tsc`'s default. Unrecognized numeric values also fall back to
/// `ES5` so a stale or malformed JS payload cannot panic the compiler.
#[must_use]
pub fn target_kind_from_u8(target: Option<u8>) -> ScriptTarget {
    let raw = target.unwrap_or(ScriptTarget::ES5 as u8);
    ScriptTarget::from_ts_numeric(u32::from(raw)).unwrap_or(ScriptTarget::ES5)
}

/// Convert a JS-supplied numeric `module` option to a canonical
/// [`ModuleKind`].
///
/// `None` means "the option was not supplied"; the result is
/// `ModuleKind::None` (numeric `0`) to match `tsc`'s default for
/// programs that have not configured a module system. Unrecognized
/// numeric values also fall back to `ModuleKind::None` so a stale or
/// malformed JS payload cannot panic the compiler.
#[must_use]
pub fn module_kind_from_u8(module: Option<u8>) -> ModuleKind {
    let raw = module.unwrap_or(ModuleKind::None as u8);
    ModuleKind::from_ts_numeric(u32::from(raw)).unwrap_or(ModuleKind::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_default_is_es5() {
        assert_eq!(target_kind_from_u8(None), ScriptTarget::ES5);
    }

    #[test]
    fn target_unknown_numeric_falls_back_to_es5() {
        // 250 is outside the documented numeric range.
        assert_eq!(target_kind_from_u8(Some(250)), ScriptTarget::ES5);
    }

    #[test]
    fn target_known_numeric_round_trips() {
        // 0 → ES3, 1 → ES5, 2 → ES2015 are the canonical low values.
        assert_eq!(target_kind_from_u8(Some(0)), ScriptTarget::ES3);
        assert_eq!(target_kind_from_u8(Some(1)), ScriptTarget::ES5);
        assert_eq!(target_kind_from_u8(Some(2)), ScriptTarget::ES2015);
    }

    #[test]
    fn module_default_is_none() {
        assert_eq!(module_kind_from_u8(None), ModuleKind::None);
    }

    #[test]
    fn module_unknown_numeric_falls_back_to_none() {
        assert_eq!(module_kind_from_u8(Some(250)), ModuleKind::None);
    }

    #[test]
    fn module_known_numeric_round_trips() {
        // 0 → None, 1 → CommonJS, 5 → ES2015 are tsc's canonical mappings.
        assert_eq!(module_kind_from_u8(Some(0)), ModuleKind::None);
        assert_eq!(module_kind_from_u8(Some(1)), ModuleKind::CommonJS);
        assert_eq!(module_kind_from_u8(Some(5)), ModuleKind::ES2015);
    }
}
