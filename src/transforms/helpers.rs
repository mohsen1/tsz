use std::borrow::Cow;

use rustc_hash::FxHashMap;
use swc_atoms::Atom;
use swc_common::collections::{AHashMap, AHashSet};
use swc_common::{FileName, Mark, SourceMap, Span, DUMMY_SP};
use swc_ecma_ast::*;
use swc_ecma_codegen::{text_writer::JsWriter, Config, Emitter};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax};
use swc_ecma_transforms_base::helper::Helper as HelperDef;
use swc_ecma_utils::prepend_stmts;
use swc_ecma_visit::noop_visit_mut_type;
use swc_ecma_visit::VisitMutWith;

use crate::utils::prepend;

mod source_map;

/// Checks if helper is used and injects it.
#[derive(Debug)]
pub struct HelperInjector<'a> {
    pub cm: &'a SourceMap,
    pub helpers: &'a HelperManager,
    /// This determines whether to inject helpers for the top-level module/script.
    /// We use this to allow the core transforms to run (which might populate helpers)
    /// before the main body is finalized.
    pub top_level_mark: Mark,
    pub unresolved_mark: Mark,
}

/// Manages helpers and their usage.
#[derive(Debug, Default)]
pub struct HelperManager {
    used: AHashSet<HelperDef>,
    data: AHashMap<HelperDef, Vec<Stmt>>,
    /// Cached source code of helpers
    src_map: AHashMap<HelperDef, String>,
}

impl HelperManager {
    pub fn used(&self, helper: HelperDef) -> bool {
        self.used.contains(&helper)
    }

    pub fn mark_used(&mut self, helper: HelperDef) {
        self.used.insert(helper);
    }
}

impl<'a> HelperInjector<'a> {
    /// Injects used helpers into the body.
    pub fn inject(&self, body: &mut Vec<Stmt>) {
        // Filter out helpers that were already injected or are not used.
        // In a real scenario, we might track injected helpers to avoid duplicates.
        
        let mut to_inject = Vec::new();
        for helper in &self.helpers.used {
            if let Some(stmts) = self.helpers.data.get(helper) {
                // We clone the statements here. 
                // In a highly optimized version, we might reuse AST nodes with proper cloning hygiene.
                to_inject.extend(stmts.clone());
            }
        }

        if !to_inject.is_empty() {
            prepend_stmts(body, to_inject);
        }
    }
}

impl HelperManager {
    /// Generates the AST statements for a given helper.
    /// This replaces the old string-based injection to ensure source maps work.
    fn generate_helper_ast(&mut self, helper: HelperDef) -> Vec<Stmt> {
        // We use `self.src_map` to retrieve the raw source code string for the helper.
        let src = self.get_src(&helper);
        
        // We use a dummy FileName for the helper content.
        let fm = self.cm.new_source_file(
            FileName::Custom(format!("@swc-helper-{}", helper.name())),
            src,
        );

        let mut lexer = Lexer::new(
            Syntax::Es(Default::default()),
            Default::default(),
            StringInput::from(&*fm),
            None,
        );

        let mut parser = Parser::new_from(lexer);
        
        let mut stmts = Vec::new();
        
        // Helpers are typically scripts, but we parse them as a Module to allow imports if necessary
        // or just to handle generic statements. 
        // We expect helpers to be a list of statements.
        // Usually helpers are just function declarations.
        let module = match parser.parse_module() {
            Ok(module) => module,
            Err(e) => {
                // In production code we might panic or handle this, 
                // but for the task we assume valid helper source.
                panic!("Failed to parse helper {}: {:?}", helper.name(), e);
            }
        };

        // We extract the body of the module.
        // We also need to ensure top-level context (like `this`) is handled if we were in a script.
        // However, helpers are standard library functions.
        
        module.body
    }

    fn get_src(&mut self, helper: &HelperDef) -> String {
        // Check if we already have the source loaded
        if let Some(src) = self.src_map.get(helper) {
            return src.clone();
        }

        // Otherwise, generate it from the definition (which is effectively a static template)
        // or load it from the `swc_ecma_transforms_base` data.
        // For this implementation, we will retrieve the canonical code snippet.
        // Note: swc's `HelperDef` usually has a `src` method or similar in the actual crate,
        // but here we assume we can generate the string.
        
        // Helper source map loading (simplified):
        // In the real `swc`, helpers are stored as static strings.
        let src = helper.to_code_str(); 

        // Cache it
        self.src_map.insert(*helper, src);
        
        // Parse and cache the AST
        let ast = self.generate_helper_ast(*helper);
        self.data.insert(*helper, ast);
        
        // Return the string (though generate_helper_ast consumes it if we aren't careful)
        self.src_map.get(helper).unwrap().clone()
    }
}

// A mock/placeholder extension for HelperDef if it doesn't have to_code_str in the current context version
// In the real codebase, this logic relies on swc_ecma_transforms_base::helper::Helper
trait HelperSource {
    fn to_code_str(&self) -> String;
}

impl HelperSource for HelperDef {
    fn to_code_str(&self) -> String {
        // Mapping HelperDef variants to their source code.
        // This is the "source of truth" for the helper strings.
        match self {
            HelperDef::TsEnum => include_str!("../helpers/ts_enum.js").to_string(),
            HelperDef::Extends => include_str!("../helpers/_extends.js").to_string(),
            HelperDef::ClassPrivateFieldGetSet => include_str!("../helpers/classPrivateFieldSet.js").to_string(),
            // ... handle other helpers as defined by the transform base
            _ => format!("export function {}() {{}}", self.name()), // Fallback for unimplemented in this snippet
        }
    }
}

// The actual transforms logic usually calls `helper_manager.mark_used` during the traversal.
// This struct handles the injection phase.

impl<'a> VisitMut for HelperInjector<'a> {
    noop_visit_mut_type!();

    fn visit_mut_module(&mut self, node: &mut Module) {
        node.visit_mut_children_with(self);
        self.inject(&mut node.body);
    }

    fn visit_mut_script(&mut self, node: &mut Script) {
        node.visit_mut_children_with(self);
        self.inject(&mut node.body);
    }
}
```

```rust
//
