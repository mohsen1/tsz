#[test]
fn contextual_kysely_select_array_preserves_aliased_expression_factory_param() {
    // Kysely-style reduction from #10683/#10677: the overload returns a mapped
    // selection over the inferred `SE`, but the arrow element inside the
    // readonly selection array still needs the contextual
    // `ExpressionBuilder<DB, TB>` parameter before `noImplicitAny`.
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

type DynamicReferenceBuilder<RA> = { dynamicReference: RA };

type TableExpression<DB, TB extends keyof DB> =
  | AnyAliasedTable<DB>
  | AnyTable<DB>
  | AliasedExpressionOrFactory<DB, TB>;

type TableExpressionOrList<DB, TB extends keyof DB> =
  | TableExpression<DB, TB>
  | ReadonlyArray<TableExpression<DB, TB>>;

type AnyAliasedTable<DB> = `${AnyTable<DB>} as ${string}`;
type AnyTable<DB> = keyof DB & string;

type ExtractTableAlias<DB, TE> = TE extends `${string} as ${infer TA}`
  ? TA extends keyof DB
    ? TA
    : never
  : TE extends keyof DB
    ? TE
    : never;

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
      : TE extends AliasedExpression<infer O, infer QA>
        ? QA extends A
          ? O
          : never
        : TE extends (qb: any) => AliasedExpression<infer O, infer QA>
          ? QA extends A
            ? O
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

type FromTables<DB, TB extends keyof DB, TE> = DrainOuterGeneric<
  TB | ExtractAliasFromTableExpression<DB, TE>
>;

type SelectFrom<
  DB,
  TB extends keyof DB,
  TE extends TableExpressionOrList<DB, TB>,
> = [TE] extends [keyof DB]
  ? SelectQueryBuilder<DB, TB | ExtractTableAlias<DB, TE>, {}>
  : [TE] extends [`${infer T} as ${infer A}`]
    ? T extends keyof DB
      ? SelectQueryBuilder<DB & ShallowRecord<A, DB[T]>, TB | A, {}>
      : never
    : TE extends ReadonlyArray<infer T>
      ? SelectQueryBuilder<From<DB, T>, FromTables<DB, TB, T>, {}>
      : SelectQueryBuilder<From<DB, TE>, FromTables<DB, TB, TE>, {}>;

type SelectExpression<DB, TB extends keyof DB> =
  | AnyAliasedColumnWithTable<DB, TB>
  | AnyAliasedColumn<DB, TB>
  | AnyColumnWithTable<DB, TB>
  | AnyColumn<DB, TB>
  | DynamicReferenceBuilder<any>
  | AliasedExpressionOrFactory<DB, TB>;

type SelectArg<DB, TB extends keyof DB, SE extends SelectExpression<DB, TB>> =
  | SE
  | ReadonlyArray<SE>
  | ((eb: ExpressionBuilder<DB, TB>) => ReadonlyArray<SE>);

type SelectCallback<DB, TB extends keyof DB> = (
  eb: ExpressionBuilder<DB, TB>,
) => ReadonlyArray<SelectExpression<DB, TB>>;

type FlattenSelectExpression<SE> =
  SE extends DynamicReferenceBuilder<infer RA>
    ? { [R in RA]: DynamicReferenceBuilder<R> }[RA]
    : SE;

type ExtractAliasFromSelectExpression<SE> = SE extends string
  ? ExtractAliasFromStringSelectExpression<SE>
  : SE extends AliasedExpression<any, infer EA>
    ? EA
    : SE extends (qb: any) => AliasedExpression<any, infer EA>
      ? EA
      : SE extends DynamicReferenceBuilder<infer RA>
        ? ExtractAliasFromStringSelectExpression<RA>
        : never;

type ExtractAliasFromStringSelectExpression<SE extends string> =
  SE extends `${string}.${infer C} as ${infer A}`
    ? A
    : SE extends `${string}.${infer C}`
      ? C
      : SE;

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

type KyselySelection<DB, TB extends keyof DB, SE> = [DB] extends [unknown]
  ? {
      [E in FlattenSelectExpression<SE> as ExtractAliasFromSelectExpression<E>]:
        SelectType<ExtractTypeFromSelectExpression<DB, TB, E>>
    }
  : {};

type CallbackSelection<DB, TB extends keyof DB, CB> =
  CB extends (eb: any) => ReadonlyArray<infer SE>
    ? KyselySelection<DB, TB, SE>
    : never;

interface SelectQueryBuilder<DB, TB extends keyof DB, O> {
  leftJoin<
    TE extends TableExpression<DB, TB>,
    K1 extends JoinReferenceExpression<DB, TB, TE>,
    K2 extends JoinReferenceExpression<DB, TB, TE>,
  >(
    table: TE,
    k1: K1,
    k2: K2,
  ): SelectQueryBuilderWithLeftJoin<DB, TB, O, TE>;

  innerJoin<
    TE extends TableExpression<DB, TB>,
    K1 extends JoinReferenceExpression<DB, TB, TE>,
    K2 extends JoinReferenceExpression<DB, TB, TE>,
  >(
    table: TE,
    k1: K1,
    k2: K2,
  ): SelectQueryBuilderWithInnerJoin<DB, TB, O, TE>;

  where(lhs: string, op: string, rhs: unknown): SelectQueryBuilder<DB, TB, O>;

  select<SE extends SelectExpression<DB, TB>>(
    selections: ReadonlyArray<SE>,
  ): SelectQueryBuilder<DB, TB, O & KyselySelection<DB, TB, SE>>;
  select<CB extends SelectCallback<DB, TB>>(
    callback: CB,
  ): SelectQueryBuilder<DB, TB, O & CallbackSelection<DB, TB, CB>>;
  select<SE extends SelectExpression<DB, TB>>(
    selection: SE,
  ): SelectQueryBuilder<DB, TB, O & KyselySelection<DB, TB, SE>>;
}

type JoinReferenceExpression<DB, TB extends keyof DB, TE> = DrainOuterGeneric<
  AnyColumn<From<DB, TE>, FromTables<DB, TB, TE>>
    | AnyColumnWithTable<From<DB, TE>, FromTables<DB, TB, TE>>
>;

type SelectQueryBuilderWithInnerJoin<
  DB,
  TB extends keyof DB,
  O,
  TE extends TableExpression<DB, TB>,
> = TE extends `${infer T} as ${infer A}`
  ? T extends keyof DB
    ? InnerJoinedBuilder<DB, TB, O, A, DB[T]>
    : never
  : TE extends keyof DB
    ? SelectQueryBuilder<DB, TB | TE, O>
    : never;

type InnerJoinedBuilder<DB, TB extends keyof DB, O, A extends string, R> =
  A extends keyof DB
    ? SelectQueryBuilder<InnerJoinedDB<DB, A, R>, TB | A, O>
    : SelectQueryBuilder<DB & ShallowRecord<A, R>, TB | A, O>;

type InnerJoinedDB<DB, A extends string, R> = DrainOuterGeneric<{
  [C in keyof DB | A]: C extends A ? R : C extends keyof DB ? DB[C] : never
}>;

type SelectQueryBuilderWithLeftJoin<
  DB,
  TB extends keyof DB,
  O,
  TE extends TableExpression<DB, TB>,
> = TE extends `${infer T} as ${infer A}`
  ? T extends keyof DB
    ? LeftJoinedBuilder<DB, TB, O, A, DB[T]>
    : never
  : TE extends keyof DB
    ? LeftJoinedBuilder<DB, TB, O, TE, DB[TE]>
    : never;

type LeftJoinedBuilder<DB, TB extends keyof DB, O, A extends keyof any, R> =
  A extends keyof DB
    ? SelectQueryBuilder<LeftJoinedDB<DB, A, R>, TB | A, O>
    : SelectQueryBuilder<DB & ShallowRecord<A, Nullable<R>>, TB | A, O>;

type LeftJoinedDB<DB, A extends keyof any, R> = DrainOuterGeneric<{
  [C in keyof DB | A]: C extends A
    ? Nullable<R>
    : C extends keyof DB
      ? DB[C]
      : never
}>;

class SelectQueryBuilderImpl<DB, TB extends keyof DB, O>
  implements SelectQueryBuilder<DB, TB, O>
{
  leftJoin(...args: any): any {
    return new SelectQueryBuilderImpl();
  }

  innerJoin(...args: any): any {
    return new SelectQueryBuilderImpl();
  }

  where(...args: any): any {
    return new SelectQueryBuilderImpl();
  }

  select<SE extends SelectExpression<DB, TB>>(
    selection: SelectArg<DB, TB, SE>,
  ): SelectQueryBuilder<DB, TB, O & KyselySelection<DB, TB, SE>> {
    return new SelectQueryBuilderImpl<DB, TB, O & KyselySelection<DB, TB, SE>>();
  }
}

class QueryCreator<DB> {
  selectFrom<TE extends TableExpressionOrList<DB, never>>(
    from: TE,
  ): SelectFrom<DB, never, TE> {
    return new SelectQueryBuilderImpl() as SelectFrom<DB, never, TE>;
  }
}

class Kysely<DB> extends QueryCreator<DB> {}

type MssqlSysTables = {
  "sys.tables": { name: string; object_id: number; schema_id: number; type: "U" };
  "sys.views": { name: string; type: "V" };
  "sys.schemas": { name: string; schema_id: number };
  "sys.columns": { name: string; object_id: number; user_type_id: number };
  "sys.types": { name: string; user_type_id: number };
};

const db = new Kysely<MssqlSysTables>();

db.selectFrom("sys.tables as tables")
  .leftJoin("sys.schemas as table_schemas", "table_schemas.schema_id", "tables.schema_id")
  .innerJoin("sys.columns as columns", "columns.object_id", "tables.object_id")
  .innerJoin("sys.types as types", "types.user_type_id", "columns.user_type_id")
  .select([
    "tables.name as table_name",
    (eb) =>
      eb
        .ref("tables.type")
        .$castTo<MssqlSysTables["sys.tables"]["type"] | MssqlSysTables["sys.views"]["type"]>()
        .as("table_type"),
    "table_schemas.name as table_schema_name",
    "columns.name as column_name",
    "types.name as type_name",
  ]);
"#;
    let diags = relevant_strict_default_lib_diagnostics(source);
    assert!(
        lacks_any_diagnostic_code(&diags, &[7006, 2347, 2322, 2394, 2416]),
        "Kysely-style selection callback should keep its builder context. Got: {diags:#?}"
    );
}
