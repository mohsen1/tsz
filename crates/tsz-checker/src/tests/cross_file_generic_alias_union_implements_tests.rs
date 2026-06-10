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
