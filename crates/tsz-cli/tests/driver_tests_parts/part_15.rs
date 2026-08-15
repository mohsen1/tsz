// Additional driver tests split out of `part_10.rs` to keep each shard under
// the 2000-line arch-size budget. Included by `driver_tests.rs` alongside the
// other `part_NN.rs` shards, sharing the same imports and helpers.

#[test]
fn checked_js_direct_file_jsdoc_import_string_literal_export_names_resolve() {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;

    write_file(
        &base.join("dep.d.ts"),
        r#"export declare const value: number;
export { value as "a,b" };
export { value as "as" };
export { value as "from" };
"#,
    );
    write_file(
        &base.join("index.js"),
        r#"// @ts-check
/** @import { "a,b" as CommaName, "as" as AsName, "from" as FromName } from "./dep" */
/** @type {CommaName} */
const a = "x";
/** @type {AsName} */
const b = "x";
/** @type {FromName} */
const c = "x";
"#,
    );

    let mut args = default_args();
    args.allow_js = true;
    args.check_js = true;
    args.no_emit = true;
    args.types = Some(Vec::new());
    args.files = vec![PathBuf::from("index.js")];

    let result = compile(&args, base).expect("compile should succeed");
    let codes: Vec<u32> = result.diagnostics.iter().map(|diag| diag.code).collect();

    // Same as the tsconfig-driven sibling: the `@import` aliases resolve to the
    // exported *value* `value`, so using them in `@type` positions is TS2749
    // (value-used-as-type) at each use, naming the local alias — not TS2322 and
    // not a TS2694 at the `@import` clause. Oracle 7.0.2. See #17551.
    let value_as_type = diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF;
    let value_as_type_count = codes.iter().filter(|&&code| code == value_as_type).count();
    assert_eq!(
        value_as_type_count, 3,
        "Expected three TS2749 (value-used-as-type) diagnostics from the direct-file JSDoc import aliases, got diagnostics: {:?}",
        result.diagnostics
    );
    for alias in ["CommaName", "AsName", "FromName"] {
        assert!(
            result.diagnostics.iter().any(|diag| diag.code == value_as_type
                && diag.message_text.contains(&format!("'{alias}'"))),
            "Expected a TS2749 naming the local alias '{alias}', got diagnostics: {:?}",
            result.diagnostics
        );
    }
    assert!(
        !codes.contains(&2322),
        "The alias uses are value-as-type errors, not assignability (TS2322) errors: {:?}",
        result.diagnostics
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME)
            && !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN)
            && !codes.contains(&diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER)
            && !codes.contains(&diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER)
            && !codes.contains(&diagnostic_codes::HAS_NO_EXPORTED_MEMBER_NAMED_DID_YOU_MEAN),
        "String-literal export names should resolve without unresolved-name or bogus member diagnostics: {:?}",
        result.diagnostics
    );
}
