//! Checker-only slice of TypeScript's non-strict-family option fan-out.
//!
//! `tsz_core::config::option_fanout` owns the full non-strict fan-out, but it
//! operates on `ResolvedCompilerOptions` (which embeds the emit-only
//! `PrinterOptions`) and therefore lives in `tsz-core`. The two derivations
//! that are purely checker-semantic — `verbatimModuleSyntax -> isolatedModules`
//! and `esModuleInterop -> allowSyntheticDefaultImports` — also have to fire on
//! the bare [`CheckerOptions`] that the WASM and `tsz_server` lanes build
//! (no printer, no emit). This module is their single owner so those lanes
//! stop hand-rolling the implication.
//!
//! `tsz_core::config::option_fanout` delegates the checker half of its work
//! here, so the CLI, tsconfig, and server lanes all derive these two members
//! from exactly one place.
//!
//! Each rule is pinned to its tsc 6.0.3 `computedOptions` entry
//! (`TypeScript/src/compiler/utilities.ts`).

use super::checker::CheckerOptions;

/// Apply the checker-semantic non-strict-family implications to `options`.
///
/// Idempotent: every rule is a monotonic set toward `true`, mirroring tsc's
/// `computedOptions` (`value || derivedFrom`).
pub const fn apply_checker_fanout(options: &mut CheckerOptions) {
    // `verbatimModuleSyntax` implies `isolatedModules`
    // (tsc `computedOptions.isolatedModules`:
    // `isolatedModules || verbatimModuleSyntax`).
    if options.verbatim_module_syntax {
        options.isolated_modules = true;
    }
    // `esModuleInterop` implies `allowSyntheticDefaultImports` (tsz's
    // historical coupling; tsc <= 5.x derived `allowSyntheticDefaultImports`
    // from `esModuleInterop || module === system || moduleResolution ===
    // bundler`). tsc 6.0.3 decoupled them
    // (`computedOptions.allowSyntheticDefaultImports` is `dependencies: []`,
    // defaulting to `true`); flipping tsz's default is a checker-semantic
    // behavior change tracked separately, so this preserves current behavior.
    if options.es_module_interop {
        options.allow_synthetic_default_imports = true;
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckerOptions, apply_checker_fanout};

    #[test]
    fn verbatim_module_syntax_implies_isolated_modules() {
        let mut options = CheckerOptions {
            verbatim_module_syntax: true,
            isolated_modules: false,
            ..Default::default()
        };
        apply_checker_fanout(&mut options);
        assert!(options.isolated_modules);
    }

    #[test]
    fn es_module_interop_implies_synthetic_default_imports() {
        let mut options = CheckerOptions {
            es_module_interop: true,
            allow_synthetic_default_imports: false,
            ..Default::default()
        };
        apply_checker_fanout(&mut options);
        assert!(options.allow_synthetic_default_imports);
    }

    #[test]
    fn no_implication_when_sources_unset() {
        let mut options = CheckerOptions {
            verbatim_module_syntax: false,
            es_module_interop: false,
            isolated_modules: false,
            allow_synthetic_default_imports: false,
            ..Default::default()
        };
        apply_checker_fanout(&mut options);
        assert!(!options.isolated_modules);
        assert!(!options.allow_synthetic_default_imports);
    }
}
