//! Cross-file generic alias unions in `implements` member checks.
//!
//! A union alias whose members depend on unresolved type parameters (e.g.
//! `keyof DB[TB] & string` or a mapped-indexed template literal) must keep all
//! of its members when the union is evaluated. The evaluator's subtype-based
//! union simplification runs with `bypass_evaluation=true` and cannot judge a
//! generic-dependent `Application` member soundly: the expansion looks
//! string-like and gets absorbed by an object-shaped member (here
//! `Expression<unknown>`), collapsing the union and producing false `TS2416`
//! on every method of an implementing class (Kysely
//! `SelectQueryBuilderImpl` family, #10663).
//!
//! tsc does not subtype-reduce union members that depend on unresolved type
//! parameters, so neither do we. Cases vary binder names, member order, and
//! include a genuine-mismatch negative control so the rule follows the type
//! shape rather than identifier names.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::{check_multi_file_with_libs, load_lib_files};
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file_with_libs(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            ..CheckerOptions::default()
        },
        &load_lib_files(&["es5.d.ts"]),
    )
}

fn implements_member_errors(diagnostics: &[Diagnostic]) -> Vec<(u32, String)> {
    diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE
        })
        .map(|d| (d.code, d.message_text.to_string()))
        .collect()
}

const REFS_SRC: &str = r#"
export interface Expression<T> {
    readonly expressionType?: T | undefined;
}

export type AnyColumn<DB, TB extends keyof DB> = keyof DB[TB] & string;

export type AnyColumnWithTable<DB, TB extends keyof DB> = {
    [T in TB]: `${T & string}.${keyof DB[T] & string}`;
}[TB];

export type ReferenceExpression<DB, TB extends keyof DB> =
    | AnyColumn<DB, TB>
    | AnyColumnWithTable<DB, TB>
    | Expression<unknown>;

export interface WhereInterface<DB, TB extends keyof DB> {
    whereRef<LRE extends ReferenceExpression<DB, TB>, RRE extends ReferenceExpression<DB, TB>>(
        lhs: LRE,
        op: string,
        rhs: RRE,
    ): WhereInterface<DB, TB>;
}
"#;

const BUILDER_SRC: &str = r#"
import { ReferenceExpression, WhereInterface } from "./refs";

export interface SelectQueryBuilder<DB, TB extends keyof DB, O> extends WhereInterface<DB, TB> {
    whereRef<LRE extends ReferenceExpression<DB, TB>, RRE extends ReferenceExpression<DB, TB>>(
        lhs: LRE,
        op: string,
        rhs: RRE,
    ): SelectQueryBuilder<DB, TB, O>;
}

class SelectQueryBuilderImpl<DB, TB extends keyof DB, O> implements SelectQueryBuilder<DB, TB, O> {
    whereRef(
        lhs: ReferenceExpression<DB, TB>,
        op: string,
        rhs: ReferenceExpression<DB, TB>,
    ): SelectQueryBuilder<DB, TB, O> {
        return this;
    }
}
"#;

#[test]
fn generic_dependent_alias_union_member_survives_cross_file_implements() {
    let diags = check(
        &[("./refs.ts", REFS_SRC), ("./builder.ts", BUILDER_SRC)],
        "./builder.ts",
    );
    let errors = implements_member_errors(&diags);
    assert!(
        errors.is_empty(),
        "expected the implements check to accept the structurally identical member, got: {errors:?}",
    );
}

#[test]
fn renamed_binders_and_reordered_members_stay_clean() {
    let refs = r#"
export interface Operand<V> {
    readonly operandType?: V | undefined;
}

export type ColumnOf<Schema, Table extends keyof Schema> = keyof Schema[Table] & string;

export type QualifiedColumnOf<Schema, Table extends keyof Schema> = {
    [Row in Table]: `${Row & string}.${keyof Schema[Row] & string}`;
}[Table];

export type ColumnRef<Schema, Table extends keyof Schema> =
    | Operand<unknown>
    | QualifiedColumnOf<Schema, Table>
    | ColumnOf<Schema, Table>;

export interface FilterableQuery<Schema, Table extends keyof Schema> {
    compareColumns<L extends ColumnRef<Schema, Table>, R extends ColumnRef<Schema, Table>>(
        left: L,
        operator: string,
        right: R,
    ): FilterableQuery<Schema, Table>;
}
"#;
    let main = r#"
import { ColumnRef, FilterableQuery } from "./refs";

export interface Query<Schema, Table extends keyof Schema, Row>
    extends FilterableQuery<Schema, Table> {
    compareColumns<L extends ColumnRef<Schema, Table>, R extends ColumnRef<Schema, Table>>(
        left: L,
        operator: string,
        right: R,
    ): Query<Schema, Table, Row>;
}

class QueryImpl<Schema, Table extends keyof Schema, Row> implements Query<Schema, Table, Row> {
    compareColumns(
        left: ColumnRef<Schema, Table>,
        operator: string,
        right: ColumnRef<Schema, Table>,
    ): Query<Schema, Table, Row> {
        return this;
    }
}
"#;
    let diags = check(&[("./refs.ts", refs), ("./main.ts", main)], "./main.ts");
    let errors = implements_member_errors(&diags);
    assert!(
        errors.is_empty(),
        "expected renamed/reordered variant to stay clean, got: {errors:?}",
    );
}

#[test]
fn genuinely_narrower_impl_member_still_reports_ts2416() {
    // Negative control: the impl accepts only the object member, so the
    // interface's broader generic parameter genuinely does not fit and the
    // member-level mismatch must still be reported.
    let builder = r#"
import { Expression, ReferenceExpression, WhereInterface } from "./refs";

export interface SelectQueryBuilder<DB, TB extends keyof DB, O> extends WhereInterface<DB, TB> {
    whereRef<LRE extends ReferenceExpression<DB, TB>, RRE extends ReferenceExpression<DB, TB>>(
        lhs: LRE,
        op: string,
        rhs: RRE,
    ): SelectQueryBuilder<DB, TB, O>;
}

class SelectQueryBuilderImpl<DB, TB extends keyof DB, O> implements SelectQueryBuilder<DB, TB, O> {
    whereRef(
        lhs: Expression<unknown>,
        op: string,
        rhs: Expression<unknown>,
    ): SelectQueryBuilder<DB, TB, O> {
        return this;
    }
}
"#;
    let diags = check(
        &[("./refs.ts", REFS_SRC), ("./builder.ts", builder)],
        "./builder.ts",
    );
    let errors = implements_member_errors(&diags);
    assert!(
        !errors.is_empty(),
        "expected a genuine member mismatch to keep reporting TS2416",
    );
}

#[test]
fn single_file_equivalent_stays_clean() {
    let single = r#"
interface Expression<T> {
    readonly expressionType?: T | undefined;
}

type AnyColumn<DB, TB extends keyof DB> = keyof DB[TB] & string;

type AnyColumnWithTable<DB, TB extends keyof DB> = {
    [T in TB]: `${T & string}.${keyof DB[T] & string}`;
}[TB];

type ReferenceExpression<DB, TB extends keyof DB> =
    | AnyColumn<DB, TB>
    | AnyColumnWithTable<DB, TB>
    | Expression<unknown>;

interface WhereInterface<DB, TB extends keyof DB> {
    whereRef<LRE extends ReferenceExpression<DB, TB>, RRE extends ReferenceExpression<DB, TB>>(
        lhs: LRE,
        op: string,
        rhs: RRE,
    ): WhereInterface<DB, TB>;
}

interface SelectQueryBuilder<DB, TB extends keyof DB, O> extends WhereInterface<DB, TB> {
    whereRef<LRE extends ReferenceExpression<DB, TB>, RRE extends ReferenceExpression<DB, TB>>(
        lhs: LRE,
        op: string,
        rhs: RRE,
    ): SelectQueryBuilder<DB, TB, O>;
}

class SelectQueryBuilderImpl<DB, TB extends keyof DB, O> implements SelectQueryBuilder<DB, TB, O> {
    whereRef(
        lhs: ReferenceExpression<DB, TB>,
        op: string,
        rhs: ReferenceExpression<DB, TB>,
    ): SelectQueryBuilder<DB, TB, O> {
        return this;
    }
}
"#;
    let diags = check(&[("./single.ts", single)], "./single.ts");
    let errors = implements_member_errors(&diags);
    assert!(
        errors.is_empty(),
        "expected the single-file equivalent to stay clean, got: {errors:?}",
    );
}

/// Cross-file variant with a callback-bearing union alias whose member
/// references a cross-file generic interface (`ExpressionBuilder<DB, TB>`) in
/// function-parameter position. This exercises the residual false-TS2416 family
/// from #13044: when the same alias application `RefExpr<Schema, Tbl>` appears
/// as both the impl annotation and the erased constraint of the interface's type
/// parameter, the relation must recognize them as the same type by identity
/// (same base + same args) rather than expanding the union structurally.
#[test]
fn cross_file_callback_bearing_alias_union_implements_clean() {
    let types_src = r#"
export interface Operand<V> {
    readonly operandType?: V | undefined;
}

export interface RowsOperand<O> {
    readonly isRowsOperand: true;
    readonly operandType?: O | undefined;
}

export type OperandExpr<V> = Operand<V> | RowsOperand<Record<string, V>>;

export interface Builder<Schema, Tbl extends keyof Schema> {
    ref(reference: Tbl & string): unknown;
}

export type OperandFactory<Schema, Tbl extends keyof Schema, V> = (
    eb: Builder<Schema, Tbl>,
) => OperandExpr<V>;

export type ExprOrFactory<Schema, Tbl extends keyof Schema, V> =
    | OperandExpr<V>
    | OperandFactory<Schema, Tbl, V>;

export type ColumnOf<Schema, Tbl extends keyof Schema> = keyof Schema[Tbl] & string;

export type RefExpr<Schema, Tbl extends keyof Schema> =
    | ColumnOf<Schema, Tbl>
    | ExprOrFactory<Schema, Tbl, any>;

export interface Filterable<Schema, Tbl extends keyof Schema> {
    compareRef<L extends RefExpr<Schema, Tbl>, R extends RefExpr<Schema, Tbl>>(
        lhs: L,
        op: string,
        rhs: R,
    ): Filterable<Schema, Tbl>;
}
"#;
    let builder_src = r#"
import { RefExpr, Filterable } from "./types";

export interface QueryBuilder<Schema, Tbl extends keyof Schema, Out>
    extends Filterable<Schema, Tbl> {
    compareRef<L extends RefExpr<Schema, Tbl>, R extends RefExpr<Schema, Tbl>>(
        lhs: L,
        op: string,
        rhs: R,
    ): QueryBuilder<Schema, Tbl, Out>;
}

class QueryBuilderImpl<Schema, Tbl extends keyof Schema, Out>
    implements QueryBuilder<Schema, Tbl, Out>
{
    compareRef(
        lhs: RefExpr<Schema, Tbl>,
        op: string,
        rhs: RefExpr<Schema, Tbl>,
    ): QueryBuilder<Schema, Tbl, Out> {
        return this;
    }
}
"#;
    let diags = check(
        &[("./types.ts", types_src), ("./builder.ts", builder_src)],
        "./builder.ts",
    );
    let errors = implements_member_errors(&diags);
    assert!(
        errors.is_empty(),
        "expected cross-file callback-bearing alias union to stay clean, got: {errors:?}",
    );
}

/// Same callback-bearing pattern with renamed binders to guard against
/// identifier-specific logic.
#[test]
fn cross_file_callback_bearing_alias_renamed_binders_clean() {
    let types_src = r#"
export interface Operand<V> {
    readonly operandType?: V | undefined;
}

export interface RowsOperand<O> {
    readonly isRowsOperand: true;
    readonly operandType?: O | undefined;
}

export type OpExpr<Val> = Operand<Val> | RowsOperand<Record<string, Val>>;

export interface Composer<Db, Table extends keyof Db> {
    ref(reference: Table & string): unknown;
}

export type OpFactory<Db, Table extends keyof Db, Val> = (
    eb: Composer<Db, Table>,
) => OpExpr<Val>;

export type ExprOrOp<Db, Table extends keyof Db, Val> =
    | OpExpr<Val>
    | OpFactory<Db, Table, Val>;

export type ColOf<Db, Table extends keyof Db> = keyof Db[Table] & string;

export type ColRef<Db, Table extends keyof Db> =
    | ColOf<Db, Table>
    | ExprOrOp<Db, Table, any>;

export interface FilterOps<Db, Table extends keyof Db> {
    cmpRef<Left extends ColRef<Db, Table>, Right extends ColRef<Db, Table>>(
        lhs: Left,
        op: string,
        rhs: Right,
    ): FilterOps<Db, Table>;
}
"#;
    let main_src = r#"
import { ColRef, FilterOps } from "./types";

export interface Selector<Db, Table extends keyof Db, Row>
    extends FilterOps<Db, Table> {
    cmpRef<Left extends ColRef<Db, Table>, Right extends ColRef<Db, Table>>(
        lhs: Left,
        op: string,
        rhs: Right,
    ): Selector<Db, Table, Row>;
}

class SelectorImpl<Db, Table extends keyof Db, Row>
    implements Selector<Db, Table, Row>
{
    cmpRef(
        lhs: ColRef<Db, Table>,
        op: string,
        rhs: ColRef<Db, Table>,
    ): Selector<Db, Table, Row> {
        return this;
    }
}
"#;
    let diags = check(
        &[("./types.ts", types_src), ("./main.ts", main_src)],
        "./main.ts",
    );
    let errors = implements_member_errors(&diags);
    assert!(
        errors.is_empty(),
        "expected renamed-binders callback-bearing alias to stay clean, got: {errors:?}",
    );
}
