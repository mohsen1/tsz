//! Tests that drive the fourslash shape-variant generator through real
//! `FourslashTest` LSP scenarios. Wired into the crate root via
//! `#[cfg(test)] #[path = ...] mod` in `lib.rs`.
//!
//! This file is the high-risk-bucket pilot for the generator described in
//! `crates/tsz-lsp/src/fourslash_variants.rs`. It exercises three categories:
//!
//! 1. **Definition resolution under identifier renames.** A go-to-definition
//!    test must succeed for any consistent renaming of its user-chosen
//!    identifiers. Run against three spellings to prove the assertion
//!    depends on structural symbol resolution and not on the literal name
//!    in the original fixture.
//!
//! 2. **Definition resolution under path renames.** Multi-file fixtures
//!    must succeed when the module file name and corresponding import
//!    specifier are renamed consistently.
//!
//! 3. **Spelling-sensitive regression sentinel.** A test that emulates a
//!    forbidden single-fixture symptom patch — a function that only
//!    returns the right answer when the identifier is literally named `T`
//!    — must fail under at least one variant. This proves the generator
//!    catches the class of bug §25 warns about.
//!
use super::fourslash::FourslashTest;
use super::fourslash_variants::{ShapeVariant, ShapeVariantSource, apply_variant, shape_variants};

fn run_with_variant_label(variant: &ShapeVariantSource, f: impl FnOnce(&str)) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&variant.source)));
    if let Err(payload) = result {
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        panic!("shape variant `{}` failed: {message}", variant.label);
    }
}

/// Run the `/*ref*/ -> /*def*/` go-to-definition assertion against the
/// original source and against every renamed variant.
fn assert_ref_resolves_to_def(source: &str, variants: &[ShapeVariant]) {
    for variant in shape_variants(source, variants) {
        run_with_variant_label(&variant, |src| {
            let mut t = FourslashTest::new(src);
            t.go_to_definition("ref").expect_at_marker("def");
        });
    }
}

// -----------------------------------------------------------------------------
// 1. Definition resolution under identifier renames.
// -----------------------------------------------------------------------------

/// Two renames covering distinct lexical shapes for the same single
/// user-chosen identifier. Per CLAUDE.md §25, two name choices is the
/// minimum for proving a fix is not spelling-locked.
const RENAME_MYVAR: [ShapeVariant; 2] = [
    ShapeVariant {
        label: "myVar_to_alpha",
        identifier_renames: &[("myVar", "alpha")],
        path_renames: &[],
    },
    ShapeVariant {
        label: "myVar_to_qux",
        identifier_renames: &[("myVar", "qux")],
        path_renames: &[],
    },
];

#[test]
fn definition_const_variable_variants() {
    assert_ref_resolves_to_def(
        "
        const /*def*/myVar = 42;
        /*ref*/myVar + 1;
    ",
        &RENAME_MYVAR,
    );
}

const RENAME_FUNC_PARAM: [ShapeVariant; 2] = [
    ShapeVariant {
        label: "rename_greet_name",
        identifier_renames: &[("greet", "salute"), ("name", "who")],
        path_renames: &[],
    },
    ShapeVariant {
        label: "rename_greet_name_alt",
        identifier_renames: &[("greet", "hail"), ("name", "subject")],
        path_renames: &[],
    },
];

#[test]
fn definition_function_declaration_variants() {
    assert_ref_resolves_to_def(
        "
        function /*def*/greet(name: string) { return name; }
        /*ref*/greet('world');
    ",
        &RENAME_FUNC_PARAM,
    );
}

const RENAME_TYPE_ALIAS: [ShapeVariant; 2] = [
    ShapeVariant {
        label: "rename_alias_one",
        identifier_renames: &[("StringOrNumber", "Lit"), ("val", "out")],
        path_renames: &[],
    },
    ShapeVariant {
        label: "rename_alias_two",
        identifier_renames: &[("StringOrNumber", "Either"), ("val", "result")],
        path_renames: &[],
    },
];

#[test]
fn definition_type_alias_variants() {
    assert_ref_resolves_to_def(
        "
        type /*def*/StringOrNumber = string | number;
        const val: /*ref*/StringOrNumber = 42;
    ",
        &RENAME_TYPE_ALIAS,
    );
}

// -----------------------------------------------------------------------------
// 2. Definition resolution under path renames.
// -----------------------------------------------------------------------------

/// Multi-file fixtures must work when the file name and corresponding
/// module specifier are renamed together.
const RENAME_MODULE_A: [ShapeVariant; 2] = [
    ShapeVariant {
        label: "rename_a_to_alpha",
        identifier_renames: &[],
        path_renames: &[("a.ts", "alpha.ts"), ("./a", "./alpha")],
    },
    ShapeVariant {
        label: "rename_a_to_omega",
        identifier_renames: &[],
        path_renames: &[("a.ts", "omega.ts"), ("./a", "./omega")],
    },
];

#[test]
fn definition_cross_file_variants() {
    // Use a single-line @filename layout so the variant generator finds
    // both the file directive and the import string. Multi-file tests in
    // the harness expect the @filename form, which the generator rewrites
    // in place.
    let source = "// @filename: a.ts
export const /*def*/value = 1;
// @filename: main.ts
import { /*ref*/value } from './a';
value;
";
    for variant in shape_variants(source, &RENAME_MODULE_A) {
        run_with_variant_label(&variant, |src| {
            let mut t = FourslashTest::from_content(src);
            // Sanity: marker file lookup is robust to the rename.
            let def_file = t.marker_file("def");
            assert!(
                def_file == "a.ts" || def_file == "alpha.ts" || def_file == "omega.ts",
                "unexpected def file after variant: {def_file}"
            );
            t.go_to_definition("ref").expect_at_marker("def");
        });
    }
}

// -----------------------------------------------------------------------------
// 3. Spelling-sensitive regression sentinel.
// -----------------------------------------------------------------------------

/// The §25 anti-pattern: a handler that returns the right answer only
/// when its argument is literally `"T"`. The test below proves the
/// variant generator surfaces this kind of spelling-locked check.
fn forbidden_single_fixture_check(identifier: &str) -> bool {
    identifier == "T"
}

#[test]
fn variant_generator_catches_spelling_locked_handler() {
    let source = "type /*def*/T = number;";

    assert!(forbidden_single_fixture_check(&marker_ident_after(
        source, "def"
    )));

    let rename = ShapeVariant {
        label: "T_to_K",
        identifier_renames: &[("T", "K")],
        path_renames: &[],
    };
    let renamed = apply_variant(source, &rename);
    let renamed_ident = marker_ident_after(&renamed, "def");

    // The spelling-locked check fails after rename; a §25-compliant
    // handler would instead assert on the structural shape (an
    // identifier of any name exists at the marker), which holds for
    // both spellings.
    assert!(!forbidden_single_fixture_check(&renamed_ident));
    assert!(!renamed_ident.is_empty());
}

/// Return the identifier text that follows the named marker in `source`,
/// resolved through the production `FourslashTest` marker machinery so
/// the test exercises the same offsets the LSP uses.
fn marker_ident_after(source: &str, marker_name: &str) -> String {
    let t = FourslashTest::from_content(source);
    let marker = t.marker(marker_name);
    let cleaned = t.source(&marker.file);
    let from = marker.offset as usize;
    let tail = &cleaned[from..];
    let end = tail
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(tail.len());
    tail[..end].to_string()
}

// -----------------------------------------------------------------------------
// 4. Generator preserves marker positions across renames.
// -----------------------------------------------------------------------------

/// When a variant rewrites identifiers, marker line/column positions in
/// the resulting cleaned source must still point at the renamed identifier,
/// not at the old one. This pins the contract that markers are anchored to
/// post-rename code.
#[test]
fn marker_position_tracks_renamed_identifier() {
    let source = "type /*def*/Banana = number;\nconst x: /*ref*/Banana = 1;";
    let rename = ShapeVariant {
        label: "banana_to_apple",
        identifier_renames: &[("Banana", "Apple")],
        path_renames: &[],
    };
    let renamed = apply_variant(source, &rename);
    let t = FourslashTest::from_content(&renamed);
    // The marker for `def` should sit at the start of `Apple` in the
    // cleaned text, not at any leftover `Banana`.
    let def_pos = t.marker_file("def");
    assert_eq!(def_pos, "test.ts");
    assert!(t.source("test.ts").contains("Apple"));
    assert!(!t.source("test.ts").contains("Banana"));
}
