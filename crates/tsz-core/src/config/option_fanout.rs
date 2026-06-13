//! Single owner of TypeScript's non-strict-family compiler-option fan-out.
//!
//! tsc keeps every implied option in one `computedOptions` table
//! (`TypeScript/src/compiler/utilities.ts`): an option whose value is not
//! provided is derived from other already-resolved options. tsz previously
//! re-encoded the same implications field-by-field in the CLI driver
//! (`driver/plan.rs`) and the tsconfig resolver (`resolved_options.rs`),
//! which silently drifted (the tsconfig lane never mirrored the
//! `isolatedModules`/`verbatimModuleSyntax` const-enum implication to the
//! printer, so tsconfig-driven emit erased const enums that the CLI lane
//! preserved).
//!
//! This module is the single declarative home for those derivations. Both
//! emit-capable engines call [`apply_non_strict_fanout`] on a
//! [`ResolvedCompilerOptions`] (which embeds both `checker` and `printer`)
//! after the per-flag overrides have been applied, so the implications can
//! never diverge per surface again.
//!
//! The strict-family umbrella is owned separately by
//! `tsz_common::options::strict_family`; this module covers everything else.
//!
//! Each implication is pinned to its tsc 6.0.3 `computedOptions` entry. The
//! WASM/server lanes build a bare `CheckerOptions` with no printer and no
//! emit, so they cannot consume the printer-aware [`apply_non_strict_fanout`]
//! directly; instead they call the checker-only slice
//! [`tsz_common::options::checker_fanout::apply_checker_fanout`] (which this
//! table also delegates to). That keeps the two purely checker-semantic
//! derivations (`verbatimModuleSyntax -> isolatedModules`,
//! `esModuleInterop -> allowSyntheticDefaultImports`) under one owner across
//! every engine.

use super::ResolvedCompilerOptions;

/// Apply tsc's non-strict-family option implications to `resolved`.
///
/// Must run after the raw per-option overrides (so the source flags hold
/// their resolved values) and after [`strict_family`](super::strict_family)
/// (the strict umbrella is independent of these implications). Idempotent:
/// every rule is a monotonic `||`-style set toward `true`, matching tsc's
/// `computedOptions` (`value || derivedFrom`), so a second call is a no-op.
pub const fn apply_non_strict_fanout(resolved: &mut ResolvedCompilerOptions) {
    // Checker-semantic implications (`verbatimModuleSyntax -> isolatedModules`,
    // `esModuleInterop -> allowSyntheticDefaultImports`) are owned by the
    // shared `tsz_common` helper so the WASM/server lanes share them. It must
    // run first so the printer-mirroring below sees the derived
    // `isolated_modules` value.
    tsz_common::options::checker_fanout::apply_checker_fanout(&mut resolved.checker);
    apply_composite_implications(resolved);
    apply_isolated_modules_const_enum(resolved);
    apply_import_helpers(resolved);
    apply_es_module_interop_synthetic_defaults(resolved);
}

/// `composite` implies `declaration` and `incremental`.
///
/// tsc 6.0.3 `computedOptions.declaration`
/// (`utilities.ts`: `declaration || composite`) and
/// `computedOptions.incremental` (`incremental || composite`).
const fn apply_composite_implications(resolved: &mut ResolvedCompilerOptions) {
    if resolved.composite {
        resolved.emit_declarations = true;
        resolved.checker.emit_declarations = true;
        resolved.incremental = true;
    }
}

/// `isolatedModules || verbatimModuleSyntax` implies `preserveConstEnums`
/// (so const enums are emitted as real enums rather than erased + inlined),
/// and `verbatimModuleSyntax` additionally implies `isolatedModules`.
///
/// tsc 6.0.3 `computedOptions.isolatedModules`
/// (`utilities.ts`: `isolatedModules || verbatimModuleSyntax`) and
/// `computedOptions.preserveConstEnums`
/// (`preserveConstEnums || isolatedModules(computed)`), i.e.
/// `shouldPreserveConstEnums` =
/// `preserveConstEnums || isolatedModules || verbatimModuleSyntax`.
///
/// `no_const_enum_inlining` is the tsz-internal printer companion of
/// `preserve_const_enums` (it disables inlining at const-enum usage sites);
/// it tracks the same condition.
///
/// Parity note: before this table the tsconfig resolver set only
/// `checker.isolated_modules`/`checker.verbatim_module_syntax` and never
/// mirrored the const-enum implication to the printer, so a `tsconfig.json`
/// with `isolatedModules: true` erased const enums that the CLI lane (and
/// tsc) preserved. Routing both engines through this rule fixes that
/// tsconfig-only emit divergence toward tsc.
const fn apply_isolated_modules_const_enum(resolved: &mut ResolvedCompilerOptions) {
    let verbatim = resolved.checker.verbatim_module_syntax;
    // `verbatimModuleSyntax -> isolatedModules` is the checker-semantic half,
    // owned by the shared `apply_checker_fanout` helper so the WASM/server
    // lanes derive it identically; here it has already run via
    // `apply_non_strict_fanout` before this printer-mirroring step.
    if verbatim {
        resolved.printer.verbatim_module_syntax = true;
    }
    if resolved.checker.isolated_modules || verbatim {
        resolved.printer.preserve_const_enums = true;
        resolved.printer.no_const_enum_inlining = true;
    }
}

/// `importHelpers` suppresses inline helper emission (helpers are imported
/// from `tslib` instead).
///
/// tsc emits `import ... from "tslib"` and stops inlining the `__` helpers
/// when `importHelpers` is set (`transformers/utilities.ts` emit-helper
/// scheduling). `no_emit_helpers` is the tsz printer flag that gates inline
/// helper text; `import_helpers` is mirrored to the printer so the emitter
/// knows to route through `tslib`.
const fn apply_import_helpers(resolved: &mut ResolvedCompilerOptions) {
    if resolved.import_helpers {
        resolved.printer.import_helpers = true;
        resolved.printer.no_emit_helpers = true;
    }
}

/// `esModuleInterop` implies `allowSyntheticDefaultImports`.
///
/// This is tsz's historical coupling (tsc <= 5.x derived
/// `allowSyntheticDefaultImports` from `esModuleInterop || module === system
/// || moduleResolution === bundler`). tsc 6.0.3 decoupled them:
/// `computedOptions.allowSyntheticDefaultImports` is now `dependencies: []`
/// and resolves to `allowSyntheticDefaultImports !== undefined ? value :
/// true` (`utilities.ts:9126`), independent of `esModuleInterop`. Flipping
/// tsz to the unconditional `true` default is a checker-semantic behavior
/// change (it gates TS1192/TS1259/TS2497 default-import diagnostics), so it
/// is tracked separately and NOT made here; this rule preserves tsz's
/// current behavior byte-for-byte while still owning it in one place. The
/// `esModuleInterop` default-to-`true` and the `module === system` /
/// `moduleResolution === bundler` fallbacks stay at the engine call sites
/// because they depend on TS5024 invalidation / module-resolution state that
/// is engine-local.
const fn apply_es_module_interop_synthetic_defaults(resolved: &mut ResolvedCompilerOptions) {
    // `checker.allow_synthetic_default_imports` is set by the shared
    // `apply_checker_fanout` helper above; mirror the derived value to the
    // top-level `ResolvedCompilerOptions` field consumed by the resolver/CLI.
    if resolved.es_module_interop {
        resolved.allow_synthetic_default_imports = true;
    }
}

#[cfg(test)]
mod tests {
    use super::{ResolvedCompilerOptions, apply_non_strict_fanout};

    #[test]
    fn composite_implies_declaration_and_incremental() {
        let mut resolved = ResolvedCompilerOptions {
            composite: true,
            ..Default::default()
        };
        apply_non_strict_fanout(&mut resolved);

        assert!(resolved.emit_declarations);
        assert!(resolved.checker.emit_declarations);
        assert!(resolved.incremental);
    }

    #[test]
    fn isolated_modules_preserves_const_enums_on_printer() {
        let mut resolved = ResolvedCompilerOptions::default();
        resolved.checker.isolated_modules = true;
        apply_non_strict_fanout(&mut resolved);

        assert!(
            resolved.printer.preserve_const_enums,
            "isolatedModules must preserve const enums in emit (tsc \
             shouldPreserveConstEnums)"
        );
        assert!(resolved.printer.no_const_enum_inlining);
    }

    #[test]
    fn verbatim_module_syntax_implies_isolated_modules_and_const_enums() {
        let mut resolved = ResolvedCompilerOptions::default();
        resolved.checker.verbatim_module_syntax = true;
        apply_non_strict_fanout(&mut resolved);

        assert!(
            resolved.checker.isolated_modules,
            "verbatimModuleSyntax implies isolatedModules (tsc computedOptions)"
        );
        assert!(resolved.printer.verbatim_module_syntax);
        assert!(resolved.printer.preserve_const_enums);
        assert!(resolved.printer.no_const_enum_inlining);
    }

    #[test]
    fn no_const_enum_implication_when_neither_set() {
        let mut resolved = ResolvedCompilerOptions::default();
        apply_non_strict_fanout(&mut resolved);

        assert!(!resolved.printer.preserve_const_enums);
        assert!(!resolved.printer.no_const_enum_inlining);
        assert!(!resolved.checker.isolated_modules);
    }

    #[test]
    fn import_helpers_suppresses_inline_helpers() {
        let mut resolved = ResolvedCompilerOptions {
            import_helpers: true,
            ..Default::default()
        };
        apply_non_strict_fanout(&mut resolved);

        assert!(resolved.printer.import_helpers);
        assert!(resolved.printer.no_emit_helpers);
    }

    #[test]
    fn es_module_interop_implies_synthetic_default_imports() {
        // Both engines (CLI/tsconfig) populate the top-level and the
        // `checker` copies of `esModuleInterop` together, so mirror that here:
        // the checker-semantic half of the implication is owned by
        // `apply_checker_fanout` (driven by `checker.es_module_interop`) and
        // the top-level mirror by `apply_es_module_interop_synthetic_defaults`.
        let mut resolved = ResolvedCompilerOptions {
            es_module_interop: true,
            ..Default::default()
        };
        resolved.checker.es_module_interop = true;
        apply_non_strict_fanout(&mut resolved);

        assert!(resolved.allow_synthetic_default_imports);
        assert!(resolved.checker.allow_synthetic_default_imports);
    }

    #[test]
    fn no_implications_fire_for_default_options() {
        let mut resolved = ResolvedCompilerOptions::default();
        apply_non_strict_fanout(&mut resolved);

        assert!(!resolved.emit_declarations);
        assert!(!resolved.incremental);
        assert!(!resolved.allow_synthetic_default_imports);
        assert!(!resolved.printer.no_emit_helpers);
    }

    #[test]
    fn idempotent_second_call_is_a_no_op() {
        let mut resolved = ResolvedCompilerOptions {
            composite: true,
            import_helpers: true,
            es_module_interop: true,
            ..Default::default()
        };
        resolved.checker.verbatim_module_syntax = true;
        apply_non_strict_fanout(&mut resolved);
        let once = resolved.clone();
        apply_non_strict_fanout(&mut resolved);

        assert_eq!(once.emit_declarations, resolved.emit_declarations);
        assert_eq!(once.incremental, resolved.incremental);
        assert_eq!(
            once.checker.isolated_modules,
            resolved.checker.isolated_modules
        );
        assert_eq!(
            once.printer.preserve_const_enums,
            resolved.printer.preserve_const_enums
        );
        assert_eq!(
            once.allow_synthetic_default_imports,
            resolved.allow_synthetic_default_imports
        );
    }
}
