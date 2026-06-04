#[test]
fn test_check_files_parallel_zod_issue_data_cross_file_spread() {
    let files = vec![
        (
            "helpers/util.ts".to_string(),
            r#"
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<T, U> = T extends U ? never : T;

export namespace util {
    export type OmitKeys<T, K extends string> = Pick<T, Exclude<keyof T, K>>;
    export const arrayToEnum = <T extends string, U extends [T, ...T[]]>(
        items: U
    ): { [k in U[number]]: k } => {
        const obj: any = {};
        return obj as any;
    };
}
"#
            .to_string(),
        ),
        (
            "ZodError.ts".to_string(),
            r#"
import { ZodParsedType } from "./helpers/parseUtil";
import { util } from "./helpers/util";

export const ZodIssueCode = util.arrayToEnum([
    "invalid_type",
    "custom",
    "invalid_union",
    "invalid_enum_value",
    "unrecognized_keys",
    "invalid_arguments",
    "invalid_return_type",
    "invalid_date",
    "invalid_string",
    "too_small",
    "too_big",
    "invalid_intersection_types",
    "not_multiple_of",
]);

export type ZodIssueCode = keyof typeof ZodIssueCode;

export type ZodIssueBase = {
    path: (string | number)[];
    message?: string;
};

export interface ZodInvalidTypeIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_type;
    expected: ZodParsedType;
    received: ZodParsedType;
}

export interface ZodCustomIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.custom;
    params?: { [k: string]: any };
}

export interface ZodInvalidUnionIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_union;
    unionErrors: ZodError[];
}

export interface ZodInvalidEnumValueIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_enum_value;
    options: (string | number)[];
}

export interface ZodUnrecognizedKeysIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.unrecognized_keys;
    keys: string[];
}

export interface ZodInvalidArgumentsIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_arguments;
    argumentsError: ZodError;
}

export interface ZodInvalidReturnTypeIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_return_type;
    returnTypeError: ZodError;
}

export interface ZodInvalidDateIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_date;
}

export type StringValidation = "email" | "url" | "uuid" | "regex" | "cuid";

export interface ZodInvalidStringIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_string;
    validation: StringValidation;
}

export interface ZodTooSmallIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.too_small;
    minimum: number;
    inclusive: boolean;
    type: "array" | "string" | "number";
}

export interface ZodTooBigIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.too_big;
    maximum: number;
    inclusive: boolean;
    type: "array" | "string" | "number";
}

export interface ZodInvalidIntersectionTypesIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.invalid_intersection_types;
}

export interface ZodNotMultipleOfIssue extends ZodIssueBase {
    code: typeof ZodIssueCode.not_multiple_of;
    multipleOf: number;
}

export type ZodIssueOptionalMessage =
    | ZodInvalidTypeIssue
    | ZodUnrecognizedKeysIssue
    | ZodInvalidUnionIssue
    | ZodInvalidEnumValueIssue
    | ZodInvalidArgumentsIssue
    | ZodInvalidReturnTypeIssue
    | ZodInvalidDateIssue
    | ZodInvalidStringIssue
    | ZodTooSmallIssue
    | ZodTooBigIssue
    | ZodInvalidIntersectionTypesIssue
    | ZodNotMultipleOfIssue
    | ZodCustomIssue;

export type ZodIssue = ZodIssueOptionalMessage & { message: string };

export class ZodError {
    issues: ZodIssue[] = [];
    constructor(issues: ZodIssue[]) {
        this.issues = issues;
    }
}

type stripPath<T extends object> = T extends any
    ? util.OmitKeys<T, "path">
    : never;

export type IssueData = stripPath<ZodIssueOptionalMessage> & {
    path?: (string | number)[];
};
"#
            .to_string(),
        ),
        (
            "helpers/parseUtil.ts".to_string(),
            r#"
import {
    IssueData,
    ZodIssue,
    ZodIssueOptionalMessage,
} from "../ZodError";
import { util } from "./util";

export const ZodParsedType = util.arrayToEnum([
    "string",
    "nan",
    "number",
    "integer",
    "boolean",
    "undefined",
    "null",
    "array",
    "object",
    "unknown",
]);

export type ZodParsedType = keyof typeof ZodParsedType;

export const makeIssue = (params: {
    path: (string | number)[];
    issueData: IssueData;
}): ZodIssue => {
    const { path, issueData } = params;
    const fullPath = [...path, ...(issueData.path || [])];
    const fullIssue = {
        ...issueData,
        path: fullPath,
    };

    consume(fullIssue);

    return {
        ...issueData,
        path: fullPath,
        message: issueData.message || "",
    };
};

declare function consume(issue: ZodIssueOptionalMessage): void;
"#
            .to_string(),
        ),
    ];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
        &[],
    );

    let target_diags: Vec<_> = result
        .file_results
        .iter()
        .flat_map(|file| file.diagnostics.iter())
        .filter(|diag| matches!(diag.code, 2339 | 2345 | 2322))
        .collect();
    assert!(
        target_diags.is_empty(),
        "Expected Zod IssueData cross-file spread to preserve union properties. Actual diagnostics: {target_diags:#?}"
    );
}

#[test]
fn test_check_files_parallel_keeps_namespace_local_component_for_create_element_inference() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    type CreateElementChildren<P> =
        P extends { children?: infer C }
            ? C extends any[]
                ? C
                : C[]
            : unknown;

    declare function createElement<P extends {}>(
        type: ComponentClass<P>,
        ...children: CreateElementChildren<P>
    ): any;

    declare function createElement2<P extends {}>(
        type: ComponentClass<P>,
        child: CreateElementChildren<P>
    ): any;

    class InferFunctionTypes extends Component<{ children: (foo: number) => string }> {}

    createElement(InferFunctionTypes, (foo) => "" + foo);
    createElement2(InferFunctionTypes, [(foo) => "" + foo]);
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let result = check_files_parallel(
        &program,
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
        &[],
    );

    let file_result = result
        .file_results
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected test.ts result");
    assert!(
        !file_result.diagnostics.iter().any(|diag| diag.code == 2345),
        "parallel file checking should preserve namespace-local ComponentClass inference for createElement. Actual diagnostics: {:#?}",
        file_result.diagnostics,
    );

    let file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
    let mut recreated_checker = crate::checker::state::CheckerState::with_options(
        &file.arena,
        &rebuilt_binder,
        &query_cache,
        file.file_name.clone(),
        &crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );
    recreated_checker.check_source_file(file.source_file);
    assert!(
        !recreated_checker
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == 2345),
        "recreated binder checking should preserve namespace-local ComponentClass inference for createElement. Actual diagnostics: {:#?}",
        recreated_checker.ctx.diagnostics,
    );
}

#[test]
fn test_recreated_binder_keeps_namespace_local_generic_class_application_instance_type() {
    let files = vec![(
        "test.ts".to_string(),
        r#"
declare class Component<P> { constructor(props: P); props: P; }

namespace N1 {
    declare class Component<P> {
        constructor(props: P);
    }

    interface ComponentClass<P = {}> {
        new (props: P): Component<P>;
    }

    declare let c: ComponentClass<{ children: (foo: number) => string }>;
    const z = new c({ children: (foo) => "" + foo });
    z.props;
}
"#
        .to_string(),
    )];

    let program = compile_files(files);
    let file = program
        .files
        .iter()
        .find(|file| file.file_name == "test.ts")
        .expect("expected merged test.ts file");
    let rebuilt_binder = create_binder_from_bound_file(file, &program, 0);
    let query_cache = tsz_solver::construction::QueryCache::new(&program.type_interner);
    let mut checker = crate::checker::state::CheckerState::with_options(
        &file.arena,
        &rebuilt_binder,
        &query_cache,
        file.file_name.clone(),
        &crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(file.source_file);
    let source_file = file
        .arena
        .get(file.source_file)
        .and_then(|node| file.arena.get_source_file(node))
        .expect("missing source file");
    let namespace_stmt = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&stmt_idx| {
            let Some(stmt_node) = file.arena.get(stmt_idx) else {
                return false;
            };
            let Some(module_decl) = file.arena.get_module(stmt_node) else {
                return false;
            };
            file.arena
                .get_identifier_at(module_decl.name)
                .is_some_and(|ident| ident.escaped_text.as_str() == "N1")
        })
        .expect("missing namespace declaration");
    let namespace_body_statements = file
        .arena
        .get(namespace_stmt)
        .and_then(|node| file.arena.get_module(node))
        .map(|module| module.body)
        .and_then(|body_idx| file.arena.get(body_idx))
        .and_then(|node| file.arena.get_module_block(node))
        .and_then(|module_block| module_block.statements.as_ref())
        .map(|statements| statements.nodes.clone())
        .expect("missing namespace body");
    let component_return_name = namespace_body_statements
        .iter()
        .copied()
        .find_map(|stmt_idx| {
            let stmt_node = file.arena.get(stmt_idx)?;
            let interface_decl = file.arena.get_interface(stmt_node)?;
            let interface_name = file
                .arena
                .get_identifier_at(interface_decl.name)?
                .escaped_text
                .as_str();
            if interface_name != "ComponentClass" {
                return None;
            }
            let construct_idx = interface_decl.members.nodes.first().copied()?;
            let construct_node = file.arena.get(construct_idx)?;
            let construct_sig = file.arena.get_signature(construct_node)?;
            let type_ref_node = file.arena.get(construct_sig.type_annotation)?;
            let type_ref = file.arena.get_type_ref(type_ref_node)?;
            Some(type_ref.type_name)
        })
        .expect("missing Component<P> return type");
    let local_component_sym = file
        .scopes
        .iter()
        .find(|scope| {
            scope.kind == crate::binder::ContainerKind::Module && scope.table.has("Component")
        })
        .and_then(|scope| scope.table.get("Component"))
        .expect("missing namespace-local Component");
    let binder_resolved_component = rebuilt_binder.resolve_identifier_with_filter(
        &file.arena,
        component_return_name,
        &[],
        |_| true,
    );

    assert_eq!(
        binder_resolved_component,
        Some(local_component_sym),
        "rebuilt binder should resolve the interface's unqualified Component<P> to the namespace-local symbol",
    );
    assert!(
        checker.ctx.diagnostics.iter().any(|diag| diag.code == 2339),
        "recreated binder should keep the namespace-local Component<P> instance type inside ComponentClass<P>. Actual diagnostics: {:#?}",
        checker.ctx.diagnostics
    );
}
