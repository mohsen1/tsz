//! Cross-file Kysely callback-context rows, migrated from the in-process
//! `kysely_callback_context_tests` harness to the real `tsz` binary.
//!
//! These drive the real `tsz` binary because the callback context is inherited
//! through cross-file extension imports (`./kysely.ts`, `./query-creator.ts`,
//! `../util/object-utils.js`). The in-memory `check_multi_file_with_libs`
//! harness sets no `moduleResolution`/`allowImportingTsExtensions`, so those
//! imports resolve with reduced fidelity and the generic builder's callback
//! context is lost — a spurious `TS7006`/`TS2339`/`TS2322` appears in-harness
//! even though tsc 7.0.2 and the real binary are both clean. Verified standalone
//! against tsc 7.0.2 (both clean) before migration.

use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

struct TszOutput {
    status: ExitStatus,
    diagnostics: String,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_kysely_cross_file_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

/// Write `files` (relative path, contents) into a temp dir and run `tsz` over
/// them (in listed order) with strict cross-file bundler options, returning
/// process status and combined stdout+stderr.
fn run_tsz_files(name: &str, files: &[(&str, &str)]) -> Option<TszOutput> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let mut args: Vec<String> = Vec::new();
    for (rel, contents) in files {
        let path = temp.path.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&path, contents).expect("write fixture");
        args.push((*rel).to_string());
    }
    args.extend(
        [
            "--strict",
            "--noImplicitAny",
            "--moduleResolution",
            "bundler",
            "--module",
            "esnext",
            "--allowImportingTsExtensions",
            "--noEmit",
            "--pretty",
            "false",
        ]
        .map(String::from),
    );
    let output = Command::new(tsz_bin)
        .args(&args)
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(TszOutput {
        status: output.status,
        diagnostics: text,
    })
}

fn assert_clean_tsc_oracle(out: &TszOutput, context: &str) {
    assert!(
        out.status.success(),
        "{context} (tsc 7.0.2 is clean) must exit successfully:\n{}",
        out.diagnostics
    );
    assert!(
        out.diagnostics.trim().is_empty(),
        "{context} (tsc 7.0.2 is clean) must be diagnostic-free:\n{}",
        out.diagnostics
    );
}

const EXPRESSION_BUILDER: &str = r#"
export interface ExpressionBuilder<T> {
  ref<K extends keyof T & string>(key: K): T[K];
  call<T>(value: T): T;
}
"#;

const QUERY_CREATOR: &str = r#"
import type { ExpressionBuilder } from "./expression-builder.js";

export interface SelectQueryBuilder<T> {
  select(callback: (eb: ExpressionBuilder<T>) => ReadonlyArray<unknown>): void;
}

export class QueryCreator<DB> {
  readonly #props: { executor: unknown };

  constructor(props: { executor: unknown }) {
    this.#props = props;
  }

  selectFrom<K extends keyof DB & string>(table: K): SelectQueryBuilder<DB[K]> {
    this.#props.executor;
    return undefined as any;
  }
}
"#;

const KYSELY: &str = r#"
import { QueryCreator } from "./query-creator.ts";

export class Kysely<DB> extends QueryCreator<DB> {
  readonly #props: { executor: unknown };

  constructor(props: { executor: unknown }) {
    super(props);
    this.#props = props;
  }
}
"#;

/// A cross-file `Kysely<DB> extends QueryCreator<DB>` subclass must inherit the
/// generic builder's callback context, so `select((eb) => …)` types `eb`. tsc
/// 7.0.2 is clean; the real `tsz` binary must be too (no TS2339/TS7006/TS2347).
#[test]
fn cross_file_kysely_subclass_inherits_generic_builder_callback_context() {
    let main = r#"
type Database = {
  user: { id: number; name: string };
};

import type { Kysely } from "./kysely.ts";

declare const db: Kysely<Database>;

db.selectFrom("user").select((eb) => [
  eb.ref("id"),
  eb.call<number>(1),
]);
"#;
    let Some(out) = run_tsz_files(
        "subclass_callback_context",
        &[
            ("expression-builder.ts", EXPRESSION_BUILDER),
            ("query-creator.ts", QUERY_CREATOR),
            ("kysely.ts", KYSELY),
            ("main.ts", main),
        ],
    ) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert_clean_tsc_oracle(
        &out,
        "cross-file Kysely subclass should inherit the generic builder callback context",
    );
}

const OBJECT_UTILS: &str = r#"
export function freeze<T>(obj: T): Readonly<T> {
  return Object.freeze(obj);
}
"#;

const OPERATION_NODE: &str = r#"
export type OperationNodeKind =
  | "IdentifierNode"
  | "SchemableIdentifierNode";

export interface OperationNode {
  readonly kind: OperationNodeKind;
}
"#;

const IDENTIFIER_NODE: &str = r#"
import { freeze } from "../util/object-utils.js";
import type { OperationNode } from "./operation-node.js";

export interface IdentifierNode extends OperationNode {
  readonly kind: "IdentifierNode";
  readonly name: string;
}

type IdentifierNodeFactory = Readonly<{
  is(node: OperationNode): node is IdentifierNode;
  create(name: string): Readonly<IdentifierNode>;
}>;

export const IdentifierNode: IdentifierNodeFactory =
  freeze<IdentifierNodeFactory>({
    is(node): node is IdentifierNode {
      return node.kind === "IdentifierNode";
    },

    create(name) {
      return freeze({
        kind: "IdentifierNode",
        name,
      });
    },
  });
"#;

const SCHEMABLE_IDENTIFIER_NODE: &str = r#"
import { freeze } from "../util/object-utils.js";
import { IdentifierNode } from "./identifier-node.js";
import type { OperationNode } from "./operation-node.js";

export interface SchemableIdentifierNode extends OperationNode {
  readonly kind: "SchemableIdentifierNode";
  readonly schema?: IdentifierNode;
  readonly identifier: IdentifierNode;
}

type SchemableIdentifierNodeFactory = Readonly<{
  is(node: OperationNode): node is SchemableIdentifierNode;
  create(identifier: string): Readonly<SchemableIdentifierNode>;
  createWithSchema(schema: string, identifier: string): Readonly<SchemableIdentifierNode>;
}>;

export const SchemableIdentifierNode: SchemableIdentifierNodeFactory =
  freeze<SchemableIdentifierNodeFactory>({
    is(node): node is SchemableIdentifierNode {
      return node.kind === "SchemableIdentifierNode";
    },

    create(identifier) {
      return freeze({
        kind: "SchemableIdentifierNode",
        identifier: IdentifierNode.create(identifier),
      });
    },

    createWithSchema(schema, identifier) {
      return freeze({
        kind: "SchemableIdentifierNode",
        schema: IdentifierNode.create(schema),
        identifier: IdentifierNode.create(identifier),
      });
    },
  });
"#;

/// The Kysely schemable-identifier factory reaches nested imported literal node
/// kinds through `.js`-extension cross-file imports; the freeze/factory chain
/// must preserve the literal `kind` values (no widening → no TS2322). tsc 7.0.2
/// is clean; the real `tsz` binary must be too.
#[test]
fn kysely_schemable_identifier_factory_preserves_nested_imported_literal_kind() {
    let Some(out) = run_tsz_files(
        "schemable_identifier_factory",
        &[
            ("util/object-utils.ts", OBJECT_UTILS),
            ("operation-node/operation-node.ts", OPERATION_NODE),
            ("operation-node/identifier-node.ts", IDENTIFIER_NODE),
            (
                "operation-node/schemable-identifier-node.ts",
                SCHEMABLE_IDENTIFIER_NODE,
            ),
        ],
    ) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert_clean_tsc_oracle(
        &out,
        "Kysely nested imported factory should keep literal node kinds",
    );
}

const DATABASE_INTROSPECTOR: &str = r#"
export interface TableMetadata {
  readonly name: string;
  readonly isView: boolean;
  readonly columns: ColumnMetadata[];
  readonly schema?: string;
}

export interface ColumnMetadata {
  readonly name: string;
}
"#;

const MYSQL_INTROSPECTOR: &str = r#"
import type { TableMetadata } from "../database-introspector.js";
import { freeze } from "../../util/object-utils.js";

interface RawColumnMetadata {
  readonly tableName: string;
  readonly tableType: string;
  readonly columnName: string;
}

declare function findTable(
  tables: TableMetadata[],
  name: string,
): TableMetadata | undefined;

export function collect(columns: RawColumnMetadata[]): TableMetadata[] {
  return columns.reduce<TableMetadata[]>((tables, item) => {
    let table = findTable(tables, item.tableName);

    if (!table) {
      table = freeze({
        name: item.tableName,
        isView: item.tableType === "VIEW",
        schema: undefined,
        columns: [],
      });
      tables.push(table);
    }

    table.columns.push({ name: item.columnName });
    return tables;
  }, []);
}
"#;

/// A generic call assigned to an imported interface union inside a deferred
/// reducer callback is a killing definition when its instantiated return is
/// compatible with that interface. Both the callee and the declared assignment
/// surface cross file boundaries here, matching the `MySQL` introspector shape
/// that exposed the false TS18048. tsc 7.0.2 is clean.
#[test]
fn kysely_imported_generic_assignment_narrows_deferred_reducer_local() {
    let Some(out) = run_tsz_files(
        "imported_generic_assignment_flow",
        &[
            ("util/object-utils.ts", OBJECT_UTILS),
            ("dialect/database-introspector.ts", DATABASE_INTROSPECTOR),
            ("dialect/mysql/mysql-introspector.ts", MYSQL_INTROSPECTOR),
        ],
    ) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert_clean_tsc_oracle(
        &out,
        "an imported generic assignment compatible with an imported interface",
    );
}

/// The owner-keyed callable cache must distinguish equal binder-relative
/// terminal ids. The checker-level companion proves the ids collide; this
/// production CLI path proves diagnostics are independent of provider order.
#[test]
fn colliding_generic_terminal_owners_are_clean_across_provider_orders() {
    const LEFT: &str = r#"
export function retain<Value>(value: Value): Readonly<Value> {
  return value;
}
"#;
    const RIGHT: &str = r#"
export function enclose<Item>(value: Item): { payload: Item } {
  return { payload: value };
}
"#;
    const CONSUMER: &str = r#"
import { retain } from "./left.js";
import { enclose } from "./right.js";

interface LeftValue { readonly name: string; readonly values: string[]; }
interface RightValue { readonly payload: { readonly count: number }; }

export function deferred(leftValues: LeftValue[], rightValues: RightValue[]) {
  return (): void => {
    let left = leftValues.find((candidate) => candidate.name === "missing");
    let right = rightValues.find((candidate) => candidate.payload.count === -1);
    if (!left) left = retain({ name: "left", values: [] });
    if (!right) right = enclose({ count: 1 });
    left.values.push(left.name);
    right.payload.count.toFixed();
  };
}
"#;

    for (name, files) in [
        (
            "generic_terminal_owner_left_first",
            [
                ("left.ts", LEFT),
                ("right.ts", RIGHT),
                ("consumer.ts", CONSUMER),
            ],
        ),
        (
            "generic_terminal_owner_right_first",
            [
                ("right.ts", RIGHT),
                ("left.ts", LEFT),
                ("consumer.ts", CONSUMER),
            ],
        ),
    ] {
        let Some(out) = run_tsz_files(name, &files) else {
            println!("tsz binary not found; skipping");
            return;
        };
        assert_clean_tsc_oracle(
            &out,
            "colliding terminal owners must compile in either provider order",
        );
    }
}
