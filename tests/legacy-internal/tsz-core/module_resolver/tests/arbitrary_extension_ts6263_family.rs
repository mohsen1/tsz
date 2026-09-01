//! TS6263 for the *general* arbitrary-extension declaration family at
//! `ModuleResolver::lookup()`.
//!
//! Complements `json_decl_companion` (#17021), which exhaustively covers the
//! `.json` companion matrix. This module guards the branches that the json
//! fix shares but that no `lookup()`-level test exercised:
//!
//! 1. The non-`json` family (`.css`/`.html`/`.svelte`/`.node`, ...): a
//!    genuinely-unknown extension resolves to its `<base>.d.<ext>.ts` companion
//!    and reports TS6263 when `--allowArbitraryExtensions` is off, clean when
//!    on. The existing `arbitrary_ext_decl_module_resolution_tests` cover this
//!    family only through the checker's in-memory path, which emits no
//!    resolution diagnostics — so the driver-owned gate
//!    (`is_arbitrary_extension_declaration` + the `lookup` emit site) had no
//!    guard. `.json` and this family share that predicate, so a broad
//!    short-circuit re-silencing one would silence the other; these catch it.
//!
//! 2. The declaration-file-importer exemption: tsc does not report TS6263 when
//!    the importer is itself a declaration file (`.d.ts`/`.d.mts`/`.d.cts`) —
//!    those already live in the type-declaration layer. Unguarded before now,
//!    for both the family and `.json`.
//!
//! 3. A multi-dot base (`data.config.json` -> `data.config.d.json.ts`): the
//!    `with_extension` companion shape and the `ends_with(".d.<ext>.ts")`
//!    predicate must agree on a specifier whose base already contains a dot.
//!
//! Structural rule (shared with #17021): resolution lands on the declaration
//! companion regardless of the flag; the flag only toggles the diagnostic. The
//! resolved path is always preserved so the imported type still binds.

use super::super::*;
use super::fixtures::TempFixture;

const TS6263: u32 = MODULE_WAS_RESOLVED_TO_BUT_ALLOW_ARBITRARY_EXTENSIONS_IS_NOT_SET;

/// One arbitrary-extension family member: the user-written specifier and the
/// `<base>.d.<ext>.ts` companion file it resolves through.
struct FamilyCase {
    specifier: &'static str,
    companion: &'static str,
}

const FAMILY: &[FamilyCase] = &[
    FamilyCase {
        specifier: "./style.css",
        companion: "style.d.css.ts",
    },
    FamilyCase {
        specifier: "./component.html",
        companion: "component.d.html.ts",
    },
    FamilyCase {
        specifier: "./Widget.svelte",
        companion: "Widget.d.svelte.ts",
    },
    FamilyCase {
        specifier: "./addon.node",
        companion: "addon.d.node.ts",
    },
];

fn lookup_outcome(
    fixture: &TempFixture,
    importer: &str,
    specifier: &str,
    allow_arbitrary_extensions: bool,
) -> ModuleLookupOutcome {
    let options = ResolvedCompilerOptions {
        module_resolution: Some(ModuleResolutionKind::Node),
        allow_arbitrary_extensions,
        module_suffixes: vec![String::new()],
        ..Default::default()
    };
    let mut resolver = ModuleResolver::new(&options);
    let importer_path = fixture.join(importer);
    let request = ModuleLookupRequest {
        specifier,
        containing_file: &importer_path,
        specifier_span: Span::new(0, specifier.len() as u32),
        import_kind: ImportKind::EsmImport,
        resolution_mode_override: None,
        no_implicit_any: false,
        implied_classic_resolution: false,
    };
    resolver
        .lookup(&request, |_, _| None, |_| false, None)
        .classify()
}

/// Flag off: every family member resolves to its declaration companion and
/// reports TS6263, with the resolved path preserved (the diagnostic is
/// additive). The message names the companion and the flag.
#[test]
fn family_reports_ts6263_when_flag_off_across_extensions() {
    for case in FAMILY {
        let fixture = TempFixture::new();
        fixture.write("main.ts", &format!("import x from '{}';", case.specifier));
        fixture.write(
            case.companion,
            "declare const x: number;\nexport default x;\n",
        );

        let outcome = lookup_outcome(&fixture, "main.ts", case.specifier, false);

        let resolved = outcome
            .resolved_path
            .as_ref()
            .unwrap_or_else(|| panic!("{}: resolved path preserved on TS6263", case.specifier));
        assert!(
            resolved.ends_with(case.companion),
            "{}: expected resolution to {}, got {}",
            case.specifier,
            case.companion,
            resolved.display()
        );
        let error = outcome
            .error
            .as_ref()
            .unwrap_or_else(|| panic!("{}: expected TS6263 when flag off", case.specifier));
        assert_eq!(
            error.code, TS6263,
            "{}: expected TS6263, got {error:?}",
            case.specifier
        );
        assert!(
            error.message.contains(case.companion)
                && error.message.contains("allowArbitraryExtensions"),
            "{}: message names companion and flag: {}",
            case.specifier,
            error.message
        );
    }
}

/// Flag on: the same resolutions are silent.
#[test]
fn family_is_clean_when_flag_on_across_extensions() {
    for case in FAMILY {
        let fixture = TempFixture::new();
        fixture.write("main.ts", &format!("import x from '{}';", case.specifier));
        fixture.write(
            case.companion,
            "declare const x: number;\nexport default x;\n",
        );

        let outcome = lookup_outcome(&fixture, "main.ts", case.specifier, true);

        assert!(
            outcome.error.is_none(),
            "{}: flag on must be clean, got {:?}",
            case.specifier,
            outcome.error
        );
        let resolved = outcome
            .resolved_path
            .as_ref()
            .unwrap_or_else(|| panic!("{}: resolved path", case.specifier));
        assert!(
            resolved.ends_with(case.companion),
            "{}: expected resolution to {}",
            case.specifier,
            case.companion
        );
    }
}

/// Declaration-file importers are exempt from TS6263 (they already live in the
/// type layer), for both the family and `.json`. Covers `.d.ts`, `.d.mts`, and
/// `.d.cts` importers, and asserts resolution still lands on the companion.
#[test]
fn declaration_file_importer_is_exempt_from_ts6263() {
    let importers = ["host.d.ts", "host.d.mts", "host.d.cts"];
    for importer in importers {
        // Family member (.css).
        let fixture = TempFixture::new();
        fixture.write(importer, "import s from './style.css';");
        fixture.write(
            "style.d.css.ts",
            "declare const s: number;\nexport default s;\n",
        );
        let outcome = lookup_outcome(&fixture, importer, "./style.css", false);
        assert!(
            outcome.error.is_none(),
            "importer {importer}: .css companion is exempt from TS6263, got {:?}",
            outcome.error
        );
        assert!(
            outcome
                .resolved_path
                .as_ref()
                .is_some_and(|p| p.ends_with("style.d.css.ts")),
            "importer {importer}: resolution still lands on the .css companion"
        );

        // `.json` companion, same exemption.
        let fixture = TempFixture::new();
        fixture.write(importer, "import d from './data.json';");
        fixture.write("data.json", "{}");
        fixture.write(
            "data.d.json.ts",
            "declare const d: number;\nexport default d;\n",
        );
        let outcome = lookup_outcome(&fixture, importer, "./data.json", false);
        assert!(
            outcome.error.is_none(),
            "importer {importer}: .json companion is exempt from TS6263, got {:?}",
            outcome.error
        );
        assert!(
            outcome
                .resolved_path
                .as_ref()
                .is_some_and(|p| p.ends_with("data.d.json.ts")),
            "importer {importer}: resolution still lands on the .json companion"
        );
    }
}

/// A specifier whose base already contains a dot (`data.config.json`) must
/// still match its `data.config.d.json.ts` companion — the `with_extension`
/// companion shape and the `ends_with(".d.json.ts")` predicate have to agree
/// on a multi-dot base.
#[test]
fn multi_dot_json_base_reports_ts6263_when_flag_off() {
    let fixture = TempFixture::new();
    fixture.write("main.ts", "import d from './data.config.json';");
    fixture.write("data.config.json", "{}");
    fixture.write(
        "data.config.d.json.ts",
        "declare const d: number;\nexport default d;\n",
    );

    let outcome = lookup_outcome(&fixture, "main.ts", "./data.config.json", false);

    let resolved = outcome
        .resolved_path
        .as_ref()
        .expect("resolved path preserved on TS6263");
    assert!(
        resolved.ends_with("data.config.d.json.ts"),
        "expected the multi-dot companion, got {}",
        resolved.display()
    );
    assert_eq!(
        outcome.error.as_ref().map(|e| e.code),
        Some(TS6263),
        "multi-dot json base must report TS6263 when flag off: {:?}",
        outcome.error
    );
}
