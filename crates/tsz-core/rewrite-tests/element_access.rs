use std::path::PathBuf;
use std::sync::Arc;

use tsz::diagnostics::DiagnosticCategory;
use tsz::service::LanguageService;
use tsz::source::{FileId, SourceText};
use tsz::syntax::{
    Expression, ExpressionKind, Literal, StatementKind, StringLiteral, parse_source,
};
use tsz::{CompileExitStatus, Compiler, CompilerOptions, SemanticCompletion, SourceInput};

fn compile(source: &str, strict: bool, no_emit: bool) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "es2015".to_string(),
            strict,
            no_emit,
            ..CompilerOptions::default()
        },
    )
}

fn assert_complete(source: &str) {
    let output = compile(source, true, true);
    assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
    assert_eq!(
        output.semantic_completion,
        SemanticCompletion::Complete,
        "{source}"
    );
    assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
}

fn parsed_expression_shape(source: &str) -> String {
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("syntax.ts"),
        Arc::<str>::from(source),
    ));
    assert_eq!(parsed.diagnostics, [], "{source}: {:?}", parsed.diagnostics);
    let [statement] = parsed.unit.statements.as_slice() else {
        panic!("expected one expression statement: {source}");
    };
    let StatementKind::Expression(expression) = &statement.kind else {
        panic!("expected an expression statement: {source}");
    };
    expression_shape(expression)
}

fn parsed_diagnostic_codes(source: &str) -> Vec<u32> {
    parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("syntax.ts"),
        Arc::<str>::from(source),
    ))
    .diagnostics
    .iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

fn expression_shape(expression: &Expression) -> String {
    match &expression.kind {
        ExpressionKind::Identifier { name, .. } => name.clone(),
        ExpressionKind::Literal(Literal::Number(number)) => number.raw().to_string(),
        ExpressionKind::Literal(Literal::String(StringLiteral::Plain(value))) => {
            format!("{value:?}")
        }
        ExpressionKind::Literal(Literal::Null) => "null".to_string(),
        ExpressionKind::Member { object, name, .. } => {
            format!("Member({}, {name})", expression_shape(object))
        }
        ExpressionKind::ElementAccess { object, index } => format!(
            "Element({}, {})",
            expression_shape(object),
            expression_shape(index)
        ),
        ExpressionKind::Object(properties) => format!(
            "Object({})",
            properties
                .iter()
                .map(|property| format!(
                    "{}{}{}",
                    property.name,
                    if property.shorthand { "~" } else { ":" },
                    expression_shape(&property.value)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExpressionKind::Assignment { left, right, .. } => format!(
            "Assign({}, {})",
            expression_shape(left),
            expression_shape(right)
        ),
        ExpressionKind::New {
            callee,
            type_arguments,
            arguments,
        } => format!(
            "New<{}>({}, [{}])",
            type_arguments.len(),
            expression_shape(callee),
            expression_shapes(arguments)
        ),
        ExpressionKind::Call {
            callee,
            type_arguments,
            arguments,
        } => format!(
            "Call<{}>({}, [{}])",
            type_arguments.as_ref().map_or(0, Vec::len),
            expression_shape(callee),
            expression_shapes(arguments)
        ),
        ExpressionKind::Parenthesized(inner) => format!("Paren({})", expression_shape(inner)),
        ExpressionKind::As { expression, .. } => format!("As({})", expression_shape(expression)),
        kind => panic!("unexpected expression shape: {kind:?}"),
    }
}

fn expression_shapes(expressions: &[Expression]) -> String {
    expressions
        .iter()
        .map(expression_shape)
        .collect::<Vec<_>>()
        .join(", ")
}

fn unchecked_compile(source: &str) -> tsz::CompileOutput {
    Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "es2015".to_string(),
            strict: true,
            no_check: true,
            ..CompilerOptions::default()
        },
    )
}

fn unchecked_javascript(source: &str) -> String {
    let output = unchecked_compile(source);
    assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
    output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .unwrap_or_else(|| panic!("missing JavaScript output for {source}"))
        .text
        .clone()
}

#[test]
fn element_access_emit_preserves_internal_comments_once_in_source_order() {
    let source = concat!(
        "/*0*/ Array /*1*/[ /*2*/ \"toString\" /*3*/ ] /*4*/; /*5*/\n\n",
        "/*0*/ Array \n",
        "    // single line\n",
        "    /*1*/[ /*2*/ \"toString\"\n",
        "    // single line\n",
        "    /*3*/ ] /*4*/",
    );
    assert_eq!(
        unchecked_javascript(source),
        concat!(
            "\"use strict\";\n",
            "/*0*/ Array /*1*/[ /*2*/\"toString\" /*3*/] /*4*/; /*5*/\n",
            "/*0*/ Array\n",
            "// single line\n",
            "/*1*/ [ /*2*/\"toString\"\n",
            "// single line\n",
            "/*3*/ ]; /*4*/\n",
        )
    );
}

#[test]
fn element_access_comment_emit_respects_erasure_removal_and_nested_boundaries() {
    let erased = concat!(
        "/*before-type*/ type Hidden = string; /*after-type*/\n",
        "/*lead*/ renamed /*before-open*/[ /*after-open*/ key /*before-close*/ ] ",
        "/*after-close*/; /*after-semi*/\n",
        "/*next-leading*/ other[/*inside*/ key];",
    );
    assert_eq!(
        unchecked_javascript(erased),
        concat!(
            "\"use strict\";\n",
            "/*lead*/ renamed /*before-open*/[ /*after-open*/key /*before-close*/] ",
            "/*after-close*/; /*after-semi*/\n",
            "/*next-leading*/ other[ /*inside*/key];\n",
        )
    );

    let nested = concat!(
        "renamed /*outer-open*/[ /*outer-index*/ nested ",
        "/*inner-open*/[ /*inner-index*/ key /*inner-close*/ ] ",
        "/*outer-close*/ ] /*end*/;",
    );
    assert_eq!(
        unchecked_javascript(nested),
        concat!(
            "\"use strict\";\n",
            "renamed /*outer-open*/[ /*outer-index*/nested ",
            "/*inner-open*/[ /*inner-index*/key /*inner-close*/] ",
            "/*outer-close*/] /*end*/;\n",
        )
    );

    let removed = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("/*lead*/ renamed /*open*/[/*index*/ key /*close*/]; /*tail*/"),
        )],
        &CompilerOptions {
            target: "es2015".to_string(),
            no_check: true,
            remove_comments: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(removed.diagnostics, []);
    assert_eq!(
        removed
            .emitted_files
            .iter()
            .find(|file| !file.declaration)
            .expect("JavaScript output")
            .text,
        "\"use strict\";\nrenamed[key];\n"
    );

    let module = Compiler::new().compile(
        vec![SourceInput::new(
            "case.ts",
            Arc::<str>::from("/*lead*/ export const value=renamed[/*index*/key];"),
        )],
        &CompilerOptions {
            target: "es2015".to_string(),
            module: "esnext".to_string(),
            no_check: true,
            ..CompilerOptions::default()
        },
    );
    assert_eq!(module.diagnostics, []);
    assert_eq!(
        module
            .emitted_files
            .iter()
            .find(|file| !file.declaration)
            .expect("JavaScript output")
            .text,
        "/*lead*/ export const value = renamed[ /*index*/key];\n"
    );
}

#[test]
fn constructor_member_access_has_exact_ast_grouping_and_javascript_emit() {
    for (source, expected_shape, emitted_expression) in [
        (
            "new Foo[1]();",
            "New<0>(Element(Foo, 1), [])",
            "new Foo[1]();",
        ),
        (
            "new ns.Renamed['ctor']();",
            "New<0>(Element(Member(ns, Renamed), \"ctor\"), [])",
            "new ns.Renamed['ctor']();",
        ),
        (
            "(new Foo())[1]();",
            "Call<0>(Element(Paren(New<0>(Foo, [])), 1), [])",
            "(new Foo())[1]();",
        ),
        (
            "new Foo().member();",
            "Call<0>(Member(New<0>(Foo, []), member), [])",
            "new Foo().member();",
        ),
        (
            "new C[0].m(a);",
            "New<0>(Member(Element(C, 0), m), [a])",
            "new C[0].m(a);",
        ),
        (
            "new C[0]().m[1](a);",
            "Call<0>(Element(Member(New<0>(Element(C, 0), []), m), 1), [a])",
            "new C[0]().m[1](a);",
        ),
    ] {
        assert_eq!(parsed_expression_shape(source), expected_shape, "{source}");
        assert_eq!(
            unchecked_javascript(source),
            format!("\"use strict\";\n{emitted_expression}\n"),
            "{source}"
        );
    }
}

#[test]
fn postfix_non_null_recovery_keeps_the_erased_receiver_and_later_suffixes() {
    for (source, expected_shape, emitted) in [
        ("value!;", "value", "value;"),
        (
            "value!.property;",
            "Member(value, property)",
            "value.property;",
        ),
        ("value![0];", "Element(value, 0)", "value[0];"),
        ("value!();", "Call<0>(value, [])", "value();"),
        ("value!!;", "value", "value;"),
        (
            r#"null! as { [K in keyof number[] as Exclude<K,"length">]: (number[])[K] };"#,
            "As(null)",
            "null;",
        ),
    ] {
        assert_eq!(parsed_expression_shape(source), expected_shape, "{source}");
        assert_eq!(
            unchecked_compile(source).semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_eq!(
            unchecked_javascript(source),
            format!("\"use strict\";\n{emitted}\n")
        );
    }
    let newline = "value\n!other;";
    let parsed = parse_source(&SourceText::new(
        FileId(0),
        PathBuf::from("newline.ts"),
        Arc::<str>::from(newline),
    ));
    assert_eq!(parsed.diagnostics, []);
    assert_eq!(parsed.unit.statements.len(), 2);
}

#[test]
fn nested_new_preserves_the_inner_element_callee_and_omitted_outer_argument_list() {
    assert_eq!(
        parsed_expression_shape("new new C[0]();"),
        "New<0>(New<0>(Element(C, 0), []), [])"
    );
}

#[test]
fn constructor_element_callee_precedes_type_arguments_without_broadening_generic_emit() {
    let source = "new C[0]<T>(a);";
    assert_eq!(
        parsed_expression_shape(source),
        "New<1>(Element(C, 0), [a])"
    );
    let output = unchecked_compile(source);
    assert_eq!(output.diagnostics, []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    assert!(output.emitted_files.is_empty());
}

#[test]
fn parser_object_creation_array_literal_4_keeps_the_element_access_in_the_new_callee() {
    let source = "new Foo[1]();";
    assert_eq!(
        parsed_expression_shape(source),
        "New<0>(Element(Foo, 1), [])"
    );
    assert_eq!(
        unchecked_javascript(source),
        "\"use strict\";\nnew Foo[1]();\n"
    );
}

#[test]
fn direct_renamed_parenthesized_and_cross_line_array_accesses_are_complete() {
    assert_complete(concat!(
        "let values:number[]=[1];",
        "const direct:number=values[0];",
        "const renamed=values;",
        "(renamed)\n[(0)]=2;",
        "const wrapped:number=((renamed)[(0)]);",
    ));
}

#[test]
fn shorthand_default_has_exact_cover_grammar_shape_and_javascript_emit() {
    let source = "({renamed = 1}=source);";
    assert_eq!(
        parsed_expression_shape(source),
        "Paren(Assign(Object(renamed~Assign(renamed, 1)), source))"
    );
    assert_eq!(
        unchecked_javascript(source),
        "\"use strict\";\n({ renamed = 1 } = source);\n"
    );

    for (source, emitted) in [
        ("({renamed}=source);", "({ renamed } = source);"),
        (
            "({sourceKey: renamed = 1}=source);",
            "({ sourceKey: renamed = 1 } = source);",
        ),
        (
            "let renamed=0;const value={renamed: renamed = 1};",
            "let renamed = 0;\nconst value = { renamed: renamed = 1 };",
        ),
    ] {
        assert_eq!(
            unchecked_javascript(source),
            format!("\"use strict\";\n{emitted}\n"),
            "{source}"
        );
    }
}

#[test]
fn destructuring_assignment_targets_are_recursive_and_keep_the_rhs_value() {
    for source in [
        "function test(p:any){'use strict';'use strong';p={prop:p}=p;}",
        "var a:any;var x:string;[x]=a;",
        "var a:any;({}=a);([]=a);",
        "var a:any;({}={} = a);([]=[] = a);",
        "function qux(bar:{value:number}){let foo:number;({value:foo}=bar);let x=()=>bar;}",
        concat!(
            "let renamed=0;let wrapped=0;let nested=0;",
            "([renamed,(wrapped)]=[1,2]);",
            "({outer:{value:((nested))}}={outer:{value:3}});",
            "const result:{value:number}=({value:renamed}={value:4});",
        ),
        "let renamed=0;const source={renamed:2};({renamed = 1}=source);",
        concat!(
            "let renamed=0;let wrapped=0;",
            "const source={outer:{renamed:2}};",
            "({outer:{renamed = ((wrapped = 1))}}=source);",
        ),
        concat!(
            "let renamed=0;let nested=0;",
            "const source={outer:[2]};",
            "({outer:[renamed = ((nested = 1))]}=source);",
        ),
        concat!(
            "let renamed=0;const source={sourceKey:2};",
            "({sourceKey: renamed = 1}=source);",
        ),
    ] {
        let output = compile(source, true, false);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
        assert!(
            output.emitted_files.iter().any(|file| !file.declaration),
            "{source}"
        );
    }

    for source in [
        "({value:1}={value:2});",
        "[1]=[2];",
        "let renamed=0;(renamed=1)=2;",
        "let kept:number=0;({valid:kept,invalid:1}={valid:'wrong',invalid:2});",
        "let kept:number=0;[kept,1]=['wrong',2];",
    ] {
        let output = compile(source, true, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_eq!(
            output.exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{source}"
        );
    }
}

#[test]
fn shorthand_default_relates_the_default_expression_to_the_assignment_target() {
    let source = concat!(
        "let renamed:string='';const source={renamed:'kept'};",
        "({renamed = ((1))}=source);",
    );
    let output = compile(source, true, true);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
            .collect::<Vec<_>>(),
        vec![(2322, "Type 'number' is not assignable to type 'string'.")]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn optional_source_default_does_not_manufacture_an_outer_assignment_error() {
    let source = concat!(
        "const source:{renamed?:number}={};let renamed=0;",
        "({renamed = 1}=source);",
    );
    let output = compile(source, true, true);
    assert_eq!(output.diagnostics, []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn shorthand_default_is_rejected_only_in_an_ordinary_object_literal() {
    let source = "let renamed=0;const invalid={renamed = 1};";
    let equals = source.rfind("= 1").expect("shorthand equals") as u32;
    let output = compile(source, true, true);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![(
            1312,
            equals,
            1,
            "Did you mean to use a ':'? An '=' can only follow a property name when the containing object literal is part of a destructuring pattern."
        )]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);

    let mut service = LanguageService::new(CompilerOptions {
        target: "es2015".to_string(),
        strict: true,
        no_emit: true,
        ..CompilerOptions::default()
    });
    service.open("case.ts", Arc::<str>::from(source));
    assert_eq!(
        service
            .syntactic_diagnostics("case.ts")
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![1312]
    );

    assert_complete("let renamed=0;const valid={renamed: renamed = 1};");
}

#[test]
fn heterogeneous_assignment_targets_fail_closed_without_an_exact_syntax_pair() {
    for source in [
        "declare const values:[number,string];let count:number=0;let label:string='';[count,label]=values;",
        "declare const values:{0:number;1:string};let count=0;let label='';[count,label]=values;",
        "function assign<T>(values:T){let count=0;let label='';[count,label]=values;}",
        "let count:number=0;let label:string='';[count,label]=[1];",
        "let count=0;let label='';[count,{label}]=[1,{other:'ok'}];",
    ] {
        let output = compile(source, true, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_eq!(
            output.exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{source}"
        );
    }
}

#[test]
fn heterogeneous_array_assignment_targets_have_exact_positional_relations() {
    assert_complete("let first:number=0;let second:string='';[first,second]=[1,'ok'];");
    for source in [
        concat!(
            "let count:number=0;let label:string='';",
            "const kept:(number|string)[]=([((count)),(label)]=[(1),(('ok'))]);",
        ),
        concat!(
            "let count:number=0;let label:string='';",
            "({right:label,left:count}={left:1,right:'ok'});",
        ),
        concat!(
            "let count:number=0;let label:string='';",
            "({pair:[count,(label)]}={pair:[1,('ok')]});",
        ),
    ] {
        assert_complete(source);
    }

    let source = "let first:number=0;let second:string='';[first,second]=[1,2];";
    let output = compile(source, true, true);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.file.as_str(),
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.category,
                diagnostic.message_text.as_str(),
                diagnostic.related_information.as_slice(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            "case.ts",
            2322,
            source.rfind("second").unwrap() as u32,
            "second".len() as u32,
            DiagnosticCategory::Error,
            "Type 'number' is not assignable to type 'string'.",
            &[][..],
        )]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );

    let source = "let first:number=0;let second:string='';[first,second]=['bad',2];";
    let output = compile(source, true, true);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.start, diagnostic.length))
            .collect::<Vec<_>>(),
        vec![
            (2322, source.rfind("first").unwrap() as u32, 5),
            (2322, source.rfind("second").unwrap() as u32, 6),
        ]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn paired_destructuring_defaults_exclude_undefined_only_under_strict_null_checks() {
    for source in [
        "let value:number=0;[value = 1]=[undefined];",
        "let wrapped:number=0;[((wrapped)) = ((1))]=[((undefined))];",
        "let renamed:number=0;({renamed = ((1))}={renamed:((undefined))});",
    ] {
        assert_complete(source);
    }

    let output = compile(
        "let value:number=0;[value = 1]=[undefined as number|undefined];",
        true,
        true,
    );
    assert_eq!(output.diagnostics, []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);

    let source = "let value:number=0;[value = 1]=[undefined];";
    let output = compile(source, false, true);
    assert_eq!(output.diagnostics, []);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);

    let source = "let value:number=0;[value = 1]=[null];";
    let output = compile(source, true, true);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.start,
                diagnostic.length,
                diagnostic.message_text.as_str(),
            ))
            .collect::<Vec<_>>(),
        vec![(
            2322,
            source.rfind("value").unwrap() as u32,
            "value".len() as u32,
            "Type 'null' is not assignable to type 'number'.",
        )]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn tuple_string_object_and_index_signature_reads_use_canonical_types() {
    for source in [
        "declare const tuple:[number,string];const first:number=tuple[0];const second:string=(tuple)[1];",
        "const character:string='abc'[1];",
        "declare const object:{named:boolean};const named:boolean=object['named'];",
        "declare let byString:{[key:string]:number};const fallback:number=byString[3];",
    ] {
        assert_complete(source);
    }
}

#[test]
fn union_receivers_and_union_indices_combine_read_types() {
    assert_complete(concat!(
        "declare const tagged:{tag:'left'}|{tag:'right'};",
        "const tag:'left'|'right'=tagged['tag'];",
        "declare const pair:{left:number;right:string};",
        "declare const side:'left'|'right';",
        "const selected:number|string=pair[side];",
    ));
}

#[test]
fn typed_array_writes_and_bitwise_precedence_remain_complete() {
    let source = concat!(
        "function multiply(a0,a1){",
        "let r:number[]=[];let v;",
        "r[0]=(v=a0*a0)&0xFFFF;",
        "r[1]=(v=((v/0x10000)|0)+2*a0*a1)&0xFFFF;",
        "return r;}",
    );
    let output = compile(source, false, false);
    assert_eq!(output.diagnostics, [], "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(output.exit_status, CompileExitStatus::Success);
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("JavaScript output");
    assert!(
        javascript.text.contains("r[0] = (v = a0 * a0) & 0xFFFF;"),
        "{}",
        javascript.text
    );
    assert!(
        javascript
            .text
            .contains("r[1] = (v = ((v / 0x10000) | 0) + 2 * a0 * a1) & 0xFFFF;"),
        "{}",
        javascript.text
    );
}

#[test]
fn valid_element_types_still_report_independent_assignment_relations() {
    let source = concat!(
        "let values:string[]=['text'];",
        "const first:number=values[0];",
        "const object:{value:string}={value:'text'};",
        "const second:boolean=object['value'];",
    );
    let output = compile(source, true, true);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message_text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (2322, "Type 'string' is not assignable to type 'number'."),
            (2322, "Type 'string' is not assignable to type 'boolean'."),
        ],
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
}

#[test]
fn unsupported_generic_missing_and_readonly_accesses_remain_deferred() {
    for source in [
        "function read<T,K>(object:T,key:K){return object[key];}",
        "declare let object:{present:number};const missing=object['absent'];",
        "declare let object:{readonly value:number};object['value']=1;",
        "declare let object:{readonly [key:string]:number};object['value']=1;",
        "declare let object:{[key:number]:string};const value=object[1];",
    ] {
        let output = compile(source, true, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::SemanticIncomplete);
    }
}

#[test]
fn implicit_any_missing_keys_are_option_structural_not_name_special_cases() {
    for source in [
        "declare const receiver:{present:number};const value=receiver['renamedMissing'];",
        "declare const values:number[];const value=values['renamedMissing'];",
        "class Vessel{read(){return this['renamedMissing'];}}",
        "class Renamed{read(){return this['__proto__'];}}",
    ] {
        let output = compile(source, false, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }

    let strict = compile(
        "declare const receiver:{present:number};const value=receiver['renamedMissing'];",
        true,
        true,
    );
    assert_eq!(strict.diagnostics, []);
    assert_eq!(strict.semantic_completion, SemanticCompletion::Deferred);
}

#[test]
fn array_constructor_property_access_uses_the_owned_loose_library_boundary() {
    for source in [
        "const value=Array['toString'];",
        "const value=Array['renamedMissing'];",
        "Array['toString'];",
        "Array['renamedMissing'];",
        "/*0*/ Array /*1*/[ /*2*/ 'toString' /*3*/ ] /*4*/; /*5*/",
        "/*0*/ Array\n// line\n/*1*/[ /*2*/ 'toString'\n// line\n/*3*/ ] /*4*/",
    ] {
        let output = compile(source, false, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
    }

    for source in ["Array['renamedMissing'];", "(Array)['renamedMissing'];"] {
        let output = compile(source, true, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
    }
}

#[test]
fn strict_array_constructor_reads_and_writes_use_the_generated_function_member() {
    for source in [
        "Array['toString'];",
        "(Array)['toString'];",
        "const renamed=Array;renamed['toString'];",
        "const method:()=>string=Array['toString'];",
        "const rendered:string=Array['toString']();",
        "Array['toString']=()=>'';",
        "const renamed=Array;(renamed)['toString']=()=>'';",
        "/*0*/ Array /*1*/[ /*2*/ 'toString' /*3*/ ] /*4*/; /*5*/",
        "/*0*/ Array\n// line\n/*1*/[ /*2*/ 'toString'\n// line\n/*3*/ ] /*4*/",
    ] {
        let output = compile(source, true, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Complete,
            "{source}"
        );
        assert_eq!(output.exit_status, CompileExitStatus::Success, "{source}");
    }

    for source in [
        "Array['renamedMissing'];",
        "Array['renamedMissing']=123;",
        "declare const key:string;Array[key];",
        "Array['toLocaleString'];",
    ] {
        let output = compile(source, true, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(
            output.semantic_completion,
            SemanticCompletion::Deferred,
            "{source}"
        );
        assert_eq!(
            output.exit_status,
            CompileExitStatus::SemanticIncomplete,
            "{source}"
        );
    }
}

#[test]
fn known_array_function_member_writes_have_exact_strict_and_loose_relations() {
    for strict in [true, false] {
        for source in [
            "Array['toString']=123;",
            "const renamed=Array;(renamed)['toString']=123;",
            "interface Function{renamed():void}Array['toString']=123;",
            "interface Object{toString():number}Array['toString']=123;",
            "interface RenamedFunction{toString():number}Array['toString']=123;",
        ] {
            let output = compile(source, strict, true);
            assert_eq!(
                output
                    .diagnostics
                    .iter()
                    .map(|diagnostic| (
                        diagnostic.file.as_str(),
                        diagnostic.code,
                        diagnostic.start,
                        diagnostic.length,
                        diagnostic.category,
                        diagnostic.message_text.as_str(),
                        diagnostic.related_information.as_slice(),
                    ))
                    .collect::<Vec<_>>(),
                vec![(
                    "case.ts",
                    2322,
                    source.rfind("123").expect("assignment source") as u32,
                    3,
                    DiagnosticCategory::Error,
                    "Type 'number' is not assignable to type '() => string'.",
                    &[][..],
                )],
                "strict={strict}: {source}"
            );
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "strict={strict}: {source}"
            );
            assert_eq!(
                output.exit_status,
                CompileExitStatus::DiagnosticsPresentOutputsSkipped,
                "strict={strict}: {source}"
            );
        }

        for source in [
            "Array['toString']=()=>'';",
            "const renamed=Array;(renamed)['toString']=()=>'';",
        ] {
            let output = compile(source, strict, true);
            assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Complete,
                "strict={strict}: {source}"
            );
        }
    }

    for source in [
        "Array['renamedMissing']=123;",
        "const renamed=Array;(renamed)['renamedMissing']=123;",
    ] {
        let output = compile(source, false, true);
        assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
        assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    }
}

#[test]
fn matching_array_function_augmentations_defer_reads_and_writes_locally() {
    for strict in [true, false] {
        for source in [
            concat!(
                "interface ArrayConstructor{toString():number}",
                "const expected:()=>number=Array['toString'];",
            ),
            concat!(
                "interface CallableFunction{toString():number}",
                "const renamed=Array;renamed['toString']=123;",
            ),
            concat!(
                "interface Function{toString():number}",
                "const renamed=Array;const expected:()=>number=(renamed)['toString'];",
            ),
        ] {
            let output = compile(source, strict, true);
            assert_eq!(output.diagnostics, [], "{source}: {:?}", output.diagnostics);
            assert_eq!(
                output.semantic_completion,
                SemanticCompletion::Deferred,
                "strict={strict}: {source}"
            );
            assert_eq!(
                output.exit_status,
                CompileExitStatus::SemanticIncomplete,
                "strict={strict}: {source}"
            );
        }
    }
}

#[test]
fn unused_strict_accesses_still_require_the_owned_lookup() {
    let supported = compile(
        "declare const receiver:{present:number};receiver['present'];",
        true,
        true,
    );
    assert_eq!(supported.diagnostics, []);
    assert_eq!(supported.semantic_completion, SemanticCompletion::Complete);

    let missing = compile(
        "declare const receiver:{present:number};receiver['renamedMissing'];",
        true,
        true,
    );
    assert_eq!(missing.diagnostics, []);
    assert_eq!(missing.semantic_completion, SemanticCompletion::Deferred);
    assert_eq!(missing.exit_status, CompileExitStatus::SemanticIncomplete);
}

#[test]
fn empty_element_access_keeps_its_syntax_owned_ts1011() {
    let output = compile("declare const values:number[];values[];", true, true);
    assert_eq!(
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        vec![1011]
    );
    assert_eq!(output.semantic_completion, SemanticCompletion::Complete);
    assert_eq!(
        output.exit_status,
        CompileExitStatus::DiagnosticsPresentOutputsSkipped
    );
}

#[test]
fn recovery_contexts_do_not_reclassify_array_syntax_as_empty_element_access() {
    for (source, expected) in [
        ("const chosen = flag ? renamed : [];", vec![1109, 1109]),
        ("const nested = flag ? renamed : [1, 2];", vec![1109, 1109]),
        ("function nested([], {}): void {}", vec![]),
        (
            "type Selected<Value> = Value extends never[] ? Value : [];",
            vec![],
        ),
    ] {
        assert_eq!(parsed_diagnostic_codes(source), expected, "{source}");
    }
}

#[test]
fn keyof_owns_postfix_array_and_indexed_type_suffixes() {
    for source in [
        "type Renamed<Payload> = keyof Payload[];",
        "type Indexed<Payload, Slot extends keyof Payload> = keyof Payload[Slot];",
        "type Remapped = { [Slot in keyof number[] as Slot]: (number[])[Slot] };",
        "type Wrapped<Payload> = (keyof Payload)[];",
    ] {
        let diagnostics = parsed_diagnostic_codes(source);
        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
    }
}

#[test]
fn retained_recovery_extents_do_not_manufacture_empty_element_access() {
    for source in [
        "declare const value:unknown;const asserted=<Renamed[]>value;",
        "declare const value:unknown;const asserted=<Wrapped[]>((value));",
        "declare let renamed:unknown;for(renamed of []){}",
        "declare let renamed:unknown;for((renamed) of []){}",
        "declare let renamed:unknown;for(renamed of\n[]){}",
        "function nested(){declare let renamed:unknown;for((renamed) of\n[]){}}",
        "declare const source:unknown;const [,,[,[],,[],]]=source;",
        "declare function consume(...values:unknown[]):void;consume(...[]);",
        "const nested=[...[...[]]];",
        "declare const value:unknown;const asserted=<Renamed[]>\nvalue;",
        concat!(
            "declare const value:unknown;declare const renamed:number[];",
            "const asserted=<unknown[]>value renamed[];",
        ),
    ] {
        assert!(
            !parsed_diagnostic_codes(source).contains(&1011),
            "{source}: {:?}",
            parsed_diagnostic_codes(source),
        );
    }
}

#[test]
fn statement_boundaries_keep_real_empty_element_access_diagnostics() {
    for source in [
        concat!(
            "declare const value:unknown;declare const values:number[];",
            "const asserted=<unknown[]>value;values[];",
        ),
        concat!(
            "declare const value:unknown;declare const values:number[];\n",
            "const asserted=<unknown[]>value\nvalues[];",
        ),
        concat!(
            "declare let renamed:unknown;declare const values:number[];",
            "for(renamed of\n[]){values[];}",
        ),
    ] {
        assert_eq!(
            parsed_diagnostic_codes(source)
                .into_iter()
                .filter(|code| *code == 1011)
                .count(),
            1,
            "{source}",
        );
    }
}

#[test]
fn definite_empty_element_access_still_reports_ts1011_through_wrappers() {
    for source in [
        "declare const values:number[];values[];",
        "declare const renamed:number[];(renamed)[];",
        "declare const nested:number[];((nested))[];",
    ] {
        assert_eq!(parsed_diagnostic_codes(source), vec![1011], "{source}");
    }
}
