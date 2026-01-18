```rust
// src/transforms/async_es5_tests.rs

use super::*;
use swc_ecma_parser::{Parser, StringInput, Syntax};
use swc_ecma_transforms_base::resolver::resolver;
use swc_common::{errors::ColorConfig, SourceMap};
use swc_ecma_codegen::{text_writer::JsWriter, Emitter};
use std::sync::Arc;
use swc_common::FileName;

#[test]
fn test_simple_async_function() {
    let source = r#"
    async function test() {
        await 1;
        return 2;
    }
    "#;

    let output = transform(source);
    assert!(output.contains("state")); // Check for state machine artifacts
}

#[test]
fn test_async_arrow_function() {
    let source = r#"
    const fn = async () => {
        await foo();
    };
    "#;

    let output = transform(source);
    assert!(output.contains("function")); // Should output a function
}

#[test]
fn test_class_method_async() {
    let source = r#"
    class Foo {
        async bar() {
            await baz();
        }
    }
    "#;

    let output = transform(source);
    assert!(output.contains("state"));
}

// Helper function to run the transformation
fn transform(source: &str) -> String {
    let cm = Arc::new(SourceMap::default());
    let handler = swc_common::errors::Handler::with_tty_emitter(
        ColorConfig::Auto,
        true,
        false,
        Some(cm.clone()),
    );

    let fm = cm.new_source_file(
        FileName::Custom("test.js".into()),
        source.into(),
    );

    let mut parser = Parser::new(
        Syntax::Es(Default::default()),
        StringInput::from(&*fm),
        None,
    );

    let mut module = parser.parse_module().map_err(|e| {
        e.into_diagnostic(&handler).emit();
        panic!("Parse error")
    }).unwrap();

    let mut async_transform = AsyncEs5::new(Config::default());
    
    // Run resolver first
    module.visit_mut_with(&mut resolver());
    // Run async transform
    module.visit_mut_with(&mut async_transform);

    let mut buf = Vec::new();
    {
        let writer = JsWriter::new(cm, "\n", &mut buf, None);
        let mut emitter = Emitter {
            cfg: swc_ecma_codegen::Config {
                minify: false,
                ..Default::default()
            },
            cm: cm.clone(),
            comments: None,
            wr: writer,
        };
        emitter.emit_module(&module).unwrap();
    }

    String::from_utf8(buf).unwrap()
}
```

This code provides the basic structure for the `async_es5` transform. The key parts are:

1.  **`AsyncEs5` struct**: Holds configuration and state for the transformation.
2.  **`VisitMut` impl**: Hooks into SWC's visitor pattern to find and transform async functions.
3.  **`StateMachineBuilder`**: A helper to encapsulate the logic for creating the state machine AST.
4.  **Tests**: Basic tests to verify the transformation works.

In a full implementation, the `StateMachineBuilder` would contain the complex logic to break the function body into states and generate the corresponding switch statement and helper calls. The provided code gives you the skeleton and the key integration points.

Let me know if you'd like me to flesh out any specific part in more detail!
