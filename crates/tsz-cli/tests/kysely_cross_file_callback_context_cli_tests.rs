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
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
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
/// combined stdout+stderr.
fn run_tsz_files(name: &str, files: &[(&str, &str)]) -> Option<String> {
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
    Some(text)
}

fn assert_lacks_codes(out: &str, codes: &[&str], context: &str) {
    for code in codes {
        assert!(
            !out.contains(code),
            "{context} (tsc 7.0.2 is clean); got {code}:\n{out}"
        );
    }
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
    assert_lacks_codes(
        &out,
        &["TS2339", "TS7006", "TS2347"],
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
    assert_lacks_codes(
        &out,
        &["TS2322"],
        "Kysely nested imported factory should keep literal node kinds",
    );
}
