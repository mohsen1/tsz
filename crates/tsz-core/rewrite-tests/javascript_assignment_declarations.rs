use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::service::{LanguageService, ServiceQuery};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

#[macro_use]
#[path = "fixtures/service_query_expect.rs"]
mod service_query_expect;
expect_claimed_extension!();

fn compile(files: &[(&str, &str)]) -> tsz::CompileOutput {
    compile_with_no_unused(files, false)
}

fn compile_with_no_unused(files: &[(&str, &str)], no_unused: bool) -> tsz::CompileOutput {
    Compiler::new().compile(
        files
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        &CompilerOptions {
            allow_js: true,
            check_js: Some(true),
            strict: false,
            no_emit: true,
            no_unused_locals: no_unused,
            no_unused_parameters: no_unused,
            ..CompilerOptions::default()
        },
    )
}

fn compile_javascript_implicit_any(files: &[(&str, &str)], no_check: bool) -> tsz::CompileOutput {
    Compiler::new().compile(
        files
            .iter()
            .map(|(path, source)| SourceInput::new(*path, Arc::<str>::from(*source)))
            .collect(),
        &CompilerOptions {
            allow_js: true,
            check_js: Some(true),
            strict: false,
            no_implicit_any: Some(true),
            no_check,
            no_emit: true,
            target: "es2015".to_string(),
            ..CompilerOptions::default()
        },
    )
}

#[test]
fn no_unused_function_expression_gap_is_node_scoped_and_option_gated() {
    let source = concat!(
        "{",
        "  const renamedHolder: (unused: number) => number = function (unused) { const nestedUnused = 1; return 1; };",
        "  renamedHolder;",
        "}",
        "MissingSibling;",
    );
    let deferred = compile_with_no_unused(&[("no-unused.ts", source)], true);
    assert_eq!(codes(&deferred), [2304], "{:#?}", deferred.diagnostics);
    assert_eq!(deferred.semantic_completion, SemanticCompletion::Deferred);

    let claimed = compile_with_no_unused(&[("no-unused-off.ts", source)], false);
    assert_eq!(codes(&claimed), [2304], "{:#?}", claimed.diagnostics);
    assert_eq!(claimed.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn no_unused_without_a_function_expression_candidate_stays_complete() {
    assert_complete(&compile_with_no_unused(
        &[(
            "no-candidate.ts",
            "function kept(used) { return used; } kept(1);",
        )],
        true,
    ));
}

fn assert_complete(output: &tsz::CompileOutput) {
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Complete,
        "emitted={:#?}, status={:?}",
        output.emitted_files,
        output.exit_status,
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success);
}

fn codes(output: &tsz::CompileOutput) -> Vec<u32> {
    output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn assert_property_services_nonclaimed(
    service: &LanguageService,
    path: &str,
    offset: u32,
    files: &[String],
) {
    assert!(matches!(
        service.quick_info(path, offset),
        ServiceQuery::Nonclaimed(_)
    ));
    assert!(
        matches!(
            service.definition_and_bound_span(path, offset),
            ServiceQuery::Nonclaimed(_)
        ),
        "definition at {path}:{offset} must wait for property navigation identity",
    );
    assert!(matches!(
        service.references(path, offset),
        ServiceQuery::Nonclaimed(_)
    ));
    assert!(matches!(
        service.document_highlights(path, offset, files),
        ServiceQuery::Nonclaimed(_)
    ));
    assert!(matches!(
        service.rename(path, offset),
        ServiceQuery::Nonclaimed(_)
    ));
}

#[test]
fn renamed_empty_object_collects_a_direct_named_property() {
    assert_complete(&compile(&[(
        "renamed.js",
        "const vessel = {}; vessel.answer = 42; vessel.answer;",
    )]));
}

#[test]
fn function_valued_property_remains_callable() {
    assert_complete(&compile(&[(
        "callable.js",
        concat!(
            "var target = {};",
            "target.perform = function (renamed) { return true; };",
            "target.perform();",
        ),
    )]));
}

#[test]
fn untyped_javascript_function_values_have_weak_minimum_arity() {
    assert_complete(&compile(&[(
        "variable-call.js",
        "const invoke = function (renamed) { return renamed; }; invoke();",
    )]));
}

#[test]
fn untyped_javascript_function_declarations_have_weak_minimum_arity() {
    assert_complete(&compile(&[(
        "declaration-call.js",
        "function invoke(renamed) { return renamed; } invoke();",
    )]));
}

#[test]
fn javascript_defaults_and_rest_keep_their_authored_call_shape() {
    assert_complete(&compile(&[(
        "default-rest.js",
        concat!(
            "function flexible(first = 1, ...remaining) { return first; }",
            "flexible(); flexible(1, 2, 3);",
        ),
    )]));
}

#[test]
fn typescript_function_parameters_keep_strong_minimum_arity() {
    let output = compile(&[(
        "typescript-arity.ts",
        "function invoke(renamed: number) { return renamed; } invoke();",
    )]);
    assert_eq!(codes(&output), [2554], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn immediate_iife_uses_its_argument_count_independently_of_source_kind() {
    assert_complete(&compile(&[(
        "iife.ts",
        "(function (first, trailing) { return first; })(1);",
    )]));
    let typed = compile(&[(
        "typed-iife.ts",
        "(function (first: number, trailing: number) { return first; })(1);",
    )]);
    assert_eq!(codes(&typed), [2554], "{:#?}", typed.diagnostics);
}

#[test]
fn cold_and_warm_expando_root_queries_agree() {
    let cold_first = compile(&[(
        "cold-first.js",
        "root.value = 1; root.value; var root = {}; root.value;",
    )]);
    let warm_first = compile(&[(
        "warm-first.js",
        "var root = {}; root.value = 1; root.value;",
    )]);
    assert_complete(&cold_first);
    assert_complete(&warm_first);
    assert_eq!(cold_first.diagnostics, warm_first.diagnostics);
    assert_eq!(
        cold_first.semantic_completion,
        warm_first.semantic_completion
    );
}

#[test]
fn repeated_incomplete_expando_root_demands_remain_deferred_not_cycle() {
    let output = compile(&[(
        "incomplete-root.js",
        concat!(
            "function root(value) {",
            "  switch (value) { case 1: return 1; default: return 2; }",
            "}",
            "root.property = 1; root.property; root.property;",
        ),
    )]);
    assert!(!codes(&output).contains(&2339), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn jsdoc_typed_javascript_signature_is_capability_deferred() {
    let output = compile(&[(
        "documented.js",
        concat!(
            "/** @param {number} value */",
            "function documented(value) { return value; }",
            "documented();",
            "missingSibling;",
        ),
    )]);
    assert_eq!(codes(&output), [2304], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn jsdoc_param_tag_on_class_property_initializer_has_no_false_implicit_any() {
    let source = concat!(
        "class Foo {\n",
        "    /**@param {string} x */\n",
        "    m = x => x.toLowerCase();\n",
        "}\n",
    );
    let output = compile_javascript_implicit_any(&[("a.js", source)], false);

    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn class_property_jsdoc_signature_locality_covers_wrappers_and_member_forms() {
    let source = concat!(
        "class Renamed {\n",
        "  /** @param {number} directParameter */\n",
        "  direct = directParameter => MissingInside;\n",
        "  /** @param {number} wrappedParameter */\n",
        "  wrapped = (((wrappedParameter) => wrappedParameter));\n",
        "  /** @param {number} expressionParameter */\n",
        "  expression = function (expressionParameter) { return expressionParameter; };\n",
        "  /** @param {number} staticParameter */\n",
        "  static fixed = staticParameter => staticParameter;\n",
        "  /* ordinary block comment */\n",
        "  ordinary = ordinaryParameter => ordinaryParameter;\n",
        "  plain = plainParameter => plainParameter;\n",
        "}\n",
        "MissingSibling;\n",
    );
    let output = compile_javascript_implicit_any(&[("locality.js", source)], false);
    let identity = output
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.len(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        identity,
        vec![
            (
                "locality.js",
                2304,
                source.find("MissingInside").unwrap() as u32,
                "MissingInside".len() as u32,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingInside'.",
                0,
            ),
            (
                "locality.js",
                7006,
                source.find("ordinaryParameter").unwrap() as u32,
                "ordinaryParameter".len() as u32,
                DiagnosticCategory::Error,
                "Parameter 'ordinaryParameter' implicitly has an 'any' type.",
                0,
            ),
            (
                "locality.js",
                7006,
                source.find("plainParameter").unwrap() as u32,
                "plainParameter".len() as u32,
                DiagnosticCategory::Error,
                "Parameter 'plainParameter' implicitly has an 'any' type.",
                0,
            ),
            (
                "locality.js",
                2304,
                source.find("MissingSibling").unwrap() as u32,
                "MissingSibling".len() as u32,
                DiagnosticCategory::Error,
                "Cannot find name 'MissingSibling'.",
                0,
            ),
        ],
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn class_property_jsdoc_signature_is_repeatable_root_order_independent_and_nocheck_safe() {
    let affected = (
        "affected.js",
        "class Vessel { /** @param {number} renamed */ invoke = renamed => renamed; }",
    );
    let independent = ("independent.js", "MissingCrossFile;");
    let forward = compile_javascript_implicit_any(&[affected, independent], false);
    let repeated = compile_javascript_implicit_any(&[affected, independent], false);
    let reverse = compile_javascript_implicit_any(&[independent, affected], false);

    assert_eq!(forward.diagnostics, repeated.diagnostics);
    assert_eq!(forward.diagnostics, reverse.diagnostics);
    assert_eq!(codes(&forward), [2304]);
    assert_eq!(forward.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(repeated.semantic_completion, forward.semantic_completion);
    assert_eq!(reverse.semantic_completion, forward.semantic_completion);

    let unchecked = compile_javascript_implicit_any(&[affected, independent], true);
    assert_eq!(unchecked.diagnostics, []);
    assert_eq!(unchecked.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(unchecked.exit_status, CompileExitStatus::Success);
}

#[test]
fn jsdoc_function_expression_is_deferred_but_ordinary_block_comments_are_not() {
    let documented = compile(&[(
        "documented-expression.js",
        "const renamed = /** @param {number} value */ function (value) { return value; }; renamed();",
    )]);
    assert_eq!(documented.diagnostics, []);
    assert_eq!(documented.semantic_completion, SemanticCompletion::Deferred);
    assert_complete(&compile(&[(
        "ordinary-comment.js",
        "/* @param {number} value */ function renamed(value) { return value; } renamed();",
    )]));
}

#[test]
fn jsdoc_property_value_defers_its_call_but_keeps_an_independent_diagnostic() {
    let output = compile(&[(
        "documented-property.js",
        concat!(
            "const root = {};",
            "root.invoke = /** @param {number} value */ function (value) { return value; };",
            "root.invoke(); MissingSibling;",
        ),
    )]);
    assert_eq!(codes(&output), [2304], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn harmless_jsdoc_is_conservatively_deferred_until_jsdoc_signatures_are_owned() {
    let output = compile(&[(
        "harmless-doc.js",
        "/** documentation only */ function ordinary(value) { return value; } ordinary();",
    )]);
    assert_eq!(output.diagnostics, []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn jsdoc_signature_withholds_quick_info_but_preserves_binder_definition_identity() {
    let source =
        "/** @param {number} value */ function documented(value) { return value; } documented();";
    let mut service = LanguageService::new(CompilerOptions {
        allow_js: true,
        check_js: Some(true),
        strict: false,
        no_emit: true,
        ..CompilerOptions::default()
    });
    service.open("documented-service.js", Arc::<str>::from(source));
    let declaration = source.find("documented").unwrap() as u32;
    let reference = source.rfind("documented").unwrap() as u32;
    assert!(matches!(
        service.quick_info("documented-service.js", declaration),
        ServiceQuery::Nonclaimed(_)
    ));
    assert!(
        service
            .definition_and_bound_span("documented-service.js", reference)
            .expect_claimed("documented function definition")
            .is_some()
    );
}

#[test]
fn pure_jsdoc_signature_gaps_keep_inside_and_outside_diagnostics() {
    for (path, source) in [
        (
            "documented-declaration.js",
            "/** @param {number} value */ function documented(value) { MissingInside; return value; } documented(); MissingOutside;",
        ),
        (
            "documented-expression.js",
            "const documented = /** @param {number} value */ function (value) { MissingInside; return value; }; documented(); MissingOutside;",
        ),
        (
            "documented-arrow.js",
            "const documented = /** @param {number} value */ (value) => { MissingInside; return value; }; documented(); MissingOutside;",
        ),
        (
            "documented-expression-arrow.js",
            "const documented = /** @param {number} value */ value => MissingInside; documented(); MissingOutside;",
        ),
        (
            "documented-default.js",
            "const documented = /** @param {number} value */ function (value = MissingDefault) { return value; }; documented(); MissingOutside;",
        ),
    ] {
        let output = compile(&[(path, source)]);
        assert_eq!(
            codes(&output),
            [2304, 2304],
            "{path}: {:#?}",
            output.diagnostics
        );
        assert!(
            ["MissingInside", "MissingDefault", "MissingOutside"]
                .iter()
                .filter(|name| source.contains(**name))
                .all(|name| output
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message_text.contains(*name))),
            "{path}: {:#?}",
            output.diagnostics,
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn variable_level_jsdoc_types_defer_renamed_wrapped_and_function_initializers() {
    let prefix = "function source(x) { return x; } function identity(x) { return x; }";
    for (path, source) in [
        (
            "variable-arrow.js",
            "/** @type {(x: number) => number} */ const renamed = x => x; renamed(); MissingSibling;",
        ),
        (
            "variable-wrapped.js",
            "/** @type {(x: number) => number} */ const renamed = (((x) => x)); renamed(); MissingSibling;",
        ),
        (
            "variable-function.js",
            "/** @type {(x: number) => number} */ const renamed = function (x) { return x; }; renamed(); MissingSibling;",
        ),
        (
            "variable-alias.js",
            "/** @type {(x: number) => number} */ const renamed = (source); renamed(); MissingSibling;",
        ),
        (
            "variable-call-wrapper.js",
            "/** @type {(x: number) => number} */ const renamed = identity(source); renamed(); MissingSibling;",
        ),
    ] {
        let source = format!("{prefix}{source}");
        let output = compile(&[(path, &source)]);
        assert_eq!(codes(&output), [2304], "{path}: {:#?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn variable_jsdoc_value_withholds_quick_info_but_keeps_definition_identity() {
    let source = concat!(
        "function source(value) { return value; }",
        "/** @type {(value: number) => number} */ const alias = source;",
        "alias();",
    );
    let mut service = LanguageService::new(CompilerOptions {
        allow_js: true,
        check_js: Some(true),
        strict: false,
        no_emit: true,
        ..CompilerOptions::default()
    });
    service.open("variable-jsdoc-service.js", Arc::<str>::from(source));
    let declaration = source.find("alias").unwrap() as u32;
    let reference = source.rfind("alias").unwrap() as u32;
    assert!(matches!(
        service.quick_info("variable-jsdoc-service.js", declaration),
        ServiceQuery::Nonclaimed(_)
    ));
    assert!(
        service
            .definition_and_bound_span("variable-jsdoc-service.js", reference)
            .expect_claimed("JSDoc variable definition")
            .is_some()
    );
}

#[test]
fn leading_jsdoc_on_property_assignment_defers_its_dependent_value() {
    let output = compile(&[(
        "documented-property-assignment.js",
        concat!(
            "function source(value) { return value; }",
            "const root = {};",
            "/** @type {(value: number) => number} */ root.invoke = source;",
            "root.invoke(); MissingSibling;",
        ),
    )]);
    assert_eq!(codes(&output), [2304], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn inline_jsdoc_casts_defer_variable_and_property_value_owners() {
    for (path, source) in [
        (
            "inline-variable-cast.js",
            concat!(
                "function source(value) { return value; }",
                "const alias = /** @type {(value: number) => number} */ (source);",
                "alias(); MissingSibling;",
            ),
        ),
        (
            "inline-property-cast.js",
            concat!(
                "function source(value) { return value; } const root = {};",
                "root.invoke = /** @type {(value: number) => number} */ (source);",
                "root.invoke(); MissingSibling;",
            ),
        ),
    ] {
        let output = compile(&[(path, source)]);
        assert_eq!(codes(&output), [2304], "{path}: {:#?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn only_parenthesized_type_tags_form_jsdoc_casts() {
    assert_complete(&compile(&[(
        "bare-inline-doc.js",
        concat!(
            "function source(value) { return value; }",
            "const alias = /** @type {(value: number) => number} */ source;",
            "alias();",
        ),
    )]));
    assert_complete(&compile(&[(
        "harmless-parenthesized-doc.js",
        concat!(
            "function source(value) { return value; }",
            "const alias = /** documentation only */ (source); alias();",
        ),
    )]));
    for tag in ["@typex {number}", "@satisfiesx {number}"] {
        let source = format!("const renamed = /** {tag} */ (1); renamed;");
        assert_complete(&compile(&[("tag-boundary.js", &source)]));
    }
}

#[test]
fn jsdoc_cast_scanner_recognizes_inline_tags_and_malformed_type_forms() {
    let prefix = "function source(value) { return value; }";
    for (path, doc) in [
        (
            "description-type.js",
            "description @type {(value:number)=>number}",
        ),
        (
            "two-tags.js",
            "description @deprecated old @type {(value:number)=>number}",
        ),
        (
            "two-typed-tags.js",
            "description @satisfies {(value:number)=>number} @type {(value:number)=>number}",
        ),
        (
            "multiline-type.js",
            "description\n * continued @type {(value:number)=>number}\n ",
        ),
        (
            "omitted-type-braces.js",
            "description @type (value:number)=>number",
        ),
        ("missing-type.js", "description @type"),
        ("invalid-type-braces.js", "description @type {"),
        (
            "omitted-satisfies-braces.js",
            "description @satisfies (value:number)=>number",
        ),
        ("invalid-satisfies-braces.js", "description @satisfies {"),
    ] {
        let source = format!("{prefix} const alias=/** {doc} */ (source); alias();");
        let output = compile(&[(path, &source)]);
        assert!(
            output.diagnostics.is_empty(),
            "{path}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
    }
}

#[test]
fn inline_jsdoc_casts_close_at_fresh_function_boundaries() {
    let output = compile(&[(
        "nested-inline-cast.js",
        concat!(
            "function source(value) { return value; }",
            "const outer = () => /** @type {(value: number) => number} */ (source)(MissingInside);",
            "outer(); MissingOutside;",
        ),
    )]);
    assert_eq!(codes(&output), [2304, 2304], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_complete(&compile(&[(
        "no-inline-cast.js",
        "function source(value) { return value; } const alias = (source); alias();",
    )]));
}

#[test]
fn inline_cast_owner_closure_preserves_independent_identifiers() {
    for (path, source, expected) in [
        (
            "direct-cast.js",
            "function source(value){return value} /** @type {(value:number)=>number} */ (source)(MissingIndependent);",
            1,
        ),
        (
            "return-cast.js",
            "function source(value){return value} const wrap=()=>{return /** @type {(value:number)=>number} */ (source)(MissingInside)}; wrap(); MissingOutside;",
            2,
        ),
        (
            "condition-cast.js",
            "function source(value){return value} if (/** @type {boolean} */ (source)) { MissingInside; } MissingOutside;",
            2,
        ),
        (
            "default-cast.js",
            "function source(value){return value} const wrap=(value=/** @type {(value:number)=>number} */ (source)(MissingDefault))=>value; wrap(); MissingOutside;",
            2,
        ),
    ] {
        let output = compile(&[(path, source)]);
        assert_eq!(
            codes(&output),
            vec![2304; expected],
            "{path}: {:#?}",
            output.diagnostics
        );
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn satisfies_casts_use_the_same_typed_owner_closure() {
    for (path, source) in [
        (
            "satisfies-mismatch.js",
            "const renamed = /** @satisfies {number} */ ('wrong'); renamed; MissingSibling;",
        ),
        (
            "satisfies-excess.js",
            "const renamed = /** @satisfies {{known:number}} */ ({known:1, extra:2}); renamed; MissingSibling;",
        ),
        (
            "satisfies-direct.js",
            "/** @satisfies {number} */ ('wrong'); MissingSibling;",
        ),
        (
            "satisfies-arrow.js",
            "const renamed=()=>/** @satisfies {number} */ ('wrong'); renamed(); MissingSibling;",
        ),
        (
            "satisfies-property.js",
            "function source(value){return value} const root={}; root.renamed=/** @satisfies {(value:number)=>number} */ (source); root.renamed(); MissingSibling;",
        ),
    ] {
        let output = compile(&[(path, source)]);
        assert_eq!(codes(&output), [2304], "{path}: {:#?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn nested_object_and_function_properties_form_one_dependency_closed_tree() {
    assert_complete(&compile(&[(
        "nested.js",
        concat!(
            "var namespace = {};",
            "namespace.commands = {};",
            "namespace.commands.count = 111;",
            "namespace.commands.run = function () {};",
            "namespace.commands.count;",
            "namespace.commands.run();",
        ),
    )]));
}

#[test]
fn function_property_can_own_a_later_named_property() {
    assert_complete(&compile(&[(
        "function-expando.js",
        concat!(
            "const root = {};",
            "root.callback = function () {};",
            "root.callback.metadata = {};",
            "root.callback.metadata;",
        ),
    )]));
}

#[test]
fn property_rhs_identity_covers_every_executable_expression_owner() {
    let prefix = concat!(
        "const root={}; function source(value){return value}",
        "function consume(value){return value}",
    );
    for (path, body) in [
        (
            "initializer-owner.js",
            "const captured=(root.invoke=source); root.invoke(); captured();",
        ),
        (
            "argument-owner.js",
            "consume(root.invoke=source); root.invoke();",
        ),
        (
            "return-owner.js",
            "function configure(){return root.invoke=1} configure(); root.invoke;",
        ),
        (
            "arrow-owner.js",
            "const configure=()=>root.invoke=source; configure(); root.invoke();",
        ),
        (
            "function-owner.js",
            "const configure=function(){root.invoke=source}; configure(); root.invoke();",
        ),
    ] {
        let source = format!("{prefix}{body}");
        let output = compile(&[(path, &source)]);
        assert_eq!(output.diagnostics, [], "{path}: {:#?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
    }
}

#[test]
fn repeated_cross_file_rhs_identity_is_order_independent() {
    let root = concat!(
        "var root={}; function source(value){return value}",
        "const captured=(root.invoke=source); captured();",
    );
    let extension = concat!(
        "function configure(){return root.invoke=1}",
        "configure(); root.invoke;",
    );
    for files in [
        [("a.js", root), ("b.js", extension)],
        [("a.js", extension), ("b.js", root)],
    ] {
        let output = compile(&files);
        assert_eq!(
            output.diagnostics,
            [],
            "{files:?}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{files:?}"
        );
    }
}

#[test]
fn exported_external_module_root_keeps_its_local_value_group() {
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "external.js",
            Arc::<str>::from(concat!(
                "// this is a javascript file...\n",
                "export const Adapter = {};\n",
                "Adapter.prop = {};\n",
                "// comment this out, and it works\n",
                "Adapter.asyncMethod = function () {};",
            )),
        )],
        &CompilerOptions {
            allow_js: true,
            check_js: Some(true),
            strict: false,
            strict_null_checks: Some(false),
            strict_property_initialization: Some(false),
            no_implicit_any: Some(false),
            // Diagnostic conformance invokes TSZ with `--noEmit`; CommonJS
            // function-expression emit remains an independently typed gap.
            no_emit: true,
            target: "es2015".to_string(),
            module: "commonjs".to_string(),
            out_dir: Some("dist".into()),
            ..CompilerOptions::default()
        },
    );
    assert_complete(&output);
}

#[test]
fn cross_file_property_identity_is_independent_of_root_order() {
    let declaration = ("a.js", "var shared = {}; shared.bucket = {};");
    let extension = ("b.js", "shared.bucket.value = 1; shared.bucket.value;");
    let forward = compile(&[declaration, extension]);
    let reverse = compile(&[extension, declaration]);
    assert_complete(&forward);
    assert_complete(&reverse);
    assert_eq!(forward.diagnostics, reverse.diagnostics);
    assert_eq!(forward.semantic_completion, reverse.semantic_completion);
}

#[test]
fn repeated_var_root_groups_share_properties_across_file_orders() {
    let declaration = ("a.js", "var shared = {}; var shared = {};");
    let extension = ("b.js", "shared.bucket = 1; shared.bucket;");
    assert_complete(&compile(&[declaration, extension]));
    assert_complete(&compile(&[extension, declaration]));
}

#[test]
fn repeated_function_root_groups_share_properties_across_file_orders() {
    let first = ("a.js", "function shared() {}");
    let second = ("b.js", "function shared() {}");
    let extension = ("c.js", "shared.bucket = 1; shared.bucket; shared();");
    let forward = compile(&[first, second, extension]);
    let reverse = compile(&[extension, second, first]);
    assert!(
        !codes(&forward).contains(&2339),
        "{:#?}",
        forward.diagnostics
    );
    assert!(
        !codes(&reverse).contains(&2339),
        "{:#?}",
        reverse.diagnostics
    );
    assert_eq!(forward.diagnostics, reverse.diagnostics);
    assert_eq!(forward.semantic_completion, reverse.semantic_completion);
}

#[test]
fn repeated_property_assignments_share_one_canonical_property() {
    assert_complete(&compile(&[(
        "repeated.js",
        "let record = {}; record.value = 1; record.value = 2; record.value;",
    )]));
}

#[test]
fn a_jsdoc_peer_defers_the_whole_repeated_property_value_group() {
    let ordinary = concat!(
        "function source(value) { return value; }",
        "var root = {}; root.invoke = source;",
    );
    let documented = concat!(
        "var root = {};",
        "/** @type {(value: number) => number} */ root.invoke = source;",
        "root.invoke(); root.invoke(); MissingSibling;",
    );
    for files in [
        [("a.js", ordinary), ("b.js", documented)],
        [("a.js", documented), ("b.js", ordinary)],
    ] {
        let output = compile(&files);
        assert_eq!(codes(&output), [2304], "{:#?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    }
}

#[test]
fn equal_property_spelling_does_not_merge_distinct_roots() {
    assert_complete(&compile(&[(
        "distinct.js",
        concat!(
            "const left = {}; const right = {};",
            "left.value = 1;",
            "right.value = function () {};",
            "left.value; right.value();",
        ),
    )]));
}

#[test]
fn expando_property_services_fail_closed_at_exact_member_nodes() {
    let source = concat!(
        "const left = {}; const right = {};",
        "left.bucket = {}; left.bucket.value = 1; left.bucket.value = 2;",
        "right.value = 3; left.bucket.value; right.value;",
    );
    let mut service = LanguageService::new(CompilerOptions {
        allow_js: true,
        check_js: Some(true),
        no_emit: true,
        ..CompilerOptions::default()
    });
    service.open("members.js", Arc::<str>::from(source));
    let files = ["members.js".to_string()];
    assert!(
        service
            .quick_info("members.js", source.find("left").unwrap() as u32)
            .expect_claimed("expando root quick info")
            .is_some(),
        "the renamed root declaration remains independently claimed",
    );
    for (needle, start) in [
        ("left.bucket", source.find("left.bucket").unwrap()),
        (
            "left.bucket.value",
            source.find("left.bucket.value").unwrap(),
        ),
        ("right.value", source.find("right.value").unwrap()),
        (
            "left.bucket.value",
            source.rfind("left.bucket.value").unwrap(),
        ),
        ("right.value", source.rfind("right.value").unwrap()),
    ] {
        let start = start as u32;
        let property = start + needle.rfind('.').unwrap() as u32 + 1;
        assert_property_services_nonclaimed(&service, "members.js", property, &files);
        assert!(
            service
                .definition_and_bound_span("members.js", start)
                .expect_claimed("expando root definition")
                .is_some(),
            "the root binder identity remains independently claimed",
        );
    }
}

#[test]
fn cross_file_repeated_root_property_service_nonclaims_are_order_independent() {
    for (declaration_path, use_path) in [("a.js", "b.js"), ("b.js", "a.js")] {
        let declaration =
            "var shared = {}; var shared = {}; shared.bucket = {}; shared.bucket.value = 1;";
        let usage = "shared.bucket.value;";
        let mut service = LanguageService::new(CompilerOptions {
            allow_js: true,
            check_js: Some(true),
            no_emit: true,
            ..CompilerOptions::default()
        });
        service.open(declaration_path, Arc::<str>::from(declaration));
        service.open(use_path, Arc::<str>::from(usage));
        let files = [declaration_path.to_string(), use_path.to_string()];
        for (path, source) in [(declaration_path, declaration), (use_path, usage)] {
            for (property, _) in source
                .match_indices("bucket")
                .chain(source.match_indices("value"))
            {
                assert_property_services_nonclaimed(&service, path, property as u32, &files);
            }
        }
    }
}

#[test]
fn incomplete_and_unresolved_javascript_members_are_service_nonclaimed() {
    for (path, source, bound_root) in [
        ("unresolved.js", "missing.value=1; missing.value;", None),
        (
            "noneligible.js",
            "const rooted={known:1}; rooted.value=1; rooted.value;",
            Some("rooted"),
        ),
    ] {
        let mut service = LanguageService::new(CompilerOptions {
            allow_js: true,
            check_js: Some(true),
            no_emit: true,
            ..CompilerOptions::default()
        });
        service.open(path, Arc::<str>::from(source));
        let files = [path.to_string()];
        for (property, _) in source.match_indices("value") {
            assert_property_services_nonclaimed(&service, path, property as u32, &files);
        }
        if let Some(root) = bound_root {
            let root = source.find(root).unwrap() as u32;
            assert!(matches!(
                service.quick_info(path, root),
                ServiceQuery::Nonclaimed(_)
            ));
            assert!(
                service
                    .definition_and_bound_span(path, root)
                    .expect_claimed("JavaScript root definition")
                    .is_some(),
                "the root binder identity remains independently claimed",
            );
            assert!(matches!(
                service.references(path, root),
                ServiceQuery::Nonclaimed(_)
            ));
            assert!(
                !service
                    .document_highlights(path, root, &files)
                    .expect_claimed("JavaScript root highlights")
                    .is_empty()
            );
            assert!(
                service
                    .rename(path, root)
                    .expect_claimed("JavaScript root rename")
                    .info
                    .can_rename
            );
        } else {
            let root = source.find("missing").unwrap() as u32;
            assert!(
                service
                    .quick_info(path, root)
                    .expect_claimed("unresolved JavaScript root quick info")
                    .is_none()
            );
            assert!(
                service
                    .definition_and_bound_span(path, root)
                    .expect_claimed("unresolved JavaScript root definition")
                    .is_none()
            );
            assert!(
                service
                    .references(path, root)
                    .expect_claimed("unresolved JavaScript root references")
                    .is_empty()
            );
            assert!(
                service
                    .document_highlights(path, root, &files)
                    .expect_claimed("unresolved JavaScript root highlights")
                    .is_empty()
            );
            let rename = service
                .rename(path, root)
                .expect_claimed("unresolved JavaScript root rename");
            assert!(!rename.info.can_rename);
            assert!(rename.locations.is_empty());
        }
    }
}

#[test]
fn block_and_function_nesting_preserve_the_lexical_root_identity() {
    assert_complete(&compile(&[(
        "scoped.js",
        concat!(
            "function configure() {",
            "  const local = {};",
            "  { local.setting = 1; local.setting; }",
            "}",
            "configure();",
        ),
    )]));
}

#[test]
fn parenthesized_receiver_and_assignment_keep_the_same_identity() {
    assert_complete(&compile(&[(
        "wrapped.js",
        "const holder = {}; (((holder).value = 1)); holder.value;",
    )]));
}

#[test]
fn typescript_property_assignment_keeps_the_ordinary_negative_diagnostic() {
    let output = compile(&[(
        "negative.ts",
        "const typed = {}; typed.value = 1; typed.value;",
    )]);
    assert!(codes(&output).contains(&2339), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn assignments_without_a_property_producer_use_ordinary_semantics() {
    for (path, source, expected) in [
        ("unresolved.js", "missing.value = 1;", &[2304][..]),
        (
            "renamed-unresolved.js",
            "absentRenamed.deep.value = 1;",
            &[2304][..],
        ),
        ("negative.js", "const target = {}; target[-1] = 1;", &[][..]),
        ("wrapped.js", "const target = {}; target[(0)] = 1;", &[][..]),
        (
            "computed.js",
            "const renamed = {}; const index = 0; renamed[index] = 1;",
            &[][..],
        ),
    ] {
        let output = compile(&[(path, source)]);
        assert_eq!(
            codes(&output),
            expected,
            "{path}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{path}"
        );
    }
}

#[test]
fn canonical_element_property_assignments_remain_fail_closed() {
    for (path, source) in [
        (
            "string.js",
            "const target = {}; target['value'] = 1; target['value'];",
        ),
        (
            "number.js",
            "const renamed = {}; renamed[0] = 1; renamed[0];",
        ),
    ] {
        let output = compile(&[(path, source)]);
        assert!(
            !codes(&output).contains(&2339),
            "{path}: {:#?}",
            output.diagnostics
        );
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{path}"
        );
    }
}

#[test]
fn noneligible_object_initializer_remains_fail_closed() {
    let output = compile(&[(
        "nonempty.js",
        "const target = { present: 1 }; target.value = 1; target.value;",
    )]);
    assert!(!codes(&output).contains(&2339), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn call_receiver_assignment_remains_fail_closed() {
    let output = compile(&[(
        "call-receiver.js",
        "function make() { return {}; } make().value = 1;",
    )]);
    assert!(!codes(&output).contains(&2339), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn commonjs_shaped_roots_are_not_claimed_as_local_expando_objects() {
    let output = compile(&[(
        "commonjs.js",
        "exports.value = 1; module.exports.value = 2; require('x').value = 3;",
    )]);
    assert!(!codes(&output).contains(&2339), "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn incomplete_target_still_checks_an_independent_rhs_diagnostic() {
    let output = compile(&[(
        "independent.js",
        "const target = {}; target['value'] = absent;",
    )]);
    assert_eq!(codes(&output), [2304], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn complete_target_still_checks_an_independent_rhs_diagnostic_once() {
    let output = compile(&[(
        "complete-independent.js",
        "const target = {}; target.value = absent; target.value;",
    )]);
    assert_eq!(codes(&output), [2304], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}
