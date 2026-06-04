#[cfg(unix)]
#[test]
fn declaration_emit_symlinked_import_type_package_ref_stays_portable() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("Folder/monorepo/package-a/index.d.ts"),
        r#"export declare const styles: import("styled-components").InterpolationValue[];
"#,
    );
    write_file(
        &base.join("Folder/node_modules/styled-components/package.json"),
        r#"{
  "name": "styled-components",
  "version": "3.3.3",
  "typings": "typings/styled-components.d.ts"
}"#,
    );
    write_file(
        &base.join("Folder/node_modules/styled-components/typings/styled-components.d.ts"),
        r#"export interface InterpolationValue {}
"#,
    );
    write_file(
        &base.join("Folder/monorepo/core/index.ts"),
        r#"import { styles } from "package-a";

export function getStyles() {
    return styles;
}
"#,
    );
    std::fs::create_dir_all(base.join("Folder/monorepo/package-a/node_modules"))
        .expect("package-a node_modules");
    std::fs::create_dir_all(base.join("Folder/monorepo/core/node_modules"))
        .expect("core node_modules");
    std::os::unix::fs::symlink(
        base.join("Folder/node_modules/styled-components"),
        base.join("Folder/monorepo/package-a/node_modules/styled-components"),
    )
    .expect("styled-components symlink");
    std::os::unix::fs::symlink(
        base.join("Folder/monorepo/package-a"),
        base.join("Folder/monorepo/core/node_modules/package-a"),
    )
    .expect("package-a symlink");

    let args = CliArgs::try_parse_from([
        "tsz",
        "--declaration",
        "--ignoreConfig",
        "--alwaysStrict",
        "true",
        "--esModuleInterop",
        "--target",
        "es2015",
        "--module",
        "commonjs",
        "Folder/monorepo/package-a/index.d.ts",
        "Folder/node_modules/styled-components/typings/styled-components.d.ts",
        "Folder/monorepo/core/index.ts",
    ])
    .expect("CLI args should parse");

    let result = compile(&args, base).expect("compile should succeed");
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != 2883),
        "symlinked package import type should be portable, got: {:#?}",
        result.diagnostics
    );
    let dts =
        fs::read_to_string(base.join("Folder/monorepo/core/index.d.ts")).expect("read core d.ts");
    assert!(
        dts.contains(
            r#"export declare function getStyles(): import("styled-components").InterpolationValue[];"#
        ),
        "expected bare package import type in declaration emit, got: {dts}"
    );
}

#[cfg(unix)]
#[test]
fn declaration_emit_symlinked_import_type_package_ref_reports_when_dependency_is_resolution_only() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("Folder/monorepo/package-a/index.d.ts"),
        r#"export declare const styles: import("styled-components").InterpolationValue[];
"#,
    );
    write_file(
        &base.join("Folder/node_modules/styled-components/package.json"),
        r#"{
  "name": "styled-components",
  "version": "3.3.3",
  "typings": "typings/styled-components.d.ts"
}"#,
    );
    write_file(
        &base.join("Folder/node_modules/styled-components/typings/styled-components.d.ts"),
        r#"export interface InterpolationValue {}
"#,
    );
    write_file(
        &base.join("Folder/monorepo/core/index.ts"),
        r#"import { styles } from "package-a";

export function getStyles() {
    return styles;
}
"#,
    );
    std::fs::create_dir_all(base.join("Folder/monorepo/package-a/node_modules"))
        .expect("package-a node_modules");
    std::fs::create_dir_all(base.join("Folder/monorepo/core/node_modules"))
        .expect("core node_modules");
    std::os::unix::fs::symlink(
        base.join("Folder/node_modules/styled-components"),
        base.join("Folder/monorepo/package-a/node_modules/styled-components"),
    )
    .expect("styled-components symlink");
    std::os::unix::fs::symlink(
        base.join("Folder/monorepo/package-a"),
        base.join("Folder/monorepo/core/node_modules/package-a"),
    )
    .expect("package-a symlink");

    let mut args = default_args();
    args.project = Some(base.join("Folder/monorepo/core/tsconfig.json"));
    write_file(
        &base.join("Folder/monorepo/core/tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "strict": true,
    "declaration": true
  },
  "files": ["index.ts"]
}"#,
    );

    let result = compile(&args, base).expect("compile should succeed");
    let ts2883_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts2883_messages.len(),
        1,
        "expected one TS2883 diagnostic for resolution-only symlinked import type, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages[0].contains("InterpolationValue"),
        "expected TS2883 to name imported member, got: {}",
        ts2883_messages[0]
    );
    assert!(
        ts2883_messages[0]
            .contains("../package-a/node_modules/styled-components/typings/styled-components"),
        "expected TS2883 to preserve source-package node_modules path, got: {}",
        ts2883_messages[0]
    );
}

#[test]
fn declaration_emit_commonjs_call_tuple_reports_nested_reference_ts2883() {
    let temp = TempDir::new().expect("temp dir");
    let base = temp.path.as_path();

    write_file(
        &base.join("r/node_modules/foo/node_modules/nested/index.d.ts"),
        "export interface NestedProps {}\n",
    );
    write_file(
        &base.join("r/node_modules/foo/other/index.d.ts"),
        "export interface OtherIndexProps {}\n",
    );
    write_file(
        &base.join("r/node_modules/foo/other.d.ts"),
        "export interface OtherProps {}\n",
    );
    write_file(
        &base.join("r/node_modules/foo/index.d.ts"),
        r#"import { OtherProps } from "./other";
import { OtherIndexProps } from "./other/index";
import { NestedProps } from "nested";
export interface SomeProps {}

export function foo(): [SomeProps, OtherProps, OtherIndexProps, NestedProps];
"#,
    );
    write_file(
        &base.join("node_modules/root/index.d.ts"),
        r#"export interface RootProps {}

export function bar(): RootProps;
"#,
    );
    write_file(
        &base.join("r/entry.ts"),
        r#"import { foo } from "foo";
import { bar } from "root";
export const x = foo();
export const y = bar();
"#,
    );
    write_file(
        &base.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2015",
    "module": "commonjs",
    "declaration": true
  },
  "files": ["r/entry.ts"]
}"#,
    );

    let mut args = default_args();
    args.project = Some(base.join("tsconfig.json"));

    let result = compile(&args, base).expect("compile should succeed");
    let ts2883_messages: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == 2883)
        .map(|d| d.message_text.clone())
        .collect();

    assert_eq!(
        ts2883_messages.len(),
        1,
        "expected one TS2883 diagnostic for nested tuple return reference, got: {ts2883_messages:#?}"
    );
    assert!(
        ts2883_messages[0].contains("NestedProps"),
        "expected TS2883 to name NestedProps, got: {}",
        ts2883_messages[0]
    );
    assert!(
        ts2883_messages[0].contains("foo/node_modules/nested"),
        "expected TS2883 to reference foo/node_modules/nested, got: {}",
        ts2883_messages[0]
    );
}
