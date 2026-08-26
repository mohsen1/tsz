use std::sync::Arc;

use crate::{Compiler, CompilerOptions, SourceInput};

#[test]
fn declaration_accessors_follow_typescript_erasure_and_setter_spelling() {
    let source = r#"
declare class AccessorMatrix<T> {
    static get shared(): string;
    static set shared(authored: string);
    protected get protectedValue(): T;
    protected set protectedValue(renamed: T);
    public get exposed(): T;
    public set exposed(candidate: T);
    get current(): T;
    set current(candidate: T);
    private static get hiddenStatic(): string;
    private static set hiddenStatic(secret: string);
    private get hidden(): T;
    private set hidden(other: T);
}
"#;
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "accessor-matrix.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            declaration: true,
            target: "es2015".to_string(),
            module: "commonjs".to_string(),
            ..CompilerOptions::default()
        },
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let declaration = output
        .emitted_files
        .iter()
        .find(|file| file.declaration)
        .expect("declaration output");
    assert_eq!(
        declaration.text,
        concat!(
            "declare class AccessorMatrix<T> {\n",
            "    static get shared(): string;\n",
            "    static set shared(authored: string);\n",
            "    protected get protectedValue(): T;\n",
            "    protected set protectedValue(renamed: T);\n",
            "    get exposed(): T;\n",
            "    set exposed(candidate: T);\n",
            "    get current(): T;\n",
            "    set current(candidate: T);\n",
            "    private static get hiddenStatic();\n",
            "    private static set hiddenStatic(value);\n",
            "    private get hidden();\n",
            "    private set hidden(value);\n",
            "}\n",
        )
    );
}

#[test]
fn parameter_property_declarations_are_hoisted_before_authored_class_members() {
    let source = r#"
export class Ordered<T> {
    #native!: T;
    before: T;
    private constructor(
        private secret: T,
        public visible: T,
        protected guarded: T,
        readonly fixed: T
    ) {}
    after: T;
}
"#;
    let output = Compiler::new().compile(
        vec![SourceInput::new(
            "parameter-properties.ts",
            Arc::<str>::from(source),
        )],
        &CompilerOptions {
            declaration: true,
            target: "es2022".to_string(),
            module: "esnext".to_string(),
            no_check: true,
            ..CompilerOptions::default()
        },
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output
            .emitted_files
            .iter()
            .find(|file| file.declaration)
            .expect("declaration output")
            .text,
        concat!(
            "export declare class Ordered<T> {\n",
            "    #private;\n",
            "    private secret;\n",
            "    visible: T;\n",
            "    protected guarded: T;\n",
            "    readonly fixed: T;\n",
            "    before: T;\n",
            "    private constructor();\n",
            "    after: T;\n",
            "}\n",
        ),
    );
}
