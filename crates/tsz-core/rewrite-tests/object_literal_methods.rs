use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    ExpressionKind, FunctionLikeFunctionKind, FunctionLikeSyntax, StatementKind, parse_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn parse(source: &str) -> tsz::syntax::ParseOutput {
    let source = SourceText::new(
        FileId(0),
        "object-literal-method.ts".into(),
        Arc::<str>::from(source),
    );
    parse_source(&source)
}

fn compile(source: &str, no_check: bool, target: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new(
            "object-literal-method.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            target: target.to_string(),
            no_emit: !no_check,
            no_check,
            strict: true,
            ..CompilerOptions::default()
        },
    )
}

fn assert_services_nonclaimed(
    service: &LanguageService,
    path: &str,
    offset: u32,
    files: &[String],
) {
    assert!(service.quick_info(path, offset).is_none());
    assert!(service.definition_and_bound_span(path, offset).is_none());
    assert!(service.references(path, offset).is_empty());
    assert!(service.document_highlights(path, offset, files).is_empty());
    let rename = service.rename(path, offset);
    assert!(!rename.info.can_rename);
    assert!(rename.locations.is_empty());
}

#[test]
fn generic_object_methods_retain_signatures_bodies_and_property_fallbacks() {
    let source = concat!(
        "const sibling = 1;\n",
        "const holder = {\n",
        "  renamed<Value extends number>(input: Value): Value { return input; },\n",
        "  nested: { wrapped<Item>(item: Item) { return item; } },\n",
        "  outer<Outer>(value: Outer) { return { inner<Inner>(item: Inner): Outer { return value; } }; },\n",
        "  arrow: (value: number): number => value,\n",
        "  sibling,\n",
        "};\n",
    );
    let parsed = parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

    let Some(StatementKind::Variable(holder)) = parsed
        .unit
        .statements
        .get(1)
        .map(|statement| &statement.kind)
    else {
        panic!("expected holder declaration");
    };
    let ExpressionKind::Object(properties) = &holder
        .initializer
        .as_ref()
        .expect("holder initializer")
        .kind
    else {
        panic!("expected holder object");
    };
    assert_eq!(properties.len(), 5);

    let ExpressionKind::FunctionLike(generic) = &properties[0].value.kind else {
        panic!("expected generic method");
    };
    assert_eq!(properties[0].name, "renamed");
    assert!(!properties[0].shorthand);
    assert_eq!(generic.type_parameters.len(), 1);
    assert_eq!(generic.type_parameters[0].name, "Value");
    assert_eq!(generic.parameters.len(), 1);
    assert_eq!(generic.parameters[0].name, "input");
    assert!(generic.return_type.is_some());
    assert!(matches!(
        &generic.syntax,
        FunctionLikeSyntax::Function {
            kind: FunctionLikeFunctionKind::ObjectMethod,
            body,
            ..
        } if body.len() == 1
    ));

    let ExpressionKind::Object(nested) = &properties[1].value.kind else {
        panic!("expected nested object");
    };
    assert!(matches!(
        &nested[0].value.kind,
        ExpressionKind::FunctionLike(function)
            if matches!(
                &function.syntax,
                FunctionLikeSyntax::Function {
                    kind: FunctionLikeFunctionKind::ObjectMethod,
                    ..
                }
            )
                && function.type_parameters[0].name == "Item"
    ));
    assert!(matches!(
        &properties[2].value.kind,
        ExpressionKind::FunctionLike(function)
            if matches!(
                &function.syntax,
                FunctionLikeSyntax::Function {
                    kind: FunctionLikeFunctionKind::ObjectMethod,
                    ..
                }
            )
                && function.type_parameters[0].name == "Outer"
    ));
    assert!(matches!(
        &properties[3].value.kind,
        ExpressionKind::FunctionLike(function)
            if matches!(&function.syntax, FunctionLikeSyntax::Arrow(_))
    ));
    assert!(properties[4].shorthand);
    assert!(matches!(
        &properties[4].value.kind,
        ExpressionKind::Identifier { name, .. } if name == "sibling"
    ));
}

#[test]
fn outer_generic_function_keeps_object_method_type_parameters_structural() {
    let source = concat!(
        "function make<Events extends Record<PropertyKey, unknown>>() {\n",
        "  const all = {\n",
        "    on<Key extends keyof Events>(type: Key, value: Events[Key]) { return value; },\n",
        "  };\n",
        "  return all;\n",
        "}\n",
    );
    let parsed = parse(source);
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let StatementKind::Function(make) = &parsed.unit.statements[0].kind else {
        panic!("expected outer function");
    };
    assert_eq!(make.type_parameters[0].name, "Events");
    let StatementKind::Variable(all) = &make.body[0].kind else {
        panic!("expected local object");
    };
    let ExpressionKind::Object(properties) = &all.initializer.as_ref().expect("initializer").kind
    else {
        panic!("expected object literal");
    };
    let ExpressionKind::FunctionLike(on) = &properties[0].value.kind else {
        panic!("expected method");
    };
    assert_eq!(on.type_parameters[0].name, "Key");
    assert!(matches!(
        &on.syntax,
        FunctionLikeSyntax::Function {
            kind: FunctionLikeFunctionKind::ObjectMethod,
            ..
        }
    ));
    let output = compile(source, false, "es2022");
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn es2015_javascript_emit_preserves_generic_method_syntax_while_erasing_types() {
    let source = concat!(
        "const renamedHub = {\n",
        "  /** kept jsdoc */\n",
        "  subscribe<Value extends number>(value: Value): Value { return value; },\n",
        "  // kept line\n",
        "  second<Item>(item: Item): Item { return item; },\n",
        "  arrow: (value: number): number => value,\n",
        "};\n",
    );
    let output = compile(source, true, "es2015");
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("JavaScript output");
    assert_eq!(
        javascript.text,
        concat!(
            "\"use strict\";\n",
            "const renamedHub = {\n",
            "    /** kept jsdoc */\n",
            "    subscribe(value) { return value; },\n",
            "    // kept line\n",
            "    second(item) { return item; },\n",
            "    arrow: (value) => value,\n",
            "};\n",
        )
    );
}

#[test]
fn generic_object_method_is_a_typed_local_nonclaim_without_diagnostics() {
    let source = concat!(
        "const holder = { renamed<Value>(value: Value): Value { return value; } };\n",
        "const independent: MissingSibling = 1;\n",
    );
    let output = compile(source, false, "es2022");
    let [diagnostic] = output.diagnostics.as_slice() else {
        panic!("unexpected diagnostics: {:#?}", output.diagnostics);
    };
    assert_eq!(diagnostic.code, 2304);
    assert_eq!(
        diagnostic.start,
        source.find("MissingSibling").unwrap() as u32
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn nongeneric_and_malformed_method_shapes_remain_parser_recovery() {
    let nongeneric = "const holder = { renamed(value: number) { return value; } };\n";
    let parsed = parse(nongeneric);
    assert!(!parsed.diagnostics.is_empty());
    assert!(compile(nongeneric, true, "es2015").emitted_files.is_empty());

    let private = "const holder = { #secret<Value>(value: Value) { return value; } };\n";
    let private_output = compile(private, true, "es2015");
    assert_eq!(
        private_output.semantic_completion,
        SemanticCompletion::Deferred
    );
    assert!(private_output.emitted_files.is_empty());

    let malformed = concat!(
        "const holder = {\n",
        "  renamed<Value>(value: Value);,\n",
        "  sibling: MissingSibling,\n",
        "};\n",
        "const independent: MissingIndependent = 1;\n",
    );
    let output = compile(malformed, false, "es2022");
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.file.as_str(),
                    diagnostic.code,
                    diagnostic.start,
                    diagnostic.length,
                    diagnostic.message_text.as_str(),
                    diagnostic.category,
                    diagnostic.related_information.as_slice(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            (
                "object-literal-method.ts",
                1005,
                47,
                1,
                "'{' expected.",
                DiagnosticCategory::Error,
                &[][..],
            ),
            (
                "object-literal-method.ts",
                2304,
                61,
                14,
                "Cannot find name 'MissingSibling'.",
                DiagnosticCategory::Error,
                &[][..],
            ),
            (
                "object-literal-method.ts",
                2304,
                99,
                18,
                "Cannot find name 'MissingIndependent'.",
                DiagnosticCategory::Error,
                &[][..],
            ),
        ]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert!(output.emitted_files.is_empty());
}

#[test]
fn object_method_emit_fails_closed_for_authored_erased_comments() {
    let source =
        "const holder = { renamed /* erased */ <Value>(value: Value) { return value; } };\n";
    let output = compile(source, true, "es2015");
    assert_eq!(output.diagnostics, [], "{:#?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(output.emitted_files.is_empty());
}

#[test]
fn object_method_navigation_waits_for_program_owned_member_identity() {
    let same_file = concat!(
        "const holder = { renamed<Value>(value: Value) { return value; } };\n",
        "holder.renamed;\n",
    );
    let mut same_service = LanguageService::new(CompilerOptions::default());
    same_service.open("same.ts", Arc::<str>::from(same_file));
    let same_files = ["same.ts".to_string()];
    let same_use = same_file.rfind("renamed").expect("same-file use") as u32;
    assert_services_nonclaimed(&same_service, "same.ts", same_use + 1, &same_files);

    let declaration = "const shared = { renamedCross<Value>(value: Value) { return value; } };\n";
    let usage = "shared.renamedCross;\n";
    let mut cross_service = LanguageService::new(CompilerOptions::default());
    cross_service.open("a.ts", Arc::<str>::from(declaration));
    cross_service.open("b.ts", Arc::<str>::from(usage));
    let cross_files = ["a.ts".to_string(), "b.ts".to_string()];
    let cross_use = usage.find("renamedCross").expect("cross-file use") as u32;
    assert_services_nonclaimed(&cross_service, "b.ts", cross_use + 1, &cross_files);

    let control = "const independent: number = 1; independent;";
    let mut control_service = LanguageService::new(CompilerOptions::default());
    control_service.open("control.ts", Arc::<str>::from(control));
    let control_files = ["control.ts".to_string()];
    let reference = control.find("independent").expect("control declaration") as u32;
    assert!(
        control_service
            .quick_info("control.ts", reference + 1)
            .is_some()
    );
    assert!(
        control_service
            .definition_and_bound_span("control.ts", reference + 1)
            .is_some()
    );
    assert!(
        !control_service
            .references("control.ts", reference + 1)
            .is_empty()
    );
    assert!(
        !control_service
            .document_highlights("control.ts", reference + 1, &control_files)
            .is_empty()
    );
    assert!(
        control_service
            .rename("control.ts", reference + 1)
            .info
            .can_rename
    );
}
