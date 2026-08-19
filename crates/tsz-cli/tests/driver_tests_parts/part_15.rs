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

// ---------------------------------------------------------------------------
// Cross-file interface declaration merging: overload resolution order.
//
// When one global interface symbol has declarations in multiple program
// files, tsc resolves calls against the merged overload set with the LATER
// declaration group's signatures tried first (reorderCandidates), while the
// stored/display order stays forward. Program order is the driver's file
// order. Oracle: tsc 6.0.2/7.0.2 on each fixture (#17646 cross-file
// follow-up). The same-file re-open ordering is fenced separately by
// `merged_interface_overload_order_tests` in tsz-checker (#17652).
// ---------------------------------------------------------------------------

fn compile_ordered_files(files: &[(&str, &str)]) -> (TempDir, Vec<(u32, String)>) {
    let tmp = TempDir::new().unwrap();
    let base = &tmp.path;
    for (name, contents) in files {
        write_file(&base.join(name), contents);
    }
    let mut args = default_args();
    args.no_emit = true;
    args.types = Some(Vec::new());
    args.files = files.iter().map(|(name, _)| PathBuf::from(name)).collect();
    let result = compile(&args, base).expect("compile should succeed");
    let diags = result
        .diagnostics
        .iter()
        .map(|diag| (diag.code, diag.message_text.clone()))
        .collect();
    (tmp, diags)
}

#[test]
fn cross_file_interface_merge_call_prefers_later_declaration_group() {
    let (_tmp, diags) = compile_ordered_files(&[
        ("early.ts", "interface Widget { paint(x: string): 1 }\n"),
        (
            "late.ts",
            "interface Widget { paint(x: string): 2 }\ndeclare const w: Widget;\nconst chosen: 2 = w.paint(\"x\");\n",
        ),
    ]);
    assert!(
        diags.is_empty(),
        "later file's declaration group must win the merged-overload call, got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_earlier_group_annotation_reports_ts2322() {
    let (_tmp, diags) = compile_ordered_files(&[
        ("first.ts", "interface Gauge { read(x: string): 1 }\n"),
        (
            "second.ts",
            "interface Gauge { read(x: string): 2 }\ndeclare const gauge: Gauge;\nconst wrong: 1 = gauge.read(\"x\");\n",
        ),
    ]);
    assert!(
        diags.iter().any(|(code, message)| *code == 2322
            && message.contains("'2'")
            && message.contains("'1'")),
        "annotating with the earlier group's return type must fail (call returns 2), got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_reversed_file_order_flips_winner() {
    // Same sources as the first test but the file carrying the use comes
    // FIRST, so the other file's group is now the later one and must win.
    let (_tmp, diags) = compile_ordered_files(&[
        (
            "use.ts",
            "interface Panel { draw(x: string): 2 }\ndeclare const p: Panel;\nconst picked: 2 = p.draw(\"x\");\n",
        ),
        ("other.ts", "interface Panel { draw(x: string): 1 }\n"),
    ]);
    assert!(
        diags.iter().any(|(code, message)| *code == 2322
            && message.contains("'1'")
            && message.contains("'2'")),
        "with reversed program order the other file's group is later and must win, got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_usage_in_earlier_file_prefers_later_group() {
    let (_tmp, diags) = compile_ordered_files(&[
        (
            "consumer.ts",
            "interface Meter { sample(x: string): 1 }\ndeclare const meter: Meter;\nconst level: 2 = meter.sample(\"x\");\n",
        ),
        ("extension.ts", "interface Meter { sample(x: string): 2 }\n"),
    ]);
    assert!(
        diags.is_empty(),
        "a call in the earlier file still resolves against the later declaration group, got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_three_files_pick_latest_group() {
    let (_tmp, diags) = compile_ordered_files(&[
        ("one.ts", "interface Chain { link(x: string): 1 }\n"),
        ("two.ts", "interface Chain { link(x: string): 2 }\n"),
        (
            "three.ts",
            "interface Chain { link(x: string): 3 }\ndeclare const chain: Chain;\nconst last: 3 = chain.link(\"x\");\n",
        ),
    ]);
    assert!(
        diags.is_empty(),
        "the latest of three declaration groups must win, got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_multi_reopen_in_earlier_file_composes_with_later_file() {
    let (_tmp, diags) = compile_ordered_files(&[
        (
            "reopened.ts",
            "interface Stack { top(x: string): 1 }\ninterface Stack { top(x: string): 2 }\n",
        ),
        (
            "final.ts",
            "interface Stack { top(x: string): 3 }\ndeclare const stack: Stack;\nconst tip: 3 = stack.top(\"x\");\n",
        ),
    ]);
    assert!(
        diags.is_empty(),
        "same-file re-open groups compose with a later file's group (later file wins), got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_non_matching_later_group_falls_through_to_earlier() {
    let (_tmp, diags) = compile_ordered_files(&[
        ("base.ts", "interface Port { open(x: string): 1 }\n"),
        (
            "extra.ts",
            "interface Port { open(x: number): 9 }\ndeclare const port: Port;\nconst fallback: 1 = port.open(\"s\");\n",
        ),
    ]);
    assert!(
        diags.is_empty(),
        "a later group that does not match the arguments falls through to the earlier group, got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_bare_call_signatures_prefer_later_group() {
    let (_tmp, diags) = compile_ordered_files(&[
        (
            "callable_a.ts",
            "interface Factory { (x: string): 1; new (x: string): Factory }\n",
        ),
        (
            "callable_b.ts",
            "interface Factory { (x: string): 2 }\ndeclare const make: Factory;\nconst made: 2 = make(\"s\");\n",
        ),
    ]);
    assert!(
        diags.is_empty(),
        "interface-level call signatures from the later file must be tried first, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Cross-file interface declaration merging: construct-signature (`new`)
// resolution order. tsc's reorderCandidates is shared between call and
// construct resolution, so the later program file's construct group is tried
// first too. Oracle: tsc 7.0.2 on each fixture. The same-file re-open
// construct ordering is fenced by `merged_interface_construct_order_tests`
// in tsz-checker.
// ---------------------------------------------------------------------------

#[test]
fn cross_file_interface_merge_new_prefers_later_declaration_group() {
    let (_tmp, diags) = compile_ordered_files(&[
        ("early.ts", "interface WidgetCtor { new (x: string): 1 }\n"),
        (
            "late.ts",
            "interface WidgetCtor { new (x: string): 2 }\ndeclare const Widget: WidgetCtor;\nconst built: 2 = new Widget(\"x\");\n",
        ),
    ]);
    assert!(
        diags.is_empty(),
        "later file's construct group must win the merged `new`, got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_new_usage_in_earlier_file_prefers_later_group() {
    let (_tmp, diags) = compile_ordered_files(&[
        (
            "consumer.ts",
            "interface RigCtor { new (x: string): 1 }\ndeclare const Rig: RigCtor;\nconst built: 2 = new Rig(\"x\");\n",
        ),
        ("extension.ts", "interface RigCtor { new (x: string): 2 }\n"),
    ]);
    assert!(
        diags.is_empty(),
        "a `new` in the earlier file still resolves against the later construct group, got: {diags:?}"
    );
}

#[test]
fn cross_file_interface_merge_new_reversed_file_order_flips_winner() {
    let (_tmp, diags) = compile_ordered_files(&[
        (
            "use.ts",
            "interface FlipCtor { new (x: string): 2 }\ndeclare const Flip: FlipCtor;\nconst flipped: 2 = new Flip(\"x\");\n",
        ),
        ("other.ts", "interface FlipCtor { new (x: string): 1 }\n"),
    ]);
    assert!(
        diags.iter().any(|(code, message)| *code == 2322
            && message.contains("'1'")
            && message.contains("'2'")),
        "with reversed program order the other file's construct group is later and must win, got: {diags:?}"
    );
}
