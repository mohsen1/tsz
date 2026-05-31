use super::EnumES5Transformer;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

/// Enum ES5 emitter wrapping `EnumES5Transformer` + `IRPrinter`
pub struct EnumES5Emitter<'a> {
    indent_level: u32,
    transformer: EnumES5Transformer<'a>,
}

impl<'a> EnumES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        EnumES5Emitter {
            indent_level: 0,
            transformer: EnumES5Transformer::new(arena),
        }
    }

    pub const fn set_indent_level(&mut self, level: u32) {
        self.indent_level = level;
    }

    /// Set source text for raw expression extraction
    pub const fn set_source_text(&mut self, text: &'a str) {
        self.transformer.set_source_text(text);
    }

    /// Set whether the emit target downlevels block-scoped declarations to
    /// `var` (ES5/ES3), enabling the `var E = void 0;` reset for block-scoped
    /// enums.
    pub const fn set_target_es5(&mut self, value: bool) {
        self.transformer.set_target_es5(value);
    }

    /// Set whether const enums should be preserved (emitted instead of erased)
    pub const fn set_preserve_const_enums(&mut self, value: bool) {
        self.transformer.set_preserve_const_enums(value);
    }

    /// Set whether the enum should emit its own `var E;` declaration.
    pub const fn set_emit_var_declaration(&mut self, value: bool) {
        self.transformer.set_emit_var_declaration(value);
    }

    /// Fold a CommonJS export binding into the enum IIFE tail.
    pub fn set_commonjs_export_fold(&mut self, export_name: &str) {
        self.transformer.set_commonjs_export_fold(export_name);
    }

    pub fn set_commonjs_export_folds<'b>(
        &mut self,
        export_names: impl IntoIterator<Item = &'b str>,
    ) {
        self.transformer.set_commonjs_export_folds(export_names);
    }

    /// Fold a System export call into the enum IIFE tail.
    pub fn set_system_export_fold(&mut self, export_name: &str) {
        self.transformer.set_system_export_fold(export_name);
    }

    /// Fold multiple System export calls into the enum IIFE tail.
    pub fn set_system_export_folds<'b>(&mut self, export_names: impl IntoIterator<Item = &'b str>) {
        self.transformer.set_system_export_folds(export_names);
    }

    /// Emit an enum declaration
    /// Returns empty string for const enums (they are erased)
    pub fn emit_enum(&mut self, enum_idx: NodeIndex) -> String {
        let ir = self.transformer.transform_enum(enum_idx);
        let ir = match ir {
            Some(ir) => ir,
            None => return String::new(),
        };

        // ASTRef nodes (used for string literals to preserve source quote
        // style) require both arena and source text to print; without them
        // the printer falls back to "undefined".
        let arena = self.transformer.arena;
        let mut printer = match self.transformer.source_text {
            Some(text) => IRPrinter::with_arena_and_source(arena, text),
            None => IRPrinter::with_arena(arena),
        };
        printer.set_indent_level(self.indent_level);
        // Propagate the ES5/ES3 target so `ASTRef` arrows inside enum member
        // initializers (e.g. `(() => ...)()`) downlevel to function expressions,
        // matching `emit_enum_declaration`; otherwise the nested AST printer
        // defaults to native ES2015 arrow emission.
        printer.set_target_es5(self.transformer.target_es5);
        let result = printer.emit(&ir);
        result.to_string()
    }

    /// Get the enum name without emitting anything
    pub fn get_enum_name(&self, enum_idx: NodeIndex) -> String {
        self.transformer.get_enum_name(enum_idx)
    }

    /// Check if enum is a const enum
    pub fn is_const_enum_by_idx(&self, enum_idx: NodeIndex) -> bool {
        self.transformer.is_const_enum_by_idx(enum_idx)
    }
}
