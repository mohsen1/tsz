//! A named import specifier's source-side name (`import { X as local }`) may
//! be a string literal since ES2022 (`import { "a,b" as local } from "mod"`).
//! `bind_import_declaration` must record that literal string as the symbol's
//! `import_name` so cross-file export lookup searches the target module for
//! the correct name — not the identifier-only fallback, which silently
//! recorded the *local* alias instead and made the export unresolvable.

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};

fn ts_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file_with_libs_stamped(
        files,
        entry,
        CheckerOptions::default(),
        &load_lib_files(&["es5.d.ts"]),
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

const DEP: &str = concat!(
    "export declare const value: number;\n",
    "export { value as \"a,b\" };\n",
    "export { value as \"as\" };\n",
    "export { value as \"from\" };\n",
);

/// A comma inside the string literal is the shape that most obviously needs
/// quote-aware handling; `TS2322` on the mismatched consumer proves the
/// import resolved to the real export's `number` type, not `any`/error.
#[test]
fn comma_in_export_name_resolves_to_declared_type() {
    let files = [
        ("dep.d.ts", DEP),
        (
            "index.ts",
            "import { \"a,b\" as CommaName } from \"./dep\";\nconst ok: number = CommaName;\nconst bad: string = CommaName;\n",
        ),
    ];
    let codes = ts_codes(&files, "index.ts");
    assert_eq!(codes, vec![2322], "codes: {codes:?}");
}

/// The source name coincides with the `as`/`from` contextual keywords used by
/// the import-specifier grammar itself; the string literal's *text* must not
/// be confused with the keyword tokens that surround it.
#[test]
fn as_keyword_shaped_export_name_resolves_to_declared_type() {
    let files = [
        ("dep.d.ts", DEP),
        (
            "index.ts",
            "import { \"as\" as AsName } from \"./dep\";\nconst ok: number = AsName;\nconst bad: string = AsName;\n",
        ),
    ];
    let codes = ts_codes(&files, "index.ts");
    assert_eq!(codes, vec![2322], "codes: {codes:?}");
}

#[test]
fn from_keyword_shaped_export_name_resolves_to_declared_type() {
    let files = [
        ("dep.d.ts", DEP),
        (
            "index.ts",
            "import { \"from\" as FromName } from \"./dep\";\nconst ok: number = FromName;\nconst bad: string = FromName;\n",
        ),
    ];
    let codes = ts_codes(&files, "index.ts");
    assert_eq!(codes, vec![2322], "codes: {codes:?}");
}

/// Negative control: a non-renamed, plain-identifier import of the same
/// module keeps resolving exactly as before.
#[test]
fn plain_identifier_export_name_still_resolves() {
    let files = [
        ("dep.d.ts", "export declare const plain: number;\n"),
        (
            "index.ts",
            "import { plain } from \"./dep\";\nconst ok: number = plain;\nconst bad: string = plain;\n",
        ),
    ];
    let codes = ts_codes(&files, "index.ts");
    assert_eq!(codes, vec![2322], "codes: {codes:?}");
}
