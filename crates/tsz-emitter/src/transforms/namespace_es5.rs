//! ES5 Namespace Transform
//!
//! Transforms TypeScript namespaces to ES5 IIFE patterns:
//!
//! ```typescript
//! namespace foo {
//!     export class Provide { }
//! }
//! ```
//!
//! Becomes:
//!
//! ```javascript
//! var foo;
//! (function (foo) {
//!     var Provide = /** @class */ (function () {
//!         function Provide() { }
//!         return Provide;
//!     }());
//!     foo.Provide = Provide;
//! })(foo || (foo = {}));
//! ```
//!
//! Also handles qualified names like `namespace A.B.C`:
//! ```javascript
//! var A;
//! (function (A) {
//!     var B;
//!     (function (B) {
//!         var C;
//!         (function (C) {
//!             // body
//!         })(C = B.C || (B.C = {}));
//!     })(B = A.B || (A.B = {}));
//! })(A || (A = {}));
//! ```

use crate::context::transform::TransformContext;
use crate::transforms::ir_printer::IRPrinter;
use crate::transforms::namespace_es5_ir::NamespaceES5Transformer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

fn strip_stray_export_lines(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let mut cleaned: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == "export" {
            if let (Some(prev), Some(next)) = (cleaned.last(), lines.get(i + 1))
                && prev.trim() == next.trim()
                && (next.trim_start().starts_with("//") || next.trim_start().starts_with("/*"))
            {
                cleaned.pop();
            }
            i += 1;
            continue;
        }
        cleaned.push(lines[i]);
        i += 1;
    }

    if output.ends_with('\n') {
        format!("{}\n", cleaned.join("\n"))
    } else {
        cleaned.join("\n")
    }
}

/// Namespace ES5 emitter
///
/// This is a thin wrapper around `NamespaceES5Transformer` and `IRPrinter`
/// for backward compatibility.
///
/// # Architecture
///
/// - Uses `NamespaceES5Transformer` to produce IR nodes
/// - Uses `IRPrinter` to emit IR nodes as JavaScript strings
/// - Maintains the same public API as the original implementation
pub struct NamespaceES5Emitter<'a> {
    arena: &'a NodeArena,
    source_text: Option<&'a str>,
    indent_level: u32,
    should_declare_var: bool,
    target_es5: bool,
    remove_comments: bool,
    transforms: Option<TransformContext>,
    system_export_folds: Vec<String>,
    transformer: NamespaceES5Transformer<'a>,
}

impl<'a> NamespaceES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        NamespaceES5Emitter {
            arena,
            source_text: None,
            indent_level: 0,
            should_declare_var: true, // Default to true for backward compatibility
            target_es5: false,
            remove_comments: false,
            transforms: None,
            system_export_folds: Vec::new(),
            transformer: NamespaceES5Transformer::new(arena),
        }
    }

    /// Create a namespace emitter with `CommonJS` mode
    pub fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        NamespaceES5Emitter {
            arena,
            source_text: None,
            indent_level: 0,
            should_declare_var: true, // Default to true for backward compatibility
            target_es5: false,
            remove_comments: false,
            transforms: None,
            system_export_folds: Vec::new(),
            transformer: NamespaceES5Transformer::with_commonjs(arena, is_commonjs),
        }
    }

    /// Set the source text for `ASTRef` emission and comment extraction
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
        self.transformer.set_source_text(text);
    }

    /// Set whether to emit a 'var' declaration for the namespace
    /// When false (e.g., when merging with a class/enum/function), the 'var' is omitted
    pub const fn set_should_declare_var(&mut self, value: bool) {
        self.should_declare_var = value;
    }

    /// Mark this emitter as targeting ES5 (disables `let` in namespace IIFE bodies).
    pub const fn set_target_es5(&mut self, es5: bool) {
        self.target_es5 = es5;
    }

    /// When true, suppress `/** @class */` annotation in output.
    pub const fn set_remove_comments(&mut self, remove: bool) {
        self.remove_comments = remove;
    }

    pub fn set_system_export_fold(&mut self, export_name: &str) {
        self.set_system_export_folds([export_name]);
    }

    pub fn set_system_export_folds<'b>(&mut self, export_names: impl IntoIterator<Item = &'b str>) {
        self.system_export_folds = export_names
            .into_iter()
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .collect();
    }

    /// Set transform directives so that nested transforms (e.g. ES5 template
    /// literal downleveling) are applied when emitting `ASTRef` nodes.
    pub fn set_transforms(&mut self, transforms: TransformContext) {
        self.transforms = Some(transforms);
    }

    /// Set whether legacy decorators are enabled (experimentalDecorators)
    pub const fn set_legacy_decorators(&mut self, enabled: bool) {
        self.transformer.set_legacy_decorators(enabled);
    }

    /// Set whether `__metadata` calls should be emitted in `__decorate`
    /// arrays for classes nested in this namespace.
    pub const fn set_emit_decorator_metadata(&mut self, enabled: bool) {
        self.transformer.set_emit_decorator_metadata(enabled);
    }

    /// Set exported variable names from prior blocks of the same namespace.
    pub fn set_prior_exported_vars(&mut self, vars: std::collections::HashSet<String>) {
        self.transformer.set_prior_exported_vars(vars);
    }

    pub fn set_default_exported_func_names(&mut self, names: std::collections::HashSet<String>) {
        self.transformer.set_default_exported_func_names(names);
    }

    /// Collect exported variable names from a namespace declaration without emitting.
    pub fn collect_exported_var_names(
        &self,
        ns_idx: NodeIndex,
    ) -> std::collections::HashSet<String> {
        self.transformer.collect_exported_var_names(ns_idx)
    }

    /// Emit a namespace declaration
    pub fn emit_namespace(&mut self, ns_idx: NodeIndex) -> String {
        let ast_qualification = self.transformer.collect_namespace_rewrite_var_names(ns_idx);
        let ir = self
            .transformer
            .transform_namespace_with_var_flag(ns_idx, self.should_declare_var);
        let mut ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };
        self.apply_system_export_fold(&mut ir);

        let mut printer = if let Some(source_text) = self.source_text {
            IRPrinter::with_arena_and_source(self.arena, source_text)
        } else {
            IRPrinter::with_arena(self.arena)
        };
        printer.set_indent_level(self.indent_level);
        printer.set_target_es5(self.target_es5);
        printer.set_remove_comments(self.remove_comments);
        if let Some((namespace, names)) = ast_qualification {
            printer.set_namespace_ast_qualification(namespace, names);
        }
        if let Some(ref transforms) = self.transforms {
            printer.set_transforms(transforms.clone());
        }
        strip_stray_export_lines(printer.emit(&ir))
    }

    /// Emit an exported namespace declaration (`CommonJS` attach-to-exports form).
    pub fn emit_exported_namespace(&mut self, ns_idx: NodeIndex) -> String {
        let ast_qualification = self.transformer.collect_namespace_rewrite_var_names(ns_idx);
        let ir = self
            .transformer
            .transform_exported_namespace_with_var_flag(ns_idx, self.should_declare_var);
        let mut ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };
        self.apply_system_export_fold(&mut ir);

        let mut printer = if let Some(source_text) = self.source_text {
            IRPrinter::with_arena_and_source(self.arena, source_text)
        } else {
            IRPrinter::with_arena(self.arena)
        };
        printer.set_indent_level(self.indent_level);
        printer.set_target_es5(self.target_es5);
        printer.set_remove_comments(self.remove_comments);
        if let Some((namespace, names)) = ast_qualification {
            printer.set_namespace_ast_qualification(namespace, names);
        }
        if let Some(ref transforms) = self.transforms {
            printer.set_transforms(transforms.clone());
        }
        strip_stray_export_lines(printer.emit(&ir))
    }

    /// Set the indent level for output
    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    fn apply_system_export_fold(&self, ir: &mut crate::transforms::ir::IRNode) {
        if self.system_export_folds.is_empty() {
            return;
        }
        if let crate::transforms::ir::IRNode::NamespaceIIFE {
            system_export_names,
            ..
        } = ir
        {
            *system_export_names = self
                .system_export_folds
                .iter()
                .cloned()
                .map(Into::into)
                .collect();
        }
    }
}

#[cfg(test)]
#[path = "../../tests/namespace_es5.rs"]
mod tests;
