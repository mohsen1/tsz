//! Assertion-overlap coverage for default-imported generic type aliases.
//!
//! Structural rule: when an explicit default export names a generic type alias,
//! the importing file must receive the alias's structural body and exact type
//! parameters. The solver can then substitute the application arguments before
//! checking assertion overlap. Named imports and non-generic default aliases are
//! controls; genuinely disjoint structural targets must still report `TS2352`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_global_index, check_multi_file_with_libs_stamped, load_lib_files,
};
use tsz_common::common::ModuleKind;
use tsz_common::diagnostics::Diagnostic;

fn strict_options() -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::CommonJS,
        strict: true,
        ..CheckerOptions::default()
    }
}

fn check(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    check_multi_file_with_global_index(files, "main.ts", strict_options())
}

const ARRIVAL: &str = r#"
type Arrival<Payload> = { success: true; value: Payload };
export default Arrival;
"#;

const REJECTION: &str = r#"
type Rejection = { success: false; message: string };
export default Rejection;
"#;

const RESOLUTION: &str = r#"
import type FailedOutcome from "./rejection";
import type SuccessfulOutcome from "./arrival";
type Resolution<Item> = SuccessfulOutcome<Item> | FailedOutcome;
export default Resolution;
"#;

#[test]
fn renamed_default_imported_generic_union_overlaps_member_array() {
    let diagnostics = check(&[
        ("arrival.ts", ARRIVAL),
        ("rejection.ts", REJECTION),
        ("resolution.ts", RESOLUTION),
        (
            "main.ts",
            r#"
import type Outcome from "./resolution";
import type Passed from "./arrival";
declare const outcomes: Outcome<unknown>[];
const accepted = outcomes as Passed<any>[];
accepted[0]?.value;
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "a renamed default-imported generic union must overlap one member; got {diagnostics:#?}",
    );
}

#[test]
fn renamed_default_imported_generic_union_overlaps_member() {
    let diagnostics = check(&[
        ("arrival.ts", ARRIVAL),
        ("rejection.ts", REJECTION),
        ("resolution.ts", RESOLUTION),
        (
            "main.ts",
            r#"
import type Outcome from "./resolution";
import type Passed from "./arrival";
declare const outcome: Outcome<unknown>;
const accepted = outcome as Passed<any>;
accepted.value;
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "a renamed default-imported generic union must overlap one member; got {diagnostics:#?}",
    );
}

#[test]
fn default_imported_generic_union_overlaps_inline_structural_target() {
    let diagnostics = check(&[
        ("arrival.ts", ARRIVAL),
        ("rejection.ts", REJECTION),
        ("resolution.ts", RESOLUTION),
        (
            "main.ts",
            r#"
import type Outcome from "./resolution";
declare const outcomes: Outcome<unknown>[];
const accepted = outcomes as { success: true; value: any }[];
accepted[0]?.value;
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "the imported source alias body must expand independently of target alias identity; got {diagnostics:#?}",
    );
}

#[test]
fn default_imported_generic_union_is_file_order_independent() {
    let main = r#"
import type Outcome from "./resolution";
declare const outcomes: Outcome<unknown>[];
const accepted = outcomes as { success: true; value: any }[];
"#;
    for files in [
        [
            ("arrival.ts", ARRIVAL),
            ("rejection.ts", REJECTION),
            ("resolution.ts", RESOLUTION),
            ("main.ts", main),
        ],
        [
            ("main.ts", main),
            ("resolution.ts", RESOLUTION),
            ("rejection.ts", REJECTION),
            ("arrival.ts", ARRIVAL),
        ],
    ] {
        let diagnostics = check(&files);
        assert!(
            diagnostics.is_empty(),
            "default-alias lowering must not depend on binder/file order; got {diagnostics:#?}",
        );
    }
}

#[test]
fn default_imported_generic_union_keeps_disjoint_ts2352() {
    let diagnostics = check(&[
        ("arrival.ts", ARRIVAL),
        ("rejection.ts", REJECTION),
        ("resolution.ts", RESOLUTION),
        (
            "main.ts",
            r#"
import type Outcome from "./resolution";
declare const outcomes: Outcome<unknown>[];
const impossible = outcomes as { success: "other"; value: number }[];
"#,
        ),
    ]);

    let codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(
        codes,
        vec![2352],
        "materializing the imported alias must not make disjoint assertions compatible; got {diagnostics:#?}",
    );
}

#[test]
fn named_imported_generic_union_control_remains_clean() {
    let diagnostics = check(&[
        (
            "types.ts",
            r#"
export type Accepted<Data> = { ok: true; data: Data };
export type Declined = { ok: false; reason: string };
export type Decision<Data> = Accepted<Data> | Declined;
"#,
        ),
        (
            "main.ts",
            r#"
import type { Accepted, Decision } from "./types";
declare const decisions: Decision<unknown>[];
const accepted = decisions as Accepted<any>[];
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "the existing named-import shortcut must remain clean; got {diagnostics:#?}",
    );
}

#[test]
fn non_generic_default_alias_control_uses_existing_path() {
    let diagnostics = check(&[
        (
            "static-result.ts",
            r#"
type StaticResult =
    | { state: "ready"; value: unknown }
    | { state: "failed"; reason: string };
export default StaticResult;
"#,
        ),
        (
            "main.ts",
            r#"
import type StaticResult from "./static-result";
declare const result: StaticResult;
const ready = result as { state: "ready"; value: any };
"#,
        ),
    ]);

    assert!(
        diagnostics.is_empty(),
        "non-generic default aliases stay on their existing resolution path; got {diagnostics:#?}",
    );
}

#[test]
fn renamed_default_alias_beats_same_named_lib_utility() {
    let libs = load_lib_files(&["es5.d.ts"]);
    for local_name in ["Package", "Record"] {
        let wrapper = format!(
            "import type {local_name} from './parcel'; type Wrapper<Value> = {{ direct: Value; parcel: {local_name}<Value>; values: Array<Value> }}; export default Wrapper;"
        );
        let diagnostics = check_multi_file_with_libs_stamped(
            &[
                (
                    "parcel.ts",
                    "type Parcel<Value> = { content: Value }; export default Parcel;",
                ),
                ("wrapper.ts", wrapper.as_str()),
                (
                    "main.ts",
                    "import type Wrapped from './wrapper'; declare const value: Wrapped<string>; value.direct.toUpperCase(); value.parcel.content.toUpperCase(); value.values[0].toUpperCase();",
                ),
            ],
            "main.ts",
            strict_options(),
            &libs,
        );

        assert!(
            diagnostics.is_empty(),
            "owner-known default alias {local_name} must preserve nested generic substitution; got {diagnostics:#?}",
        );
    }
}

#[test]
fn top_level_default_alias_beats_same_named_lib_utility() {
    let libs = load_lib_files(&["es5.d.ts"]);
    let diagnostics = check_multi_file_with_libs_stamped(
        &[
            (
                "parcel.ts",
                "type Parcel<Value> = { content: Value }; export default Parcel;",
            ),
            (
                "main.ts",
                "import type Record from './parcel'; declare const value: Record<string>; value.content.toUpperCase();",
            ),
        ],
        "main.ts",
        strict_options(),
        &libs,
    );

    assert!(
        diagnostics.is_empty(),
        "an explicit default import must shadow a same-named lib utility at the use site; got {diagnostics:#?}",
    );
}

#[test]
fn default_alias_preserves_constraints_and_trailing_defaults() {
    let files = [
        (
            "parcel.ts",
            "type Parcel<Content extends string, Extra = Content> = { content: Content; extra: Extra }; export default Parcel;",
        ),
        (
            "main.ts",
            "import type Wrapped from './parcel'; declare const value: Wrapped<'ready'>; const content: 'ready' = value.content; const extra: 'ready' = value.extra;",
        ),
    ];
    let diagnostics = check(&files);
    assert!(
        diagnostics.is_empty(),
        "owner-qualified alias parameters must preserve constraints and dependent defaults; got {diagnostics:#?}",
    );

    let diagnostics = check(&[
        files[0],
        (
            "main.ts",
            "import type Wrapped from './parcel'; type Excess = Wrapped<'ok', number, boolean>; type Invalid = Wrapped<number>;",
        ),
    ]);
    let mut codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    codes.sort_unstable();
    assert_eq!(
        codes,
        vec![2344, 2707],
        "excess arguments and violated constraints must keep their diagnostics; got {diagnostics:#?}",
    );
}

#[test]
fn default_export_alias_beats_same_named_lib_utility_in_provider() {
    let libs = load_lib_files(&["es5.d.ts"]);
    let diagnostics = check_multi_file_with_libs_stamped(
        &[
            (
                "record.ts",
                "type Record<Value> = { content: Value }; export default Record;",
            ),
            (
                "main.ts",
                "import type Wrapped from './record'; declare const value: Wrapped<string>; value.content.toUpperCase();",
            ),
        ],
        "main.ts",
        strict_options(),
        &libs,
    );

    assert!(
        diagnostics.is_empty(),
        "the provider's explicit default target must beat a same-named lib utility; got {diagnostics:#?}",
    );
}

#[test]
fn default_alias_from_external_declaration_file_uses_published_body() {
    let libs = load_lib_files(&["es5.d.ts"]);
    let diagnostics = check_multi_file_with_libs_stamped(
        &[
            (
                "/node_modules/parcel/index.d.ts",
                "type Record<Value> = { content: Value }; export default Record;",
            ),
            (
                "main.ts",
                "import type Wrapped from 'parcel'; declare const value: Wrapped<string>; value.content.toUpperCase();",
            ),
        ],
        "main.ts",
        strict_options(),
        &libs,
    );

    assert!(
        diagnostics.is_empty(),
        "an external declaration alias must instantiate from its exact published body despite a same-named lib utility; got {diagnostics:#?}",
    );
}

#[test]
fn default_alias_application_marks_only_the_used_import_referenced() {
    let mut options = strict_options();
    options.no_unused_locals = true;
    let diagnostics = check_multi_file_with_global_index(
        &[
            (
                "parcel.ts",
                "type Parcel<Value> = { content: Value }; export default Parcel;",
            ),
            (
                "other.ts",
                "type Other<Value> = { other: Value }; export default Other;",
            ),
            (
                "main.ts",
                "import type Used from './parcel'; import type Unused from './other'; export declare const value: Used<string>;",
            ),
        ],
        "main.ts",
        options,
    );

    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![6133],
        "the early alias path must mark Used, while the unused neighbor still reports; got {diagnostics:#?}",
    );
    assert!(
        diagnostics[0].message_text.contains("Unused"),
        "the remaining TS6133 must belong to the unused neighbor; got {diagnostics:#?}",
    );
}
