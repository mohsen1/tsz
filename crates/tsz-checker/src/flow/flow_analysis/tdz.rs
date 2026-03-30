//! Temporal Dead Zone (TDZ) checking for block-scoped declarations.
//!
//! Detects use-before-declaration violations for `let`, `const`, `class`, and `enum`
//! in static blocks, computed properties, heritage clauses, and top-level code.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Check if a variable is used before its declaration in a static block.
    ///
    /// This detects Temporal Dead Zone (TDZ) violations where a block-scoped variable
    /// is accessed inside a class static block before it has been declared in the source.
    ///
    /// # Example
    /// ```typescript
    /// class C {
    ///   static {
    ///     console.log(x); // Error: x used before declaration
    ///   }
    /// }
    /// let x = 1;
    /// ```
    pub(crate) fn is_variable_used_before_declaration_in_static_block(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        // var and function are hoisted, so they don't have TDZ issues in this context.
        // Imports (ALIAS) are also hoisted or handled differently.
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::CLASS | symbol_flags::ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // const enums are compile-time only — no runtime binding, no TDZ.
        // Exception: with isolatedModules, const enums get runtime bindings
        // and are subject to TDZ just like regular enums.
        if symbol.flags & symbol_flags::CONST_ENUM != 0
            && symbol.flags & symbol_flags::REGULAR_ENUM == 0
            && !self.ctx.isolated_modules()
        {
            return false;
        }

        // Skip cross-file symbols — TDZ position comparison only valid within same file
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return false;
        }

        // 3. Get the declaration node
        // Prefer value_declaration, fall back to first declaration
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        // We ensure both nodes exist in the current arena
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        // If usage is after declaration, it's valid
        if usage_node.pos >= decl_node.end {
            return false;
        }

        // For CLASS symbols: if the usage is inside the class body (usage_node.pos > decl_node.pos),
        // the reference is a self-reference within the class's own static block and is NOT a TDZ
        // violation. For example, inside `static {}` of class C, referencing C itself is valid.
        // The class name is accessible within its own body.
        if symbol.flags & symbol_flags::CLASS != 0 && usage_node.pos > decl_node.pos {
            return false;
        }

        // 5. Check if usage is inside a static block
        // Use find_enclosing_static_block which walks up the AST and stops at function boundaries.
        // This ensures we only catch immediate usage, not usage inside a closure/function
        // defined within the static block (which would execute later).
        if self.find_enclosing_static_block(usage_idx).is_some() {
            return true;
        }

        false
    }

    /// Check if a variable is used before its declaration in a computed property.
    ///
    /// Computed property names are evaluated before the property declaration,
    /// creating a TDZ for the class being declared.
    pub(crate) fn is_variable_used_before_declaration_in_computed_property(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::CLASS | symbol_flags::ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // const enums are compile-time only — no runtime binding, no TDZ.
        // Exception: with isolatedModules, const enums get runtime bindings
        // and are subject to TDZ just like regular enums.
        if symbol.flags & symbol_flags::CONST_ENUM != 0
            && symbol.flags & symbol_flags::REGULAR_ENUM == 0
            && !self.ctx.isolated_modules()
        {
            return false;
        }

        // Skip cross-file symbols — TDZ position comparison only valid within same file
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return false;
        }

        // 3. Get the declaration node
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        // The declaration name itself is not a TDZ "use". Some checker flows
        // re-query the declared symbol while analyzing its own initializer or
        // synthetic JSDoc shape, and those should not report TS2448/TS2449/TS2450
        // on the declaration identifier.
        if usage_idx == decl_idx {
            return false;
        }
        if let Some(usage_ext) = self.ctx.arena.get_extended(usage_idx)
            && usage_ext.parent == decl_idx
        {
            return false;
        }

        if usage_node.pos >= decl_node.end {
            return false;
        }

        // 5. Check if usage is inside a computed property name.
        // Skip TDZ in ambient/type-only contexts (interfaces, type aliases,
        // declare class, .d.ts files) — these don't have runtime TDZ semantics.
        if self.find_enclosing_computed_property(usage_idx).is_some()
            && !self.ctx.arena.is_in_ambient_context(usage_idx)
        {
            return true;
        }

        false
    }

    /// Check if a variable is used before its declaration in a heritage clause.
    ///
    /// Heritage clauses (extends, implements) are evaluated before the class body,
    /// creating a TDZ for the class being declared.
    pub(crate) fn is_variable_used_before_declaration_in_heritage_clause(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // 1. Get the symbol
        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // 2. Check if it is a block-scoped variable (let, const, class, enum)
        let is_block_scoped = (symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::CLASS | symbol_flags::ENUM))
            != 0;

        if !is_block_scoped {
            return false;
        }

        // const enums are compile-time only — no runtime binding, no TDZ.
        // Exception: with isolatedModules, const enums get runtime bindings
        // and are subject to TDZ just like regular enums.
        if symbol.flags & symbol_flags::CONST_ENUM != 0
            && symbol.flags & symbol_flags::REGULAR_ENUM == 0
            && !self.ctx.isolated_modules()
        {
            return false;
        }

        // Skip TDZ check for type-only contexts (interface extends, type parameters, etc.)
        // Types are resolved at compile-time, so they don't have temporal dead zones.
        if self.is_in_type_only_context(usage_idx) {
            return false;
        }

        // Skip cross-file symbols — TDZ position comparison only makes sense
        // within the same file.
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return false;
        }

        // 3. Get the declaration node
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        // 4. Check textual order: Usage must be textually before declaration
        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if usage_node.pos >= decl_node.end {
            return false;
        }

        // 5. Check if usage is inside a heritage clause (extends/implements)
        if self.find_enclosing_heritage_clause(usage_idx).is_some() {
            return true;
        }

        false
    }

    /// TS2448/TS2449/TS2450: Check if a block-scoped declaration (class, enum,
    /// let/const) is used before its declaration in immediately executing code
    /// (not inside a function/method body).
    pub(crate) fn is_class_or_enum_used_before_declaration(
        &self,
        sym_id: SymbolId,
        usage_idx: NodeIndex,
    ) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return false;
        };

        // Applies to block-scoped declarations: class, enum, let/const
        let is_block_scoped = (symbol.flags
            & (symbol_flags::CLASS | symbol_flags::ENUM | symbol_flags::BLOCK_SCOPED_VARIABLE))
            != 0;
        if !is_block_scoped {
            return false;
        }

        // const enums are compile-time only — no runtime binding, no TDZ.
        // Exception: with isolatedModules, const enums get runtime bindings
        // and are subject to TDZ just like regular enums.
        if symbol.flags & symbol_flags::CONST_ENUM != 0
            && symbol.flags & symbol_flags::REGULAR_ENUM == 0
            && !self.ctx.isolated_modules()
        {
            return false;
        }

        // Skip TDZ check for type-only contexts (type annotations, typeof in types, etc.)
        // Types are resolved at compile-time, so they don't have temporal dead zones.
        if self.is_in_type_only_context(usage_idx) {
            return false;
        }

        // Skip check for cross-file symbols (imported from another file).
        // Position comparison only makes sense within the same file.
        if symbol.import_module.is_some() {
            return false;
        }
        let is_cross_file = symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32;

        // TDZ is a same-file concept — cross-file references always refer to
        // exports from another file, which are evaluated when that file loads.
        // No TDZ applies regardless of file ordering.
        if is_cross_file {
            return false;
        }

        // In multi-file mode, symbol declarations may reference nodes in another
        // file's arena.  `self.ctx.arena` only contains the *current* file, so
        // looking up the declaration index would yield an unrelated node whose
        // position comparison is meaningless.  Detect this by verifying that the
        // node found at the declaration index really IS a class / enum / variable
        // declaration — if it isn't, the index came from a different arena.
        let is_multi_file = self.ctx.all_arenas.is_some();

        // Get the declaration position.
        // For merged symbols (e.g., namespace A + class A), the value_declaration
        // may point to the namespace rather than the class. For CLASS symbols, we
        // must find the actual class declaration to get the correct TDZ position.
        let mut decl_idx = if symbol.flags & symbol_flags::CLASS != 0 {
            // Prefer the CLASS_DECLARATION among all declarations
            let class_decl = symbol.declarations.iter().find(|&&d| {
                self.ctx
                    .arena
                    .get(d)
                    .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_DECLARATION)
            });
            if let Some(&class_d) = class_decl {
                class_d
            } else if symbol.value_declaration.is_some() {
                symbol.value_declaration
            } else if let Some(&first_decl) = symbol.declarations.first() {
                first_decl
            } else {
                return false;
            }
        } else if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return false;
        };

        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return false;
        };

        let mut decl_node_opt = self.ctx.arena.get(decl_idx);
        let mut decl_arena = self.ctx.arena;

        if is_cross_file
            && let Some(arenas) = self.ctx.all_arenas.as_ref()
            && let Some(arena) = arenas.get(symbol.decl_file_idx as usize)
        {
            decl_node_opt = arena.get(decl_idx);
            decl_arena = arena.as_ref();
        }

        if let Some(decl_node) = decl_node_opt
            && decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ext) = decl_arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = decl_arena.get(ext.parent)
            && matches!(
                parent_node.kind,
                syntax_kind_ext::VARIABLE_DECLARATION
                    | syntax_kind_ext::BINDING_ELEMENT
                    | syntax_kind_ext::PARAMETER
            )
        {
            // Destructured bindings are declared on the enclosing binding element,
            // not on the bound identifier token itself. Normalize to the container
            // so self-references in default initializers count as "inside the
            // declaration" for TDZ checks.
            decl_idx = ext.parent;
            decl_node_opt = Some(parent_node);
        }

        let Some(decl_node) = decl_node_opt else {
            return false;
        };

        // The declaration name itself is not a TDZ use. Some later checker
        // flows re-enter the symbol while validating the declaration's own
        // initializer/JSDoc shape, and those should not report TS2448 on the
        // declared name token.
        if usage_idx == decl_idx {
            return false;
        }
        if let Some(usage_ext) = self.ctx.arena.get_extended(usage_idx)
            && usage_ext.parent == decl_idx
            && matches!(
                decl_node.kind,
                syntax_kind_ext::VARIABLE_DECLARATION
                    | syntax_kind_ext::BINDING_ELEMENT
                    | syntax_kind_ext::PARAMETER
            )
        {
            return false;
        }

        // In multi-file mode, validate the declaration node kind matches the
        // symbol.  A mismatch means the node index is from a different file's
        // arena and should not be compared.
        if is_multi_file && !is_cross_file {
            let is_class = symbol.flags & symbol_flags::CLASS != 0;
            let is_enum = symbol.flags & symbol_flags::ENUM != 0;
            let is_var = symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0;
            let kind_ok = (is_class
                && (decl_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || decl_node.kind == syntax_kind_ext::CLASS_EXPRESSION))
                || (is_enum && decl_node.kind == syntax_kind_ext::ENUM_DECLARATION)
                || (is_var
                    && (decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                        || decl_node.kind == syntax_kind_ext::PARAMETER
                        || decl_node.kind == syntax_kind_ext::BINDING_ELEMENT
                        || decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16));
            if !kind_ok {
                return false;
            }
        }

        // Skip ambient declarations — `declare class`/`declare enum`/`declare const`
        // are type-level and have no TDZ. In multi-file mode, search all arenas since
        // decl_idx may point to a node in another file's arena.
        if is_cross_file {
            // Use the cross-file arena's ambient context check which walks up the AST
            // to detect `declare` keyword, AMBIENT flag, or implicit ambient context.
            // This covers classes, enums, AND variables (e.g., `declare const a: string`
            // in .d.ts files).
            if decl_arena.is_in_ambient_context(decl_idx) {
                return false;
            }
        } else if self.is_ambient_declaration(decl_idx) {
            return false;
        }

        // Only flag if usage is before declaration in source order
        // EXCEPT for block-scoped variables, which are also in TDZ during their own initializer,
        // and class decorator arguments, which execute before the class binding is created.
        let is_var = symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0;
        let is_class = symbol.flags & symbol_flags::CLASS != 0;
        let mut could_be_in_initializer = false;
        let in_for_of_header_expression = !is_cross_file
            && is_var
            && self.is_in_for_of_header_expression_of_declaration(usage_idx, decl_idx);
        if !is_cross_file && usage_node.pos >= decl_node.pos {
            if is_var && (usage_node.pos <= decl_node.end || in_for_of_header_expression) {
                // It might be in the initializer. We will confirm via AST walk.
                could_be_in_initializer = true;
            } else if is_class
                && usage_node.pos <= decl_node.end
                && self.is_in_decorator_of_declaration(usage_idx, decl_idx)
            {
                // Decorator arguments on the class itself execute before the class
                // binding is created, so they ARE in TDZ even though pos >= decl.pos.
                // Fall through to emit the TDZ error.
            } else {
                return false;
            }
        }

        // Find the declaration's enclosing function-like container (or source file).
        // This is the scope that "owns" both the declaration and (potentially) the usage.
        let decl_container = if is_cross_file {
            None // Walk up to source file
        } else {
            Some(self.find_enclosing_function_or_source_file(decl_idx))
        };

        // Check if the usage is inside a decorator of the class — decorator arguments
        // execute at class definition time (not deferred like property initializers),
        // so the function-like/property-initializer bail-outs below should not apply.
        let in_class_decorator = is_class
            && usage_node.pos >= decl_node.pos
            && usage_node.pos <= decl_node.end
            && self.is_in_decorator_of_declaration(usage_idx, decl_idx);

        // Walk up from usage: if we hit a function-like boundary BEFORE reaching
        // the declaration's container, the usage is in deferred code (a nested
        // function/arrow/method) and is NOT a TDZ violation.
        // If we reach the declaration's container without crossing a function
        // boundary, the usage executes immediately and IS a violation.
        let mut current = usage_idx;
        let mut found_decl_in_path = false;
        while current.is_some() {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if current == decl_idx {
                found_decl_in_path = true;
            }
            // If we reached the declaration container, stop - same scope means TDZ
            if Some(current) == decl_container {
                break;
            }
            // If we reach a function-like boundary before the decl container,
            // the usage is deferred and not a TDZ violation.
            // Exception: IIFEs (immediately invoked function expressions) execute
            // immediately, so they ARE TDZ violations.
            // Exception: Decorator arguments execute at class definition time,
            // so function-like boundaries within decorators don't defer execution.
            if node.is_function_like()
                && !self.ctx.arena.is_immediately_invoked(current)
                && !in_class_decorator
            {
                return false;
            }
            // IIFE - continue walking up, this function executes immediately
            // Non-static class property initializers run during constructor execution,
            // which is deferred — not a TDZ violation for class declarations.
            // Exception: decorator arguments on non-static properties still execute
            // at class definition time.
            if node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.ctx.arena.get_property_decl(node)
                && !self.has_static_modifier(&prop.modifiers)
                && !in_class_decorator
                && !self.is_in_decorator_of_declaration(usage_idx, current)
            {
                return false;
            }
            // Export assignments (`export = X` / `export default X`) are not TDZ
            // violations: the compiler reorders them after all declarations, so
            // the referenced class/variable is initialized by the time the export
            // binding is created.
            if node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                return false;
            }
            // Stop at source file
            if node.kind == syntax_kind_ext::SOURCE_FILE {
                break;
            }
            // Walk to parent
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        if could_be_in_initializer && !found_decl_in_path && !in_for_of_header_expression {
            // It was >= pos, but wasn't actually inside the declaration's AST.
            // This means it's strictly AFTER the declaration.
            return false;
        }

        true
    }

    /// Check if the usage node is inside a decorator expression that belongs to
    /// the given class declaration. Walks up from `usage_idx` looking for a
    /// DECORATOR ancestor whose owning class/member is `decl_idx`.
    ///
    /// Decorator arguments execute before the class binding is created, so
    /// references to the class in decorator arguments are TDZ violations.
    /// This applies to class-level decorators, member decorators, and IIFEs
    /// within decorator arguments.
    fn is_in_decorator_of_declaration(&self, usage_idx: NodeIndex, decl_idx: NodeIndex) -> bool {
        let mut current = usage_idx;
        while current.is_some() {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };
            // Reached the class declaration itself — not inside a decorator.
            if current == decl_idx {
                return false;
            }
            if node.kind == syntax_kind_ext::DECORATOR {
                // Found a decorator. Check if it belongs to the class declaration
                // or one of its members by walking up to see if we reach decl_idx.
                let mut ancestor = current;
                while let Some(ext) = self.ctx.arena.get_extended(ancestor) {
                    if !ext.parent.is_some() {
                        break;
                    }
                    if ext.parent == decl_idx {
                        return true;
                    }
                    // Stop at source file to avoid infinite loops
                    if let Some(parent_node) = self.ctx.arena.get(ext.parent)
                        && parent_node.kind == syntax_kind_ext::SOURCE_FILE
                    {
                        break;
                    }
                    ancestor = ext.parent;
                }
                return false;
            }
            // Non-IIFE function-like boundary means the reference is deferred,
            // so it's NOT a TDZ violation. E.g., `@dec(() => C)` is OK.
            if node.is_function_like() && !self.ctx.arena.is_immediately_invoked(current) {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            current = ext.parent;
        }
        false
    }

    /// Check if a node is in a type-only context (type annotation, type query, heritage clause).
    /// References in type-only positions don't need TDZ checks because types are
    /// resolved at compile-time, not runtime.
    fn is_in_type_only_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = idx;
        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                return false;
            };

            // Type node kinds indicate we're in a type-only context
            match parent_node.kind {
                // Core type nodes
                syntax_kind_ext::TYPE_PREDICATE
                | syntax_kind_ext::TYPE_REFERENCE
                | syntax_kind_ext::FUNCTION_TYPE
                | syntax_kind_ext::CONSTRUCTOR_TYPE
                | syntax_kind_ext::TYPE_QUERY // typeof T in type position
                | syntax_kind_ext::TYPE_LITERAL
                | syntax_kind_ext::ARRAY_TYPE
                | syntax_kind_ext::TUPLE_TYPE
                | syntax_kind_ext::OPTIONAL_TYPE
                | syntax_kind_ext::REST_TYPE
                | syntax_kind_ext::UNION_TYPE
                | syntax_kind_ext::INTERSECTION_TYPE
                | syntax_kind_ext::CONDITIONAL_TYPE
                | syntax_kind_ext::INFER_TYPE
                | syntax_kind_ext::PARENTHESIZED_TYPE
                | syntax_kind_ext::THIS_TYPE
                | syntax_kind_ext::TYPE_OPERATOR
                | syntax_kind_ext::INDEXED_ACCESS_TYPE
                | syntax_kind_ext::MAPPED_TYPE
                | syntax_kind_ext::LITERAL_TYPE
                | syntax_kind_ext::NAMED_TUPLE_MEMBER
                | syntax_kind_ext::TEMPLATE_LITERAL_TYPE
                | syntax_kind_ext::IMPORT_TYPE
                | syntax_kind_ext::HERITAGE_CLAUSE
                | syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                // Declaration containers that are purely type-level
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION => return true,

                // Stop at boundaries that separate type from value context
                syntax_kind_ext::TYPE_OF_EXPRESSION // typeof x in value position
                | syntax_kind_ext::SOURCE_FILE => return false,

                _ => {
                    // Continue walking up
                    current = ext.parent;
                }
            }
        }
        false
    }
}
