use super::*;

#[test]
fn constructor_parameter_properties_emit_fields_assignments_and_declarations() {
    let source = "export class C{constructor(public readonly x:number){}} export class D{constructor(public x:number=1){}}";
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    let javascript = output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("javascript");
    assert_eq!(
        javascript.text,
        "export class C {\n    x;\n    constructor(x) {\n        this.x = x;\n    }\n}\nexport class D {\n    x;\n    constructor(x = 1) {\n        this.x = x;\n    }\n}\n"
    );
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration");
    assert_eq!(
        declaration.text,
        "export declare class C {\n    readonly x: number;\n    constructor(x: number);\n}\nexport declare class D {\n    x: number;\n    constructor(x?: number);\n}\n"
    );

    let es2015 = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "es2015".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    let javascript = es2015
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("javascript");
    assert!(!javascript.text.contains("    x;"));
    assert_eq!(javascript.text.matches("this.x = x;").count(), 2);
}

#[test]
fn optional_and_override_parameter_properties_keep_exact_emit_structure() {
    let source = concat!(
        "export declare class B{x:number} ",
        "export class Optional{constructor(public value?:number){}} ",
        "export class Derived extends B{constructor(override x:number){super()}}",
    );
    let output = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.semantic_completion, SemanticCompletion::Deferred);
    let javascript = &output
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("javascript")
        .text;
    assert!(javascript.contains(
        "class Optional {\n    value;\n    constructor(value) {\n        this.value = value;\n    }\n}"
    ));
    assert!(javascript.contains(
        "class Derived extends B {\n    x;\n    constructor(x) {\n        super();\n        this.x = x;\n    }\n}"
    ));
    let declaration = &output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration")
        .text;
    assert!(declaration.contains(
        "export declare class Optional {\n    value?: number | undefined;\n    constructor(value?: number | undefined);\n}"
    ));
    assert!(declaration.contains(
        "export declare class Derived extends B {\n    x: number;\n    constructor(x: number);\n}"
    ));
    assert!(!declaration.contains("override x"));
}

#[test]
fn derived_parameter_property_emit_preserves_directives_and_super_order() {
    let source = concat!(
        "class B{} ",
        "export class Base{constructor(public x:number){\"use custom\";work();}} ",
        "export class Derived extends B{constructor(public x:number){\"use custom\";super();work();}}",
    );
    let esnext = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    let text = &esnext
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("esnext javascript")
        .text;
    assert!(text.contains(
        "constructor(x) {\n        \"use custom\";\n        this.x = x;\n        work();\n    }"
    ));
    assert!(text.contains(
        "constructor(x) {\n        \"use custom\";\n        super();\n        this.x = x;\n        work();\n    }"
    ));
    assert_eq!(text.matches("    x;\n").count(), 2);

    let es2015 = Compiler::new().compile(
        vec![SourceInput::new("case.ts", Arc::<str>::from(source))],
        &CompilerOptions {
            target: "es2015".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        },
    );
    let text = &es2015
        .emitted_files
        .iter()
        .find(|file| !file.declaration)
        .expect("es2015 javascript")
        .text;
    assert!(!text.contains("    x;\n"));
    assert!(text.find("super();").unwrap() < text.rfind("this.x = x;").unwrap());

    let instance = compile(
        "class C{constructor(public x:number){}} const value=new C(1).x;",
        true,
    );
    assert_eq!(instance.semantic_completion, SemanticCompletion::Deferred);
}
