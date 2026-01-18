use std::sync::Arc;

use swc_common::FileName;
use swc_common::SourceMap;
use swc_ecma_ast::Module;
use swc_ecma_codegen::{text_writer::JsWriter, Config, Emitter};
use swc_ecma_parser::{Parser, StringInput, Syntax};
use swc_ecma_transforms_base::helper::Helper;
use swc_ecma_visit::VisitMutWith;

use crate::transforms::helpers::{HelperInjector, HelperManager};

#[test]
fn test_helper_injection() {
    let cm = Arc::new(SourceMap::default());
    let manager = HelperManager::default();
    
    // Mark a helper as used
    manager.mark_used(Helper::Extends);
    manager.mark_used(Helper::ClassPrivateFieldSet);

    // Top-level marks (unresolved/top_level) typically used during transforms
    let unresolved_mark = swc_common::Mark::new();
    let top_level_mark = swc_common::Mark::new();

    let mut injector = HelperInjector {
        cm: &cm,
        helpers: &manager,
        top_level_mark,
        unresolved_mark,
    };

    // Create a dummy input module
    let src = r#"
        class A extends B {}
    "#;

    let fm = cm.new_source_file(FileName::Anon, src.into());
    let mut parser = Parser::new(
        Syntax::Es(Default::default()),
        StringInput::from(&*fm),
        None,
    );
    
    let mut module = parser.parse_module().unwrap();

    // Run injection
    module.visit_mut_with(&mut injector);

    // Verify the module body now contains the helpers
    // Note: The helpers are injected at the beginning.
    assert!(module.body.len() > 1);

    // Let's verify the content by codegenning it
    let mut buf = Vec::new();
    {
        let mut emitter = Emitter {
            cfg: Config::default().with_minify(false),
            cm: cm.clone(),
            comments: None,
            wr: JsWriter::new(cm, "\n", &mut buf, None),
        };
