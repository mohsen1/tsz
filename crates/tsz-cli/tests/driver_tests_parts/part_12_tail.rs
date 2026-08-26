// =========================================================================
// TS5097/TS2846 for dynamic `import()` module specifiers
//
// tsc emits the TypeScript-extension diagnostics (TS5097 for `.ts`, TS2846 for
// `.d.ts`) only on the *resolved-module* branch of `resolveExternalModule`. A
// dynamic `import("./x.ts")` whose target does not exist is a plain
// "cannot find module" (TS2307) — never an extension diagnostic, and never
// both stacked together. Verified against tsc 6.0.2:
//   import("./pieces.ts")  (exists)  -> TS5097
//   import("./missing.ts") (absent)  -> TS2307   (NOT TS5097, NOT both)
//   import("./shapes.d.ts")(exists)  -> TS2846
//   import("./nope.d.ts")  (absent)  -> TS2307
// =========================================================================

/// Compile a `board.ts` whose body is `entry_source`, with `pieces.ts` present.
fn dynamic_import_matrix_compile(
    base: &Path,
    entry_source: &str,
    extra_options: &str,
) -> Vec<u32> {
    write_file(
        &base.join("tsconfig.json"),
        &format!(
            r#"{{
              "compilerOptions": {{
                "module": "esnext",
                "moduleResolution": "bundler",
                "noEmit": true{extra_options}
              }},
              "include": ["**/*"]
            }}"#
        ),
    );
    write_file(&base.join("pieces.ts"), "export const rook = 1;\n");
    write_file(&base.join("shapes.d.ts"), "export declare const bishop: number;\n");
    write_file(&base.join("board.ts"), entry_source);

    let args = default_args();
    let result = compile(&args, base).expect("compile should succeed");
    result.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn dynamic_import_resolved_ts_extension_reports_ts5097() {
    let temp = TempDir::new().expect("temp dir");
    let codes = dynamic_import_matrix_compile(
        temp.path.as_path(),
        "export async function load() { return import('./pieces.ts'); }\n",
        "",
    );
    assert_eq!(
        codes,
        vec![5097],
        "a resolved dynamic import of a .ts specifier must report TS5097, got: {codes:?}"
    );
}

#[test]
fn dynamic_import_missing_ts_extension_reports_ts2307_only() {
    // Regression guard: tsz previously emitted the extension diagnostic
    // unconditionally for dynamic imports, so an unresolved `./missing.ts`
    // produced TS5097 (and stacked TS2307/TS5097), where tsc reports TS2307
    // alone. The fix routes dynamic imports through the shared, resolution-
    // gated `check_module_specifier_ts_extension` gateway and drops the
    // resolver's NotFound -> TS5097 upgrade.
    let temp = TempDir::new().expect("temp dir");
    let codes = dynamic_import_matrix_compile(
        temp.path.as_path(),
        "export async function load() { return import('./missing.ts'); }\n",
        "",
    );
    assert_eq!(
        codes,
        vec![diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS],
        "an unresolved dynamic import of a .ts specifier must report TS2307 \
         alone (no spurious TS5097), got: {codes:?}"
    );
}

#[test]
fn dynamic_import_resolved_dts_extension_reports_ts2846() {
    let temp = TempDir::new().expect("temp dir");
    let codes = dynamic_import_matrix_compile(
        temp.path.as_path(),
        "export async function load() { return import('./shapes.d.ts'); }\n",
        "",
    );
    assert_eq!(
        codes,
        vec![diagnostic_codes::A_DECLARATION_FILE_CANNOT_BE_IMPORTED_WITHOUT_IMPORT_TYPE_DID_YOU_MEAN_TO_IMPORT],
        "a resolved dynamic import of a .d.ts specifier must report TS2846, got: {codes:?}"
    );
}

#[test]
fn dynamic_import_missing_dts_extension_reports_ts2307_only() {
    let temp = TempDir::new().expect("temp dir");
    let codes = dynamic_import_matrix_compile(
        temp.path.as_path(),
        "export async function load() { return import('./nope.d.ts'); }\n",
        "",
    );
    assert_eq!(
        codes,
        vec![diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS],
        "an unresolved dynamic import of a .d.ts specifier must report TS2307 \
         alone (no spurious TS2846), got: {codes:?}"
    );
}

#[test]
fn dynamic_import_ts_extension_allowed_with_allow_importing_ts_extensions() {
    let temp = TempDir::new().expect("temp dir");
    let codes = dynamic_import_matrix_compile(
        temp.path.as_path(),
        "export async function load() { return import('./pieces.ts'); }\n",
        ",\n\"allowImportingTsExtensions\": true",
    );
    assert!(
        codes.is_empty(),
        "allowImportingTsExtensions must silence TS5097 for dynamic imports, got: {codes:?}"
    );
}

#[test]
fn dynamic_import_extensionless_specifier_reports_nothing() {
    let temp = TempDir::new().expect("temp dir");
    let codes = dynamic_import_matrix_compile(
        temp.path.as_path(),
        "export async function load() { return import('./pieces'); }\n",
        "",
    );
    assert!(
        codes.is_empty(),
        "an extensionless resolved dynamic import is the no-diagnostic control, got: {codes:?}"
    );
}
