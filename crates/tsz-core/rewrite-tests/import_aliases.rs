use std::sync::Arc;

use tsz::service::LanguageService;
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn options() -> CompilerOptions {
    CompilerOptions {
        no_emit: true,
        strict: true,
        target: "es2015".to_string(),
        module: "commonjs".to_string(),
        ..CompilerOptions::default()
    }
}

fn source(path: &str, text: &str) -> SourceInput {
    SourceInput::new(path, Arc::<str>::from(text))
}

fn assert_complete(output: &tsz::CompileOutput) {
    assert!(
        output.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        output.diagnostics
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

#[test]
fn type_only_named_import_supplies_the_value_side_of_a_type_query() {
    let declaration = source("a.ts", "export class A {}");
    let usage = source(
        "b.ts",
        "import type { A } from './a';\nlet AConstructor: typeof A;",
    );

    for roots in [
        vec![declaration.clone(), usage.clone()],
        vec![usage, declaration],
    ] {
        assert_complete(&Compiler::new().compile(roots, &options()));
    }
}

#[test]
fn renamed_regular_and_type_only_imports_work_inside_wrapped_type_queries() {
    for import in [
        "import type { Original as Constructor } from './origin';",
        "import { Original as Constructor } from './origin';",
    ] {
        let output = Compiler::new().compile(
            vec![
                source("origin.ts", "export class Original {}"),
                source(
                    "use.ts",
                    &format!("{import}\nlet wrapped: (typeof Constructor);"),
                ),
            ],
            &options(),
        );
        assert_complete(&output);
    }
}

#[test]
fn named_type_imports_resolve_exported_aliases_without_losing_local_identity() {
    let declaration = source("keys.ts", "export type PropertyKeyAlias = string | symbol;");
    let usage = source(
        "use.ts",
        concat!(
            "import type { PropertyKeyAlias as RenamedKey } from './keys';\n",
            "export type Wrapped<T extends Record<RenamedKey, unknown>> = T;",
        ),
    );

    for roots in [
        vec![declaration.clone(), usage.clone()],
        vec![usage.clone(), declaration.clone()],
    ] {
        assert_complete(&Compiler::new().compile(roots, &options()));
    }

    let mut service = LanguageService::new(options());
    service.open("keys.ts", declaration.text);
    service.open("use.ts", usage.text.clone());
    let import = usage.text.find("RenamedKey").unwrap() as u32;
    let reference = usage.text.rfind("RenamedKey").unwrap() as u32;
    let definition = service
        .definition_and_bound_span("use.ts", reference + 1)
        .expect("imported type-alias definition");
    assert_eq!(definition.definitions.len(), 1);
    assert_eq!(definition.definitions[0].file_name, "use.ts");
    assert_eq!(definition.definitions[0].text_span.start, import);
}

#[test]
fn imported_generic_type_aliases_are_structural_and_unexported_targets_fail_closed() {
    for import in [
        "import type { Box as Wrapped } from './model';",
        "import { Box as Wrapped } from './model';",
    ] {
        let output = Compiler::new().compile(
            vec![
                source("model.ts", "export type Box<T> = { value: T };"),
                source(
                    "use.ts",
                    &format!("{import}\nconst value: Wrapped<number> = {{ value: 1 }};"),
                ),
            ],
            &options(),
        );
        assert_complete(&output);
    }

    let hidden = Compiler::new().compile(
        vec![
            source("model.ts", "type Hidden = string;"),
            source(
                "use.ts",
                "import type { Hidden } from './model';\nlet value: Hidden;",
            ),
        ],
        &options(),
    );
    assert!(hidden.diagnostics.is_empty(), "{:?}", hidden.diagnostics);
    assert_eq!(hidden.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(hidden.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn import_specifier_selects_the_target_and_unowned_value_targets_fail_closed() {
    let class_target = source("class-target.ts", "export class Selected {}");
    let type_target = source("type-target.ts", "export type Selected = string;");

    let selected_class = Compiler::new().compile(
        vec![
            class_target.clone(),
            type_target.clone(),
            source(
                "use.ts",
                "import type { Selected } from './class-target';\nlet value: typeof Selected;",
            ),
        ],
        &options(),
    );
    assert_complete(&selected_class);

    let selected_type = Compiler::new().compile(
        vec![
            class_target,
            type_target,
            source(
                "use.ts",
                "import type { Selected } from './type-target';\nlet value: typeof Selected;",
            ),
        ],
        &options(),
    );
    assert!(selected_type.diagnostics.is_empty());
    assert_eq!(
        selected_type.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert_eq!(
        selected_type.exit_status,
        CompileExitStatus::SemanticIncomplete
    );
}

#[test]
fn local_value_shadowing_wins_over_a_type_only_import_alias() {
    let output = Compiler::new().compile(
        vec![
            source("types.ts", "export type Name = string;"),
            source(
                "use.ts",
                "import type { Name } from './types';\nconst Name = 1;\nlet value: typeof Name;",
            ),
        ],
        &options(),
    );
    assert_complete(&output);
}

#[test]
fn navigation_keeps_the_type_query_on_the_local_import_alias() {
    let mut service = LanguageService::new(options());
    service.open("origin.ts", Arc::<str>::from("export class Original {}"));
    let usage = concat!(
        "import type { Original as Constructor } from './origin';\n",
        "let value: typeof Constructor;",
    );
    service.open("use.ts", Arc::<str>::from(usage));

    let import_position = usage.find("Constructor").unwrap() as u32;
    let query_position = usage.rfind("Constructor").unwrap() as u32;
    let definition = service
        .definition_and_bound_span("use.ts", query_position + 1)
        .expect("type-query import definition");
    assert_eq!(definition.definitions.len(), 1);
    assert_eq!(definition.definitions[0].file_name, "use.ts");
    assert_eq!(definition.definitions[0].text_span.start, import_position);

    let references = service.references("use.ts", import_position + 1);
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].references.len(), 2);
}

#[test]
fn imported_array_type_query_preserves_authored_missing_property_order() {
    let declaration = source(
        "model.ts",
        "export declare const shapes:{zeta:string;alpha:number}[];",
    );
    let usage = source(
        "use.ts",
        concat!(
            "import type {shapes} from './model';\n",
            "declare const missing:{present:number}[];\n",
            "const value:typeof shapes=missing;",
        ),
    );

    for roots in [
        vec![declaration.clone(), usage.clone()],
        vec![usage, declaration],
    ] {
        let output = Compiler::new().compile(roots, &options());
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
        assert_eq!(output.diagnostics.len(), 1, "{:?}", output.diagnostics);
        assert_eq!(output.diagnostics[0].code, 2322);
        assert_eq!(
            output.diagnostics[0]
                .related_information
                .iter()
                .map(|related| related.message_text.as_str())
                .collect::<Vec<_>>(),
            [
                "Type '{ present: number; }' is missing the following properties from type '{ zeta: string; alpha: number; }': zeta, alpha"
            ]
        );
    }
}

#[test]
fn direct_imported_object_missing_properties_stays_deferred_until_ts2739_is_owned() {
    let declaration = source(
        "model.ts",
        "export const shape:{zeta:string;alpha:number}={zeta:'',alpha:0};",
    );
    let usage = source(
        "use.ts",
        "import type {shape} from './model';\nconst value:typeof shape={};",
    );

    for roots in [
        vec![declaration.clone(), usage.clone()],
        vec![usage, declaration],
    ] {
        let output = Compiler::new().compile(roots, &options());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn leading_parent_segments_do_not_false_link_a_sibling_source() {
    let declaration = source("target.ts", "export class Target {}");
    let usage = source(
        "use.ts",
        "import type {Target} from '../../target';\nlet value:typeof Target;",
    );

    for roots in [
        vec![declaration.clone(), usage.clone()],
        vec![usage, declaration],
    ] {
        let output = Compiler::new().compile(roots, &options());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn nested_parent_import_resolution_is_root_order_independent() {
    let declaration = source("src/model.ts", "export class Target {}");
    let usage = source(
        "src/nested/use.ts",
        "import type {Target} from '../model';\nlet value:typeof Target;",
    );

    for roots in [
        vec![declaration.clone(), usage.clone()],
        vec![usage, declaration],
    ] {
        assert_complete(&Compiler::new().compile(roots, &options()));
    }
}

#[test]
fn unsupported_exact_extensions_fail_closed() {
    for extension in ["foo", "json"] {
        let declaration = source(&format!("model.{extension}"), "export class Unsupported {}");
        let usage = source(
            "use.ts",
            &format!(
                "import type {{Unsupported}} from './model.{extension}';\nlet value:typeof Unsupported;"
            ),
        );

        for roots in [
            vec![declaration.clone(), usage.clone()],
            vec![usage, declaration],
        ] {
            let output = Compiler::new().compile(roots, &options());
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
            assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
        }
    }
}

#[test]
fn exact_javascript_extension_respects_allow_js() {
    let declaration = source("model.js", "export class JavaScriptTarget {}");
    let usage = source(
        "use.ts",
        "import type {JavaScriptTarget} from './model.js';\nlet value:typeof JavaScriptTarget;",
    );

    for roots in [
        vec![declaration.clone(), usage.clone()],
        vec![usage, declaration],
    ] {
        let disabled = Compiler::new().compile(roots.clone(), &options());
        assert!(
            disabled.diagnostics.is_empty(),
            "{:?}",
            disabled.diagnostics
        );
        assert_eq!(disabled.semantic_completion, SemanticCompletion::Deferred);
        assert_eq!(disabled.exit_status, CompileExitStatus::SemanticIncomplete);

        let mut enabled_options = options();
        enabled_options.allow_js = true;
        assert_complete(&Compiler::new().compile(roots, &enabled_options));
    }
}

#[test]
fn explicit_typescript_extension_resolves_in_either_root_order() {
    let declaration = source("model.ts", "export class TypeScriptTarget {}");
    let usage = source(
        "use.ts",
        "import type {TypeScriptTarget} from './model.ts';\nlet value:typeof TypeScriptTarget;",
    );

    for roots in [
        vec![declaration.clone(), usage.clone()],
        vec![usage, declaration],
    ] {
        assert_complete(&Compiler::new().compile(roots, &options()));
    }
}
