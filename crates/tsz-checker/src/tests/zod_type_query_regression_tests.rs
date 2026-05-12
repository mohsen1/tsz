use crate::context::CheckerOptions;
use crate::test_utils::{check_source_diagnostics, check_source_with_libs, load_default_lib_files};

#[test]
fn generic_promise_then_flattens_promise_return_from_callback() {
    let diags = check_source_diagnostics(
        r#"
interface PromiseLike<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): PromiseLike<TResult1>;
}

interface Promise<T> {
    then<TResult1 = T>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | undefined | null
    ): Promise<TResult1>;
}

interface Response {
    json(): Promise<{ entries: string[] }>;
}

declare function fetch(url: string): Promise<Response>;

fetch("/entries")
    .then(res => res.json())
    .then(data => {
        const entries: string[] = data.entries;
        return entries;
    });
"#,
    );

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2339 || d.code == 2345)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected Promise.then callback returning Promise<T> to infer T, got: {:?}",
        errors.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn object_literal_function_property_can_read_earlier_self_property() {
    let diags = check_source_diagnostics(
        r#"
const entryKeys = {
    all: ['entries'] as const,
    list: () => [...entryKeys.all, 'list'] as const
};

declare function takesKey(key: readonly ['entries', 'list']): void;
takesKey(entryKeys.list());
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345 | 7022 | 7023))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected self-referencing object literal property to keep earlier property type, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn conditional_with_failed_infer_uses_false_branch() {
    let diags = check_source_diagnostics(
        r#"
interface Register {}
interface Error { message: string }
type DefaultError = Register extends { defaultError: infer TError } ? TError : Error;
declare const err: DefaultError;
declare function takesError(err: Error): void;
takesError(err);
declare function takesString(value: string): void;
takesString(err);
"#,
    );

    let relevant: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        relevant.len(),
        1,
        "Expected conditional infer miss to resolve false branch and reject string, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn namespace_helper_keeps_explicit_indexed_access_callable_type() {
    let diags = check_source_diagnostics(
        r#"
interface ObjectConstructor {
    keys(o: object): string[];
}
declare var Object: ObjectConstructor;

export namespace util {
    export const objectKeys: ObjectConstructor["keys"] = Object.keys;
    export const objectValues = (obj: any) => objectKeys(obj).map(key => obj[key]);
}
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2349 | 7006))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected namespace helper annotation to make objectKeys callable, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn switch_discriminant_can_use_const_object_member_cases() {
    let diags = check_source_diagnostics(
        r#"
const IssueCode = {
    invalid_type: "invalid_type",
    unrecognized_keys: "unrecognized_keys",
} as const;

interface InvalidTypeIssue {
    code: typeof IssueCode.invalid_type;
    received: "string" | "undefined";
}
interface UnrecognizedKeysIssue {
    code: typeof IssueCode.unrecognized_keys;
    keys: string[];
}
type Issue = InvalidTypeIssue | UnrecognizedKeysIssue;

declare function assertNever(issue: never): never;

function message(issue: Issue): string {
    switch (issue.code) {
        case IssueCode.invalid_type:
            return issue.received;
        case IssueCode.unrecognized_keys:
            const keys: string[] = issue.keys;
            return "";
        default:
            return assertNever(issue);
    }
}
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected const-object switch cases to narrow discriminated union, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn switch_discriminant_can_use_array_to_enum_member_cases() {
    let diags = check_source_diagnostics(
        r#"
namespace util {
    export const arrayToEnum = <T extends string, U extends [T, ...T[]]>(
        items: U
    ): { [k in U[number]]: k } => {
        const obj: any = {};
        for (const item of items) obj[item] = item;
        return obj as any;
    };

    export function assertNever(_x: never): never {
        throw new Error();
    }
}

const IssueCode = util.arrayToEnum([
    "invalid_type",
    "unrecognized_keys",
]);
type IssueCode = keyof typeof IssueCode;

interface IssueBase {
    path: (string | number)[];
    message?: string;
}
interface InvalidTypeIssue extends IssueBase {
    code: typeof IssueCode.invalid_type;
    received: "string" | "undefined";
}
interface UnrecognizedKeysIssue extends IssueBase {
    code: typeof IssueCode.unrecognized_keys;
    keys: string[];
}
type Issue = InvalidTypeIssue | UnrecognizedKeysIssue;

function message(issue: Issue): string {
    let message: string;
    switch (issue.code) {
        case IssueCode.invalid_type:
            if (issue.received === "undefined") {
                message = "Required";
            } else {
                message = issue.received;
            }
            break;
        case IssueCode.unrecognized_keys:
            const keys: string[] = issue.keys;
            message = "";
            break;
        default:
            return util.assertNever(issue);
    }
    return message;
}
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected arrayToEnum switch cases to narrow discriminated union, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn keyof_array_to_enum_includes_every_string_literal_key() {
    let diags = check_source_diagnostics(
        r#"
namespace util {
    export const arrayToEnum = <T extends string, U extends [T, ...T[]]>(
        items: U
    ): { [k in U[number]]: k } => {
        const obj: any = {};
        for (const item of items) obj[item] = item;
        return obj as any;
    };
}

export const ParsedType = util.arrayToEnum([
    "string",
    "undefined",
    "object",
]);

export type ParsedType = keyof typeof ParsedType;

declare const received: ParsedType;
declare function takeParsed(value: ParsedType): ParsedType;

const direct: ParsedType = ParsedType.undefined;
const viaCall: ParsedType = takeParsed(ParsedType.string);

if (received === "undefined") {
    const value: "undefined" = received;
}
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2322 | 2345 | 2367))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected keyof arrayToEnum to include all literal keys, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_issue_data_preserves_union_properties_through_omit_and_spread() {
    let diags = check_source_diagnostics(
        r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };

namespace util {
    export type OmitKeys<T, K extends string> = Pick<T, Exclude<keyof T, K>>;
}

interface IssueBase {
    path: (string | number)[];
    message?: string;
}
interface InvalidTypeIssue extends IssueBase {
    code: "invalid_type";
    expected: "string";
    received: "undefined" | "string";
}
interface CustomIssue extends IssueBase {
    code: "custom";
    params?: { [k: string]: any };
}

type IssueOptionalMessage = InvalidTypeIssue | CustomIssue;
type StripPath<T extends object> = T extends any ? util.OmitKeys<T, "path"> : never;
type IssueData = StripPath<IssueOptionalMessage> & { path?: (string | number)[] };

declare const issueData: IssueData;
const fullPath: (string | number)[] = [];
const fullIssue = {
    ...issueData,
    path: fullPath,
};

declare function map(issue: IssueOptionalMessage): void;
map(fullIssue);
const msg = issueData.message || "";
"#,
    );

    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2339 | 2345 | 2322))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected Zod IssueData to preserve union properties through OmitKeys and spread, got: {:?}",
        relevant
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_issue_5030_defaults_path_with_logical_or_array_literal() {
    let diags = check_source_diagnostics(
        r#"
type ParseParams = {
    path: (string | number)[];
    data: unknown;
    errorMap: unknown;
    async: boolean;
};

type Partial<T> = { [P in keyof T]?: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<T, U> = T extends U ? never : T;
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

type ParseParamsNoData = Omit<ParseParams, "data">;
type ParsePathComponent = string | number;

declare function pathFromArray(arr: ParsePathComponent[]): unknown;

function test(params: Partial<ParseParamsNoData>) {
    pathFromArray(params.path || []);
}
"#,
    );

    assert!(
        diags.is_empty(),
        "Expected `params.path || []` to keep path as `(string | number)[]` in argument position, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zod_issue_5030_defaults_path_with_lib_utility_aliases() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "expected default libs to load");

    let diags = check_source_with_libs(
        r#"
type ParseParams = {
    path: (string | number)[];
    data: unknown;
    errorMap: unknown;
    async: boolean;
};

type ParseParamsNoData = Omit<ParseParams, "data">;
type ParsePathComponent = string | number;

declare function pathFromArray(arr: ParsePathComponent[]): unknown;

function test(params: Partial<ParseParamsNoData>) {
    pathFromArray(params.path || []);
}
"#,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    );

    assert!(
        diags.is_empty(),
        "Expected lib-backed `Partial<Omit<...>>` to contextually type `params.path || []`, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
