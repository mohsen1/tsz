use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_with_libs, diagnostic_line_column,
    load_default_lib_files,
};

fn strict_default_lib_diagnostics(source: &str) -> Vec<Diagnostic> {
    let lib_files = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .collect()
}

fn lacks_any_diagnostic_code(diagnostics: &[Diagnostic], codes: &[u32]) -> bool {
    !diagnostics
        .iter()
        .any(|actual| codes.contains(&actual.code))
}

fn format_diagnostics(source: &str, diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let (line, column) = diagnostic_line_column(source, diagnostic);
            format!(
                "TS{} at {line}:{column}: {}",
                diagnostic.code, diagnostic.message_text
            )
        })
        .collect()
}

#[test]
fn imported_select_callback_alias_contextually_types_array_expression_factory() {
    let lib_files = load_default_lib_files();
    let options = CheckerOptions {
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let files = [
        (
            "expression-builder.ts",
            r#"
export interface ExpressionBuilder<T> {
  ref<K extends keyof T & string>(key: K): T[K];
}
"#,
        ),
        (
            "select-parser.ts",
            r#"
import type { ExpressionBuilder } from "./expression-builder.js";

export type SelectExpression<T> =
  | keyof T & string
  | ((eb: ExpressionBuilder<T>) => unknown);

export type SelectCallback<T> = (
  eb: ExpressionBuilder<T>,
) => ReadonlyArray<SelectExpression<T>>;
"#,
        ),
        (
            "query-builder.ts",
            r#"
import type { SelectCallback, SelectExpression } from "./select-parser.js";

export interface Builder<T> {
  select<SE extends SelectExpression<T>>(selections: ReadonlyArray<SE>): void;
  select<CB extends SelectCallback<T>>(callback: CB): void;
}
"#,
        ),
        (
            "main.ts",
            r#"
import type { Builder } from "./query-builder.js";

declare const builder: Builder<{ kind: "table" }>;

builder.select([
  "kind",
  (eb) => eb.ref("kind"),
]);
"#,
        ),
    ];

    let diagnostics = check_multi_file_with_libs(&files, "main.ts", options, &lib_files)
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .collect::<Vec<_>>();

    assert!(
        lacks_any_diagnostic_code(&diagnostics, &[7006, 2347, 2693]),
        "imported callback aliases should contextually type selection factories. Got: {diagnostics:#?}"
    );
}

#[test]
fn imported_conditional_builder_alias_materializes_callback_context() {
    let lib_files = load_default_lib_files();
    let options = CheckerOptions {
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let files = [
        (
            "expression-builder.ts",
            r#"
export interface ExpressionBuilder<Row> {
  ref<K extends keyof Row & string>(key: K): Row[K];
  call<T>(value: T): T;
}
"#,
        ),
        (
            "query-builder.ts",
            r#"
import type { ExpressionBuilder } from "./expression-builder.js";

export type SelectionCallback<Schema, Table extends keyof Schema> = (
  builder: ExpressionBuilder<Schema[Table]>,
) => ReadonlyArray<unknown>;

export interface ResultBuilder<Schema, Table extends keyof Schema, Output> {
  select<Callback extends SelectionCallback<Schema, Table>>(
    callback: Callback,
  ): ResultBuilder<Schema, Table, Output>;
}
"#,
        ),
        (
            "table-parser.ts",
            r#"
export type TableExpressionOrList<Schema, Table extends keyof Schema> =
  | keyof Schema
  | `${keyof Schema & string} as ${string}`;

export type ExtractTableAlias<Schema, Expression> =
  Expression extends `${infer Source} as ${infer Alias}`
    ? Source extends keyof Schema
      ? Alias
      : never
    : Expression extends keyof Schema
      ? Expression
      : never;
"#,
        ),
        (
            "select-from-parser.ts",
            r#"
import type { ResultBuilder } from "./query-builder.js";
import type { ExtractTableAlias, TableExpressionOrList } from "./table-parser.js";

export type SelectFrom<
  Schema,
  Table extends keyof Schema,
  Expression extends TableExpressionOrList<Schema, Table>,
> = [Expression] extends [keyof Schema]
  ? ResultBuilder<Schema, Table | ExtractTableAlias<Schema, Expression>, {}>
  : [Expression] extends [`${infer Source} as ${infer Alias}`]
    ? Source extends keyof Schema
      ? ResultBuilder<Schema & { [K in Alias & string]: Schema[Source] }, Table | Alias, {}>
      : never
    : never;
"#,
        ),
        (
            "main.ts",
            r#"
import type { SelectFrom } from "./select-from-parser.js";

declare const query: SelectFrom<{ account: { id: number } }, never, "account">;

query.select((reader) => [
  reader.ref("id"),
  reader.call<number>(1),
]);
"#,
        ),
    ];

    let diagnostics = check_multi_file_with_libs(&files, "main.ts", options, &lib_files)
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .collect::<Vec<_>>();

    assert!(
        lacks_any_diagnostic_code(&diagnostics, &[2339, 2347, 7006]),
        "imported conditional builder aliases should materialize callback context. Got: {diagnostics:#?}"
    );
}

#[test]
fn imported_conditional_builder_alias_uses_declaring_file_for_duplicate_helper_names() {
    let lib_files = load_default_lib_files();
    let options = CheckerOptions {
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let files = [
        (
            "unrelated-result-builder.ts",
            r#"
export interface ResultBuilder<Schema, Table, Output> {
  poison: Output;
}
"#,
        ),
        (
            "expression-builder.ts",
            r#"
export interface ExpressionBuilder<Row> {
  ref<K extends keyof Row & string>(key: K): Row[K];
  call<T>(value: T): T;
}
"#,
        ),
        (
            "query-builder.ts",
            r#"
import type { ExpressionBuilder } from "./expression-builder.js";

export type SelectionCallback<Schema, Table extends keyof Schema> = (
  builder: ExpressionBuilder<Schema[Table]>,
) => ReadonlyArray<unknown>;

export interface ResultBuilder<Schema, Table extends keyof Schema, Output> {
  select<Callback extends SelectionCallback<Schema, Table>>(
    callback: Callback,
  ): ResultBuilder<Schema, Table, Output>;
}
"#,
        ),
        (
            "table-parser.ts",
            r#"
export type TableExpressionOrList<Schema, Table extends keyof Schema> =
  | keyof Schema
  | `${keyof Schema & string} as ${string}`;

export type ExtractTableAlias<Schema, Expression> =
  Expression extends `${infer Source} as ${infer Alias}`
    ? Source extends keyof Schema
      ? Alias
      : never
    : Expression extends keyof Schema
      ? Expression
      : never;
"#,
        ),
        (
            "select-from-parser.ts",
            r#"
import type { ResultBuilder } from "./query-builder.js";
import type { ExtractTableAlias, TableExpressionOrList } from "./table-parser.js";

export type SelectFrom<
  Schema,
  Table extends keyof Schema,
  Expression extends TableExpressionOrList<Schema, Table>,
> = [Expression] extends [keyof Schema]
  ? ResultBuilder<Schema, Table | ExtractTableAlias<Schema, Expression>, {}>
  : never;
"#,
        ),
        (
            "main.ts",
            r#"
import type { SelectFrom } from "./select-from-parser.js";

declare const query: SelectFrom<{ account: { id: number } }, never, "account">;

query.select((reader) => [
  reader.ref("id"),
  reader.call<number>(1),
]);
"#,
        ),
    ];

    let diagnostics = check_multi_file_with_libs(&files, "main.ts", options, &lib_files)
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .collect::<Vec<_>>();

    assert!(
        lacks_any_diagnostic_code(&diagnostics, &[2339, 2347, 7006]),
        "imported conditional builder aliases should resolve helper names from their declaring file. Got: {diagnostics:#?}"
    );
}

#[test]
fn kysely_join_if_chain_preserves_select_callback_context() {
    let source = r#"
type SelectType<T> = T;
type DrainOuterGeneric<T> = [T] extends [unknown] ? T : never;
type Nullable<T> = { [K in keyof T]: T[K] | null };
type ShallowRecord<K extends keyof any, T> = DrainOuterGeneric<{ [P in K]: T }>;

type AnyColumn<DB, TB extends keyof DB> = {
  [T in TB]: keyof DB[T]
}[TB] & string;

type ExtractColumnType<DB, TB extends keyof DB, C> = {
  [T in TB]: C extends keyof DB[T] ? DB[T][C] : never
}[TB];

type AnyColumnWithTable<DB, TB extends keyof DB> = {
  [T in TB]: `${T & string}.${keyof DB[T] & string}`
}[TB];

type AnyAliasedColumn<DB, TB extends keyof DB> =
  `${AnyColumn<DB, TB>} as ${string}`;

type AnyAliasedColumnWithTable<DB, TB extends keyof DB> =
  `${AnyColumnWithTable<DB, TB>} as ${string}`;

interface AliasedExpression<T, A extends string> {
  expressionType?: T;
  alias: A;
}

interface ExpressionWrapper<DB, TB extends keyof DB, T> {
  $castTo<C>(): ExpressionWrapper<DB, TB, C>;
  as<A extends string>(alias: A): AliasedExpression<T, A>;
}

interface ExpressionBuilder<DB, TB extends keyof DB> {
  ref<RE extends StringReference<DB, TB>>(
    reference: RE,
  ): ExpressionWrapper<DB, TB, ExtractTypeFromReferenceExpression<DB, TB, RE>>;
}

type StringReference<DB, TB extends keyof DB> =
  | AnyColumn<DB, TB>
  | AnyColumnWithTable<DB, TB>;

type ExtractTypeFromReferenceExpression<DB, TB extends keyof DB, RE> =
  SelectType<ExtractTypeFromStringReference<DB, TB, RE>>;

type ExtractTypeFromStringReference<DB, TB extends keyof DB, RE> =
  RE extends `${infer T}.${infer C}`
    ? T extends TB
      ? C extends keyof DB[T]
        ? DB[T][C]
        : never
      : never
    : RE extends AnyColumn<DB, TB>
      ? ExtractColumnType<DB, TB, RE>
      : unknown;

type AliasedExpressionFactory<DB, TB extends keyof DB> = (
  eb: ExpressionBuilder<DB, TB>,
) => AliasedExpression<any, any>;

type AliasedExpressionOrFactory<DB, TB extends keyof DB> =
  | AliasedExpression<any, any>
  | AliasedExpressionFactory<DB, TB>;

type AnyAliasedTable<DB> = `${keyof DB & string} as ${string}`;
type TableExpression<DB, TB extends keyof DB> =
  | keyof DB & string
  | AnyAliasedTable<DB>
  | AliasedExpressionOrFactory<DB, TB>;

type TableExpressionOrList<DB, TB extends keyof DB> =
  | TableExpression<DB, TB>
  | ReadonlyArray<TableExpression<DB, TB>>;

type ExtractAliasFromTableExpression<DB, TE> = TE extends string
  ? TE extends `${string} as ${infer TA}`
    ? TA
    : TE extends keyof DB
      ? TE
      : never
  : TE extends AliasedExpression<any, infer QA>
    ? QA
    : TE extends (qb: any) => AliasedExpression<any, infer QA>
      ? QA
      : never;

type ExtractRowTypeFromTableExpression<DB, TE, A extends keyof any> =
  TE extends `${infer T} as ${infer TA}`
    ? TA extends A
      ? T extends keyof DB
        ? DB[T]
        : never
      : never
    : TE extends A
      ? TE extends keyof DB
        ? DB[TE]
        : never
      : never;

type From<DB, TE> = DrainOuterGeneric<{
  [C in keyof DB | ExtractAliasFromTableExpression<DB, TE>]:
    C extends ExtractAliasFromTableExpression<DB, TE>
      ? ExtractRowTypeFromTableExpression<DB, TE, C>
      : C extends keyof DB
        ? DB[C]
        : never
}>;

type FromTables<DB, TB extends keyof DB, TE> =
  TB | ExtractAliasFromTableExpression<DB, TE>;

type SelectFrom<DB, TB extends keyof DB, TE> =
  TE extends `${infer T} as ${infer A}`
    ? T extends keyof DB
      ? SelectQueryBuilder<DB & ShallowRecord<A, DB[T]>, TB | A, {}>
      : never
    : never;

type SelectExpression<DB, TB extends keyof DB> =
  | AnyAliasedColumnWithTable<DB, TB>
  | AnyAliasedColumn<DB, TB>
  | AnyColumnWithTable<DB, TB>
  | AnyColumn<DB, TB>
  | AliasedExpressionOrFactory<DB, TB>;

type ExtractAliasFromSelectExpression<SE> = SE extends string
  ? SE extends `${string}.${infer C} as ${infer A}`
    ? A
    : SE extends `${string}.${infer C}`
      ? C
      : SE
  : SE extends AliasedExpression<any, infer EA>
    ? EA
    : SE extends (qb: any) => AliasedExpression<any, infer EA>
      ? EA
      : never;

type ExtractTypeFromSelectExpression<DB, TB extends keyof DB, SE> =
  SE extends string
    ? ExtractTypeFromStringSelectExpression<DB, TB, SE>
    : SE extends (eb: any) => AliasedExpression<infer O, any>
      ? O
      : SE extends AliasedExpression<infer O, any>
        ? O
        : never;

type ExtractTypeFromStringSelectExpression<DB, TB extends keyof DB, SE> =
  SE extends `${infer T}.${infer C} as ${string}`
    ? T extends TB
      ? C extends keyof DB[T]
        ? DB[T][C]
        : never
      : never
    : SE extends `${infer C} as ${string}`
      ? C extends AnyColumn<DB, TB>
        ? ExtractColumnType<DB, TB, C>
        : never
      : SE extends `${infer T}.${infer C}`
        ? T extends TB
          ? C extends keyof DB[T]
            ? DB[T][C]
            : never
          : never
        : SE extends AnyColumn<DB, TB>
          ? ExtractColumnType<DB, TB, SE>
          : never;

type KyselySelection<DB, TB extends keyof DB, SE> = {
  [E in SE as ExtractAliasFromSelectExpression<E>]:
    SelectType<ExtractTypeFromSelectExpression<DB, TB, E>>
};

type CallbackSelection<DB, TB extends keyof DB, CB> =
  CB extends (eb: any) => ReadonlyArray<infer SE>
    ? KyselySelection<DB, TB, SE>
    : never;

interface JoinBuilder<DB, TB extends keyof DB> {
  onRef(lhs: AnyColumnWithTable<DB, TB>, op: string, rhs: AnyColumnWithTable<DB, TB>): JoinBuilder<DB, TB>;
  on(lhs: AnyColumnWithTable<DB, TB>, op: string, rhs: unknown): JoinBuilder<DB, TB>;
}

type JoinCallback<DB, TB extends keyof DB, TE> = (
  join: JoinBuilder<From<DB, TE>, FromTables<DB, TB, TE>>,
) => JoinBuilder<From<DB, TE>, FromTables<DB, TB, TE>>;

interface SelectQueryBuilder<DB, TB extends keyof DB, O> {
  leftJoin<TE extends TableExpression<DB, TB>>(
    table: TE,
    callback: JoinCallback<DB, TB, TE>,
  ): SelectQueryBuilderWithLeftJoin<DB, TB, O, TE>;
  $if<O2>(
    condition: boolean,
    callback: (qb: this) => SelectQueryBuilder<any, any, O & O2>,
  ): SelectQueryBuilder<DB, TB, O & Partial<Omit<O2, keyof O>>>;
  where(lhs: AnyColumnWithTable<DB, TB>, op: string, rhs: unknown): this;
  select<SE extends SelectExpression<DB, TB>>(
    selections: ReadonlyArray<SE>,
  ): SelectQueryBuilder<DB, TB, O & KyselySelection<DB, TB, SE>>;
  select<CB extends (eb: ExpressionBuilder<DB, TB>) => ReadonlyArray<SelectExpression<DB, TB>>>(
    callback: CB,
  ): SelectQueryBuilder<DB, TB, O & CallbackSelection<DB, TB, CB>>;
}

type SelectQueryBuilderWithLeftJoin<DB, TB extends keyof DB, O, TE> =
  TE extends `${infer T} as ${infer A}`
    ? T extends keyof DB
      ? SelectQueryBuilder<DB & ShallowRecord<A, Nullable<DB[T]>>, TB | A, O>
      : never
    : never;

declare class QueryCreator<DB> {
  selectFrom<TE extends TableExpressionOrList<DB, never>>(
    from: TE,
  ): SelectFrom<DB, never, TE>;
}

type MssqlSysTables = {
  "sys.tables": { name: string; object_id: number; schema_id: number; type: "U" };
  "sys.schemas": { name: string; schema_id: number };
  "sys.columns": { name: string; object_id: number; user_type_id: number };
  "sys.types": { name: string; user_type_id: number };
  "sys.extended_properties": { major_id: number; minor_id: number; name: string };
};

declare const db: QueryCreator<MssqlSysTables>;
declare const withInternalKyselyTables: boolean;

db.selectFrom("sys.tables as tables")
  .leftJoin("sys.extended_properties as comments", (join) =>
    join
      .onRef("comments.major_id", "=", "tables.object_id")
      .on("comments.name", "=", "MS_Description"),
  )
  .$if(!withInternalKyselyTables, (qb) =>
    qb.where("tables.name", "!=", "kysely_migration"),
  )
  .select([
    "tables.name as table_name",
    (eb) =>
      eb
        .ref("tables.type")
        .$castTo<MssqlSysTables["sys.tables"]["type"]>()
        .as("table_type"),
  ]);
"#;

    let diagnostics = strict_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diagnostics, &[7006, 2347]),
        "Kysely join/$if/select callback chain should keep callback context. Got: {:#?}",
        format_diagnostics(source, &diagnostics)
    );
}

#[test]
fn kysely_union_all_chain_preserves_nested_select_callback_context_after_relation_fallout() {
    let source = r#"
type SelectType<T> = T;
type DrainOuterGeneric<T> = [T] extends [unknown] ? T : never;
type Nullable<T> = { [K in keyof T]: T[K] | null };
type ShallowRecord<K extends keyof any, T> = DrainOuterGeneric<{ [P in K]: T }>;

type AnyColumn<DB, TB extends keyof DB> = {
  [T in TB]: keyof DB[T]
}[TB] & string;

type ExtractColumnType<DB, TB extends keyof DB, C> = {
  [T in TB]: C extends keyof DB[T] ? DB[T][C] : never
}[TB];

type AnyColumnWithTable<DB, TB extends keyof DB> = {
  [T in TB]: `${T & string}.${keyof DB[T] & string}`
}[TB];

type AnyAliasedColumn<DB, TB extends keyof DB> =
  `${AnyColumn<DB, TB>} as ${string}`;

type AnyAliasedColumnWithTable<DB, TB extends keyof DB> =
  `${AnyColumnWithTable<DB, TB>} as ${string}`;

interface AliasedExpression<T, A extends string> {
  expressionType?: T;
  alias: A;
}

interface ExpressionWrapper<DB, TB extends keyof DB, T> {
  $castTo<C>(): ExpressionWrapper<DB, TB, C>;
  as<A extends string>(alias: A): AliasedExpression<T, A>;
}

interface ExpressionBuilder<DB, TB extends keyof DB> {
  ref<RE extends StringReference<DB, TB>>(
    reference: RE,
  ): ExpressionWrapper<DB, TB, ExtractTypeFromReferenceExpression<DB, TB, RE>>;
}

type StringReference<DB, TB extends keyof DB> =
  | AnyColumn<DB, TB>
  | AnyColumnWithTable<DB, TB>;

type ExtractTypeFromReferenceExpression<DB, TB extends keyof DB, RE> =
  SelectType<ExtractTypeFromStringReference<DB, TB, RE>>;

type ExtractTypeFromStringReference<DB, TB extends keyof DB, RE> =
  RE extends `${infer T}.${infer C}`
    ? T extends TB
      ? C extends keyof DB[T]
        ? DB[T][C]
        : never
      : never
    : RE extends AnyColumn<DB, TB>
      ? ExtractColumnType<DB, TB, RE>
      : unknown;

type AliasedExpressionFactory<DB, TB extends keyof DB> = (
  eb: ExpressionBuilder<DB, TB>,
) => AliasedExpression<any, any>;

type AliasedExpressionOrFactory<DB, TB extends keyof DB> =
  | AliasedExpression<any, any>
  | AliasedExpressionFactory<DB, TB>;

type AnyAliasedTable<DB> = `${keyof DB & string} as ${string}`;
type TableExpression<DB, TB extends keyof DB> =
  | keyof DB & string
  | AnyAliasedTable<DB>
  | AliasedExpressionOrFactory<DB, TB>;

type ExtractAliasFromTableExpression<DB, TE> = TE extends string
  ? TE extends `${string} as ${infer TA}`
    ? TA
    : TE extends keyof DB
      ? TE
      : never
  : TE extends AliasedExpression<any, infer QA>
    ? QA
    : TE extends (qb: any) => AliasedExpression<any, infer QA>
      ? QA
      : never;

type FromTables<DB, TB extends keyof DB, TE> =
  TB | ExtractAliasFromTableExpression<DB, TE>;

type SelectFrom<DB, TB extends keyof DB, TE> =
  TE extends `${infer T} as ${infer A}`
    ? T extends keyof DB
      ? SelectQueryBuilder<DB & ShallowRecord<A, DB[T]>, TB | A, {}>
      : never
    : never;

type SelectExpression<DB, TB extends keyof DB> =
  | AnyAliasedColumnWithTable<DB, TB>
  | AnyAliasedColumn<DB, TB>
  | AnyColumnWithTable<DB, TB>
  | AnyColumn<DB, TB>
  | AliasedExpressionOrFactory<DB, TB>;

type ExtractAliasFromSelectExpression<SE> = SE extends string
  ? SE extends `${string}.${infer C} as ${infer A}`
    ? A
    : SE extends `${string}.${infer C}`
      ? C
      : SE
  : SE extends AliasedExpression<any, infer EA>
    ? EA
    : SE extends (qb: any) => AliasedExpression<any, infer EA>
      ? EA
      : never;

type ExtractTypeFromSelectExpression<DB, TB extends keyof DB, SE> =
  SE extends string
    ? ExtractTypeFromStringSelectExpression<DB, TB, SE>
    : SE extends (eb: any) => AliasedExpression<infer O, any>
      ? O
      : SE extends AliasedExpression<infer O, any>
        ? O
        : never;

type ExtractTypeFromStringSelectExpression<DB, TB extends keyof DB, SE> =
  SE extends `${infer T}.${infer C} as ${string}`
    ? T extends TB
      ? C extends keyof DB[T]
        ? DB[T][C]
        : never
      : never
    : SE extends `${infer C} as ${string}`
      ? C extends AnyColumn<DB, TB>
        ? ExtractColumnType<DB, TB, C>
        : never
      : SE extends `${infer T}.${infer C}`
        ? T extends TB
          ? C extends keyof DB[T]
            ? DB[T][C]
            : never
          : never
        : SE extends AnyColumn<DB, TB>
          ? ExtractColumnType<DB, TB, SE>
          : never;

type KyselySelection<DB, TB extends keyof DB, SE> = {
  [E in SE as ExtractAliasFromSelectExpression<E>]:
    SelectType<ExtractTypeFromSelectExpression<DB, TB, E>>
};

type CallbackSelection<DB, TB extends keyof DB, CB> =
  CB extends (eb: any) => ReadonlyArray<infer SE>
    ? KyselySelection<DB, TB, SE>
    : never;

interface JoinBuilder<DB, TB extends keyof DB> {
  onRef(lhs: AnyColumnWithTable<DB, TB>, op: string, rhs: AnyColumnWithTable<DB, TB>): JoinBuilder<DB, TB>;
  on(lhs: AnyColumnWithTable<DB, TB>, op: string, rhs: unknown): JoinBuilder<DB, TB>;
}

type JoinCallback<DB, TB extends keyof DB, TE> = (
  join: JoinBuilder<DB, FromTables<DB, TB, TE>>,
) => JoinBuilder<DB, FromTables<DB, TB, TE>>;

interface SelectQueryBuilder<DB, TB extends keyof DB, O> {
  leftJoin<TE extends TableExpression<DB, TB>>(
    table: TE,
    callback: JoinCallback<DB, TB, TE>,
  ): SelectQueryBuilder<DB, FromTables<DB, TB, TE>, O>;
  $if<O2>(
    condition: boolean,
    callback: (qb: this) => SelectQueryBuilder<any, any, O & O2>,
  ): SelectQueryBuilder<DB, TB, O & Partial<Omit<O2, keyof O>>>;
  where(lhs: AnyColumnWithTable<DB, TB>, op: string, rhs: unknown): this;
  select<SE extends SelectExpression<DB, TB>>(
    selections: ReadonlyArray<SE>,
  ): SelectQueryBuilder<DB, TB, O & KyselySelection<DB, TB, SE>>;
  unionAll<E extends SelectQueryBuilder<any, any, O>>(
    expression: E,
  ): SelectQueryBuilder<DB, TB, O>;
}

declare class QueryCreator<DB> {
  selectFrom<TE extends TableExpression<DB, never>>(
    from: TE,
  ): SelectFrom<DB, never, TE>;
}

type MssqlSysTables = {
  "sys.tables": { name: string; object_id: number; schema_id: number; type: "U" };
  "sys.views": { name: string; object_id: number; schema_id: number; type: "V" };
  "sys.schemas": { name: string; schema_id: number };
  "sys.columns": { name: string; object_id: number; column_id: number; user_type_id: number };
  "sys.types": { name: string; user_type_id: number; schema_id: number };
  "sys.extended_properties": { major_id: number; minor_id: number; name: string };
};

declare const anyDb: QueryCreator<any>;
declare const withInternalKyselyTables: boolean;

class Introspector {
  readonly #db: QueryCreator<MssqlSysTables>;

  constructor(db: QueryCreator<any>) {
    this.#db = db;
  }

  tables() {
    return this.#db
      .selectFrom("sys.tables as tables")
      .leftJoin("sys.extended_properties as comments", (join) =>
        join
          .onRef("comments.major_id", "=", "tables.object_id")
          .on("comments.name", "=", "MS_Description"),
      )
      .$if(!withInternalKyselyTables, (qb) =>
        qb.where("tables.name", "!=", "kysely_migration"),
      )
      .select([
        "tables.name as table_name",
        (eb) =>
          eb
            .ref("tables.type")
            .$castTo<MssqlSysTables["sys.tables"]["type"]>()
            .as("table_type"),
      ])
      .unionAll(
        this.#db
          .selectFrom("sys.views as views")
          .leftJoin("sys.extended_properties as comments", (join) =>
            join
              .onRef("comments.major_id", "=", "views.object_id")
              .on("comments.name", "=", "MS_Description"),
          )
          .select([
            "views.name as table_name",
            (eb) =>
              eb
                .ref("views.type")
                .$castTo<MssqlSysTables["sys.views"]["type"]>()
                .as("table_type"),
          ]),
      );
  }
}

new Introspector(anyDb).tables();
"#;

    let diagnostics = strict_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diagnostics, &[7006, 2347]),
        "Kysely unionAll chain should keep nested callback context despite unrelated relation fallout. Got: {:#?}",
        format_diagnostics(source, &diagnostics)
    );
}

#[test]
fn kysely_freeze_factory_method_preserves_return_literal_kind() {
    let source = r#"
declare function freeze<T>(obj: T): Readonly<T>;

interface OperationNode {
  readonly kind: string;
}

interface ColumnDefinitionNode extends OperationNode {
  readonly kind: "ColumnDefinitionNode";
  readonly column: string;
  readonly frontModifiers?: ReadonlyArray<OperationNode>;
  readonly endModifiers?: ReadonlyArray<OperationNode>;
}

type ColumnDefinitionNodeFactory = Readonly<{
  create(column: string): Readonly<ColumnDefinitionNode>;
  cloneWithFrontModifier(
    node: ColumnDefinitionNode,
    modifier: OperationNode,
  ): Readonly<ColumnDefinitionNode>;
  cloneWithEndModifier(
    node: ColumnDefinitionNode,
    modifier: OperationNode,
  ): Readonly<ColumnDefinitionNode>;
  cloneWith(
    node: ColumnDefinitionNode,
    props: Partial<ColumnDefinitionNode>,
  ): Readonly<ColumnDefinitionNode>;
}>;

const ColumnDefinitionNode: ColumnDefinitionNodeFactory =
  freeze<ColumnDefinitionNodeFactory>({
    create(column) {
      return freeze({
        kind: "ColumnDefinitionNode",
        column,
      });
    },

    cloneWithFrontModifier(node, modifier) {
      return freeze({
        ...node,
        frontModifiers: node.frontModifiers
          ? freeze([...node.frontModifiers, modifier])
          : [modifier],
      });
    },

    cloneWithEndModifier(node, modifier) {
      return freeze({
        ...node,
        endModifiers: node.endModifiers
          ? freeze([...node.endModifiers, modifier])
          : [modifier],
      });
    },

    cloneWith(node, props) {
      return freeze({
        ...node,
        ...props,
      });
    },
  });
"#;

    let diagnostics = strict_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diagnostics, &[2322]),
        "Kysely freeze factory method should preserve contextual literal return. Got: {:#?}",
        format_diagnostics(source, &diagnostics)
    );
}

#[test]
fn kysely_imported_freeze_factory_method_preserves_return_literal_kind() {
    let lib_files = load_default_lib_files();
    let options = CheckerOptions {
        strict: true,
        no_implicit_any: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let files = [
        (
            "util/object-utils.ts",
            r#"
export function freeze<T>(obj: T): Readonly<T> {
  return Object.freeze(obj);
}

export function isBuffer(obj: unknown): obj is { length: number } {
  return typeof Buffer !== "undefined" && Buffer.isBuffer(obj);
}
"#,
        ),
        (
            "operation-node/operation-node.ts",
            r#"
export type OperationNodeKind =
  | "ColumnNode"
  | "ColumnDefinitionNode";

export interface OperationNode {
  readonly kind: OperationNodeKind;
}
"#,
        ),
        (
            "operation-node/column-node.ts",
            r#"
import { freeze } from "../util/object-utils.js";
import type { OperationNode } from "./operation-node.js";

export interface ColumnNode extends OperationNode {
  readonly kind: "ColumnNode";
  readonly column: string;
}

type ColumnNodeFactory = Readonly<{
  create(column: string): Readonly<ColumnNode>;
}>;

export const ColumnNode: ColumnNodeFactory = freeze<ColumnNodeFactory>({
  create(column) {
    return freeze({
      kind: "ColumnNode",
      column,
    });
  },
});
"#,
        ),
        (
            "operation-node/column-definition-node.ts",
            r#"
import { freeze } from "../util/object-utils.js";
import { ColumnNode } from "./column-node.js";
import type { OperationNode } from "./operation-node.js";

export interface ColumnDefinitionNode extends OperationNode {
  readonly kind: "ColumnDefinitionNode";
  readonly column: ColumnNode;
  readonly dataType: OperationNode;
}

type ColumnDefinitionNodeFactory = Readonly<{
  create(column: string, dataType: OperationNode): Readonly<ColumnDefinitionNode>;
}>;

export const ColumnDefinitionNode: ColumnDefinitionNodeFactory =
  freeze<ColumnDefinitionNodeFactory>({
    create(column, dataType) {
      return freeze({
        kind: "ColumnDefinitionNode",
        column: ColumnNode.create(column),
        dataType,
      });
    },
  });
"#,
        ),
    ];

    let diagnostics = check_multi_file_with_libs(
        &files,
        "operation-node/column-definition-node.ts",
        options,
        &lib_files,
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .collect::<Vec<_>>();

    assert!(
        lacks_any_diagnostic_code(&diagnostics, &[2322]),
        "Imported Kysely freeze factory method should preserve contextual literal return. Got: {diagnostics:#?}"
    );
}

#[test]
fn kysely_is_object_guard_narrows_string_or_marker_object() {
    let source = r#"
type ShallowRecord<K extends keyof any, T> = { [P in K]: T };

declare function isObject(obj: unknown): obj is ShallowRecord<string, unknown>;

interface NoMigrations {
  readonly __noMigrations__: true;
}

function migrateTo(targetMigrationName: string | NoMigrations): void {
  if (
    isObject(targetMigrationName) &&
    targetMigrationName.__noMigrations__ === true
  ) {
    return;
  }
}
"#;

    let diagnostics = strict_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diagnostics, &[2339]),
        "Kysely isObject guard should narrow string | marker object. Got: {:#?}",
        format_diagnostics(source, &diagnostics)
    );
}
