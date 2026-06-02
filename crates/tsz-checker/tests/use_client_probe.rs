use crate::context::CheckerOptions;
use crate::test_utils::check_multi_file;
use tsz_common::common::{ModuleKind, ScriptTarget};

/// Core repro: exported function whose parameter type is Parameters<typeof LocalFn>[0]
/// where `LocalFn` is NOT exported. Cross-file callers should see the concrete type.
#[test]
fn probe_use_client_parameters_typeof_local_fn() {
    let diagnostics = check_multi_file(
        &[
            (
                "client.ts",
                r#""use client";

function Inner(props: { name: string; count: number }) {
    return null;
}

export function Widget(props: Parameters<typeof Inner>[0]) {
    return null;
}
"#,
            ),
            (
                "server.ts",
                r#"import { Widget } from "./client";
export default function Page() {
    return Widget({ name: "hello", count: 42 });
}
"#,
            ),
        ],
        "server.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    );

    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "use client + Parameters<typeof LocalFn>[0]: TS2345 should not fire, got: {ts2345:#?}\nAll: {diagnostics:#?}"
    );
}

/// Same pattern without "use client" - the bug should exist independently.
#[test]
fn probe_parameters_typeof_local_fn_no_directive() {
    let diagnostics = check_multi_file(
        &[
            (
                "lib.ts",
                r#"function Inner(props: { name: string; count: number }) {
    return null;
}

export function Widget(props: Parameters<typeof Inner>[0]) {
    return null;
}
"#,
            ),
            (
                "main.ts",
                r#"import { Widget } from "./lib";
Widget({ name: "hello", count: 42 });
"#,
            ),
        ],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    );

    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "Parameters<typeof LocalFn>[0] no directive: TS2345 should not fire, got: {ts2345:#?}\nAll: {diagnostics:#?}"
    );
}

/// With exported `LocalFn`, there should be no issue (control case).
#[test]
fn probe_parameters_typeof_exported_fn_works() {
    let diagnostics = check_multi_file(
        &[
            (
                "lib.ts",
                r#"export function Inner(props: { name: string; count: number }) {
    return null;
}

export function Widget(props: Parameters<typeof Inner>[0]) {
    return null;
}
"#,
            ),
            (
                "main.ts",
                r#"import { Widget } from "./lib";
Widget({ name: "hello", count: 42 });
"#,
            ),
        ],
        "main.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    );

    let ts2345: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "Parameters<typeof ExportedFn>[0]: no TS2345, got: {ts2345:#?}"
    );
}
