//! `NodeArena` typed data accessors and semantic utility methods.
//!
//! This module contains the core `get_*` accessor methods for retrieving typed
//! node data from the arena's side pools, plus semantic utility methods like
//! `skip_parenthesized`, `is_namespace_instantiated`, and `is_in_ambient_context`.
//!
//! `NodeView`, `NodeInfo`, and `NodeAccess` are in `node_view.rs`.
use super::base::NodeIndex;
use super::node::{
    AccessExprData, AccessorData, ArrayTypeData, BinaryExprData, BindingElementData,
    BindingPatternData, BlockData, CallExprData, CaseClauseData, CatchClauseData, ClassData,
    CompositeTypeData, ComputedPropertyData, ConditionalExprData, ConditionalTypeData,
    ConstructorData, DecoratorData, EnumData, EnumMemberData, ExportAssignmentData, ExportDeclData,
    ExprStatementData, ExprWithTypeArgsData, ExtendedNodeInfo, ForInOfData, FunctionData,
    FunctionTypeData, HeritageData, IdentifierData, IfStatementData, ImportAttributeData,
    ImportAttributesData, ImportClauseData, ImportDeclData, IndexSignatureData,
    IndexedAccessTypeData, InferTypeData, InterfaceData, JsxAttributeData, JsxAttributesData,
    JsxClosingData, JsxElementData, JsxExpressionData, JsxFragmentData, JsxNamespacedNameData,
    JsxOpeningData, JsxSpreadAttributeData, JsxTextData, JumpData, LabeledData, LiteralData,
    LiteralExprData, LiteralTypeData, LoopData, MappedTypeData, MethodDeclData, ModuleBlockData,
    ModuleData, NamedImportsData, NamedTupleMemberData, Node, NodeArena, NodeArenaInner,
    ParameterData, ParenthesizedData, PropertyAssignmentData, PropertyDeclData, QualifiedNameData,
    ReturnData, ShorthandPropertyData, SignatureData, SourceFileData, SpecifierData, SpreadData,
    SwitchData, TaggedTemplateData, TemplateExprData, TemplateLiteralTypeData, TemplateSpanData,
    TryData, TupleTypeData, TypeAliasData, TypeAssertionData, TypeLiteralData, TypeOperatorData,
    TypeParameterData, TypePredicateData, TypeQueryData, TypeRefData, UnaryExprData,
    UnaryExprDataEx, VariableData, VariableDeclarationData, WrappedTypeData,
};
use super::syntax_kind_ext::{
    ARRAY_BINDING_PATTERN, ARROW_FUNCTION, AS_EXPRESSION, BINARY_EXPRESSION, BLOCK,
    CLASS_DECLARATION, CLASS_EXPRESSION, CONSTRUCTOR, DEBUGGER_STATEMENT, ENUM_DECLARATION,
    EXPORT_ASSIGNMENT, EXPORT_DECLARATION, EXPORT_SPECIFIER, EXPRESSION_WITH_TYPE_ARGUMENTS,
    FUNCTION_DECLARATION, FUNCTION_EXPRESSION, GET_ACCESSOR, IMPORT_DECLARATION,
    IMPORT_EQUALS_DECLARATION, IMPORT_TYPE, INDEX_SIGNATURE, INTERFACE_DECLARATION,
    METHOD_DECLARATION, METHOD_SIGNATURE, MODULE_BLOCK, MODULE_DECLARATION, NAMED_EXPORTS,
    NAMESPACE_EXPORT_DECLARATION, NON_NULL_EXPRESSION, OBJECT_BINDING_PATTERN, PARAMETER,
    PARENTHESIZED_EXPRESSION, PROPERTY_DECLARATION, PROPERTY_SIGNATURE, SATISFIES_EXPRESSION,
    SET_ACCESSOR, TYPE_ALIAS_DECLARATION, TYPE_ASSERTION, TYPE_PREDICATE, VARIABLE_DECLARATION,
    VARIABLE_DECLARATION_LIST, VARIABLE_STATEMENT,
};

/// tsc's `ModuleInstanceState`: how much of a namespace/module declaration
/// survives erasure. Computed syntactically by
/// [`NodeArenaInner::module_instance_state`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ModuleInstanceState {
    /// Only type-level declarations; the namespace is fully erased.
    NonInstantiated,
    /// Has runtime members; the namespace emits a value.
    Instantiated,
    /// The only runtime members are `const enum`s, which are erased unless
    /// `preserveConstEnums` keeps them.
    ConstEnumOnly,
}

/// Generate the single-condition typed getter bodies (`get_X(&Node) ->
/// Option<&XData>`) that were previously hand-written ~per node kind. Each
/// arm checks `has_data()` and that the node's kind is one of `[KIND, ..]`
/// before indexing the typed side-pool — the exact shape `define_at_accessors!`
/// wrappers delegate to.
macro_rules! define_kind_getters {
    ($(
        $(#[$meta:meta])*
        $name:ident => $field:ident -> $ret:ty, [$($kind:ident),+ $(,)?]
    );* $(;)?) => {
        impl NodeArenaInner {
            $(
                $(#[$meta])*
                #[inline]
                #[must_use]
                pub fn $name(&self, node: &Node) -> Option<&$ret> {
                    if node.has_data()
                        && ($(node.kind == $crate::parser::syntax_kind_ext::$kind)||+)
                    {
                        self.$field.get(node.data_index as usize)
                    } else {
                        None
                    }
                }
            )*
        }
    };
}
pub(crate) use define_kind_getters;

impl NodeArenaInner {
    /// Get a thin node by index
    #[inline]
    #[must_use]
    pub fn get(&self, index: NodeIndex) -> Option<&Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get(index.0 as usize)
        }
    }

    /// Get a mutable thin node by index
    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, index: NodeIndex) -> Option<&mut Node> {
        if index.is_none() {
            None
        } else {
            self.nodes.get_mut(index.0 as usize)
        }
    }

    /// Get the source start position of a node by index. Returns `None` if
    /// the index is `NodeIndex::NONE` or out of bounds. Inherent helper for
    /// the common `arena.get(idx).map(|n| n.pos)` pattern.
    #[inline]
    #[must_use]
    pub fn pos_at(&self, index: NodeIndex) -> Option<u32> {
        self.get(index).map(|n| n.pos)
    }

    /// Get the source end position of a node by index. Returns `None` if
    /// the index is `NodeIndex::NONE` or out of bounds. Inherent helper for
    /// the common `arena.get(idx).map(|n| n.end)` pattern.
    #[inline]
    #[must_use]
    pub fn end_at(&self, index: NodeIndex) -> Option<u32> {
        self.get(index).map(|n| n.end)
    }

    /// Get the `(pos, end)` source range of a node by index. Returns `None`
    /// if the index is `NodeIndex::NONE` or out of bounds. Inherent helper
    /// for the common `arena.get(idx).map(|n| (n.pos, n.end))` pattern used
    /// by emitter source-range plumbing and diagnostics.
    #[inline]
    #[must_use]
    pub fn pos_end_at(&self, index: NodeIndex) -> Option<(u32, u32)> {
        self.get(index).map(|n| (n.pos, n.end))
    }

    /// Get the syntax kind (raw `u16`) of a node by index. Returns `None` if
    /// the index is `NodeIndex::NONE` or out of bounds. Inherent mirror of
    /// [`NodeAccess::kind`] — lets callers skip the trait import when they
    /// only need the kind.
    #[inline]
    #[must_use]
    pub fn kind_at(&self, index: NodeIndex) -> Option<u16> {
        self.get(index).map(|n| n.kind)
    }

    /// Get extended info for a node
    #[inline]
    #[must_use]
    pub fn get_extended(&self, index: NodeIndex) -> Option<&ExtendedNodeInfo> {
        if index.is_none() {
            None
        } else {
            self.extended_info.get(index.0 as usize)
        }
    }

    /// Get mutable extended info for a node
    #[inline]
    #[must_use]
    pub fn get_extended_mut(&mut self, index: NodeIndex) -> Option<&mut ExtendedNodeInfo> {
        if index.is_none() {
            None
        } else {
            self.extended_info.get_mut(index.0 as usize)
        }
    }

    /// Get the parent index of a node via its extended info. Returns `None`
    /// if the index is `NodeIndex::NONE` or out of bounds. Inherent helper
    /// for the very common `arena.get_extended(idx).map(|ext| ext.parent)`
    /// pattern used by ~140 parent-walk call sites across checker/emitter.
    ///
    /// A root node returns `Some(NodeIndex::NONE)`; callers that want to
    /// distinguish "root" from "unknown" should check `is_none()` on the
    /// inner index.
    #[inline]
    #[must_use]
    pub fn parent_of(&self, index: NodeIndex) -> Option<NodeIndex> {
        self.get_extended(index).map(|ext| ext.parent)
    }

    /// Get identifier data for a node.
    /// Returns None if node is not an identifier or has no data.
    #[inline]
    #[must_use]
    pub fn get_identifier(&self, node: &Node) -> Option<&IdentifierData> {
        use tsz_scanner::SyntaxKind;
        if node.has_data()
            && (node.kind == SyntaxKind::Identifier as u16
                || node.kind == SyntaxKind::PrivateIdentifier as u16)
        {
            self.identifiers.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Returns `true` when the node is an identifier synthesized by parser
    /// error recovery — i.e. an empty-text identifier with `Atom::NONE`
    /// produced by helpers like `create_missing_expression`. Distinguishes
    /// recovery placeholders from genuine empty-named identifiers (which
    /// the scanner would never produce, but downstream synthesizers might).
    ///
    /// Use this to suppress cascading diagnostics or skip semantic checks
    /// that would treat a placeholder as a meaningful name. Codifies the
    /// implicit `escaped_text.is_empty() && atom == Atom::NONE` heuristic
    /// that several call sites already use ad-hoc, into a stable API.
    ///
    /// Robustness audit (PR #L, item 12 in
    /// `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`).
    #[inline]
    #[must_use]
    pub fn is_missing_recovery_identifier(&self, index: NodeIndex) -> bool {
        let Some(node) = self.get(index) else {
            return false;
        };
        let Some(ident) = self.get_identifier(node) else {
            return false;
        };
        ident.atom == tsz_common::interner::AstAtom::NONE && ident.escaped_text.is_empty()
    }

    /// Name of the first class-body member dropped by parser error recovery
    /// whose shape matches tsc's `var <name>() { }` recovery emit and whose
    /// position falls inside `[pos, end)` — normally a class node's span.
    ///
    /// The class emitters use this to append tsc's recovery tail
    /// (`var <name>;` plus a recovered function/arrow expression) after the
    /// class output, mirroring tsc parsing the malformed member as statements
    /// following the aborted class body. See
    /// [`crate::parser::node::ClassBodyVarFnRecovery`].
    #[must_use]
    pub fn class_body_var_fn_recovery_name_in_span(&self, pos: u32, end: u32) -> Option<&str> {
        self.class_body_var_fn_recoveries
            .iter()
            .filter(|recovery| recovery.pos >= pos && recovery.pos < end)
            .min_by_key(|recovery| recovery.pos)
            .map(|recovery| recovery.name.as_str())
    }

    /// Get the borrowed text of an `Identifier` node. Returns `None` for any
    /// other kind, including `PrivateIdentifier` -- mirrors the common
    /// caller-side pattern that pre-filters on `SyntaxKind::Identifier`
    /// before extracting identifier text.
    #[inline]
    #[must_use]
    pub fn identifier_text(&self, index: NodeIndex) -> Option<&str> {
        use tsz_scanner::SyntaxKind;
        let node = self.get(index)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            self.get_identifier(node)
                .map(|id| self.resolve_identifier_text(id))
        } else {
            None
        }
    }

    /// Get the owned text of an `Identifier` node. Returns `None` for any
    /// other kind, including `PrivateIdentifier`.
    #[inline]
    #[must_use]
    pub fn identifier_text_owned(&self, index: NodeIndex) -> Option<String> {
        self.identifier_text(index).map(str::to_owned)
    }

    /// Get literal data for a node.
    /// Returns None if node is not a literal or has no data.
    #[inline]
    #[must_use]
    pub fn get_literal(&self, node: &Node) -> Option<&LiteralData> {
        use tsz_scanner::SyntaxKind;
        if node.has_data()
            && matches!(node.kind,
                k if k == SyntaxKind::StringLiteral as u16 ||
                     k == SyntaxKind::NumericLiteral as u16 ||
                     k == SyntaxKind::BigIntLiteral as u16 ||
                     k == SyntaxKind::RegularExpressionLiteral as u16 ||
                     k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 ||
                     k == SyntaxKind::TemplateHead as u16 ||
                     k == SyntaxKind::TemplateMiddle as u16 ||
                     k == SyntaxKind::TemplateTail as u16
            )
        {
            self.literals.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Whether `index` is an object-literal expression with no member elements
    /// (`{}`).
    ///
    /// Mirrors tsc's `getExpandoInitializer` emptiness test: only an empty
    /// object literal is a valid expando host, because its shape is open and a
    /// later `x.p = …` write declares a new member. A non-empty literal
    /// (`{ a: 1 }`) has a closed shape, so the same write is an ordinary
    /// property assignment (`TS2339` under `noImplicitAny`). A prototype
    /// assignment (`X.prototype = {…}`) relaxes the rule and is gated
    /// separately by its caller.
    #[inline]
    #[must_use]
    pub fn is_empty_object_literal(&self, index: NodeIndex) -> bool {
        self.get(index).is_some_and(|node| {
            node.kind == super::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                && self
                    .get_literal_expr(node)
                    .is_some_and(|lit| lit.elements.nodes.is_empty())
        })
    }

    /// Check if a function-like node is immediately invoked (IIFE pattern).
    ///
    /// Detects patterns like `(function() {})()`, `(() => expr)()`,
    /// `((fn))()` (arbitrary paren nesting), and `new (function() {})()`.
    #[must_use]
    pub fn is_immediately_invoked(&self, func_idx: NodeIndex) -> bool {
        use super::syntax_kind_ext::{CALL_EXPRESSION, NEW_EXPRESSION, PARENTHESIZED_EXPRESSION};

        let mut current = func_idx;
        // Guard against pathological nesting depth
        for _ in 0..100 {
            let Some(ext) = self.get_extended(current) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.get(ext.parent) else {
                return false;
            };
            if parent_node.kind == PARENTHESIZED_EXPRESSION {
                current = ext.parent;
                continue;
            }
            if (parent_node.kind == CALL_EXPRESSION || parent_node.kind == NEW_EXPRESSION)
                && let Some(call) = self.get_call_expr(parent_node)
                && call.expression == current
            {
                return true;
            }
            return false;
        }
        false
    }

    /// Skip through parenthesized expressions to the underlying expression.
    ///
    /// Unwraps any number of `(expr)` wrappers.
    /// Uses a bounded loop (max 100 iterations) to guard against pathological input.
    #[must_use]
    pub fn skip_parenthesized(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.get(idx) else {
                return idx;
            };
            if node.kind == PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            return idx;
        }
        idx
    }

    /// Skip through parenthesized, non-null assertion, and comma-expression wrappers.
    ///
    /// Unwraps `(expr)`, `expr!`, and comma expressions (`(a, b)`).
    /// Uses a bounded loop (max 100 iterations) to guard against pathological input.
    #[must_use]
    pub fn skip_parenthesized_and_assertions_and_comma(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.get(idx) else {
                return idx;
            };
            if node.kind == PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == NON_NULL_EXPRESSION
                && let Some(unary) = self.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if node.kind == BINARY_EXPRESSION
                && let Some(binary) = self.get_binary_expr(node)
                && binary.operator_token == tsz_scanner::SyntaxKind::CommaToken as u16
            {
                idx = binary.right;
                continue;
            }

            return idx;
        }
        idx
    }

    /// Skip through parenthesized, non-null assertion, and type assertion expressions.
    ///
    /// Unwraps `(expr)`, `expr!`, `expr as T`, `<T>expr`, and `expr satisfies T` wrappers.
    /// Uses a bounded loop (max 100 iterations) to guard against pathological input.
    #[must_use]
    pub fn skip_parenthesized_and_assertions(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.get(idx) else {
                return idx;
            };
            if node.kind == PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == NON_NULL_EXPRESSION
                && let Some(unary) = self.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if (node.kind == TYPE_ASSERTION
                || node.kind == AS_EXPRESSION
                || node.kind == SATISFIES_EXPRESSION)
                && let Some(assertion) = self.get_type_assertion(node)
            {
                idx = assertion.expression;
                continue;
            }
            return idx;
        }
        idx
    }

    /// Skip TypeScript outer-expression wrappers used when discovering a
    /// call-like expression's underlying callee.
    ///
    /// In addition to parentheses and assertions, a generic callee such as
    /// `object.method<T>` is represented by `ExpressionWithTypeArguments`.
    /// Unwrapping it preserves the property/element receiver for `this`.
    #[must_use]
    pub fn skip_outer_expressions(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let stripped = self.skip_parenthesized_and_assertions(idx);
            if stripped != idx {
                idx = stripped;
                continue;
            }
            let Some(node) = self.get(idx) else {
                return idx;
            };
            if node.kind == EXPRESSION_WITH_TYPE_ARGUMENTS
                && let Some(type_args) = self.get_expr_type_args(node)
            {
                idx = type_args.expression;
                continue;
            }
            return idx;
        }
        idx
    }

    /// Check whether a namespace/module declaration is instantiated (has runtime value declarations).
    ///
    /// Returns `true` if the namespace contains value declarations (variables, functions,
    /// classes, enums, expression statements, export assignments), or is a
    /// `NAMESPACE_EXPORT_DECLARATION` (`export as namespace X`), which always produces a
    /// runtime global.
    ///
    /// Recursively walks dotted namespaces (`namespace Foo.Bar`) and `EXPORT_DECLARATION`
    /// wrappers to find the innermost `MODULE_BLOCK`, then checks each statement.
    #[must_use]
    pub fn is_namespace_instantiated(&self, namespace_idx: NodeIndex) -> bool {
        let Some(node) = self.get(namespace_idx) else {
            return false;
        };

        // `export as namespace X` always creates a global runtime value.
        if node.kind == NAMESPACE_EXPORT_DECLARATION {
            return true;
        }

        if node.kind != MODULE_DECLARATION {
            return false;
        }
        let Some(module_decl) = self.get_module(node) else {
            return false;
        };
        self.module_body_has_runtime_members(module_decl.body)
    }

    /// Check whether a module body contains runtime value declarations.
    ///
    /// Helper for [`is_namespace_instantiated`]. Handles dotted namespaces
    /// (body is another `MODULE_DECLARATION`) and `MODULE_BLOCK` bodies.
    fn module_body_has_runtime_members(&self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let Some(body_node) = self.get(body_idx) else {
            return false;
        };

        // Dotted namespace: `namespace Foo.Bar { ... }` — recurse into inner module
        if body_node.kind == MODULE_DECLARATION {
            return self.is_namespace_instantiated(body_idx);
        }

        if body_node.kind != MODULE_BLOCK {
            return false;
        }

        let Some(module_block) = self.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = &module_block.statements else {
            return false;
        };

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.get(stmt_idx) else {
                continue;
            };
            if self.is_runtime_module_statement(stmt_node, stmt_idx) {
                return true;
            }
        }

        false
    }

    /// tsc's `GetModuleInstanceState` for a namespace/module declaration.
    ///
    /// Distinguishes the three states tsc's classifier produces:
    /// - `NonInstantiated`: the body holds only type-level declarations
    ///   (interfaces, type aliases, non-exported imports, other
    ///   non-instantiated modules) — the namespace is fully erased.
    /// - `ConstEnumOnly`: the only runtime members are `const enum`
    ///   declarations, which are erased unless `preserveConstEnums` keeps
    ///   them.
    /// - `Instantiated`: anything else, including a bodyless declaration
    ///   (`declare module "m";`), which tsc treats as instantiated.
    ///
    /// Purely syntactic, like [`Self::is_namespace_instantiated`]: named
    /// re-export specifiers (`export { name }`) are conservatively treated as
    /// instantiating instead of resolving each alias target the way tsc's
    /// `getModuleInstanceStateForAliasTarget` does.
    #[must_use]
    pub fn module_instance_state(&self, module_idx: NodeIndex) -> ModuleInstanceState {
        let Some(node) = self.get(module_idx) else {
            return ModuleInstanceState::NonInstantiated;
        };
        if node.kind != MODULE_DECLARATION {
            return ModuleInstanceState::NonInstantiated;
        }
        let Some(module_decl) = self.get_module(node) else {
            return ModuleInstanceState::NonInstantiated;
        };
        if module_decl.body.is_none() {
            // `declare module "m";` with no body is instantiated in tsc.
            return ModuleInstanceState::Instantiated;
        }
        self.module_body_instance_state(module_decl.body)
    }

    /// Instance state of a module body (dotted-namespace chain or block).
    fn module_body_instance_state(&self, body_idx: NodeIndex) -> ModuleInstanceState {
        let Some(body_node) = self.get(body_idx) else {
            return ModuleInstanceState::NonInstantiated;
        };
        // Dotted namespace: `namespace Foo.Bar { ... }` — the state is the
        // inner module's state.
        if body_node.kind == MODULE_DECLARATION {
            return self.module_instance_state(body_idx);
        }
        if body_node.kind != MODULE_BLOCK {
            return ModuleInstanceState::NonInstantiated;
        }
        let Some(module_block) = self.get_module_block(body_node) else {
            return ModuleInstanceState::NonInstantiated;
        };
        let Some(statements) = &module_block.statements else {
            return ModuleInstanceState::NonInstantiated;
        };
        let mut state = ModuleInstanceState::NonInstantiated;
        for &stmt_idx in &statements.nodes {
            match self.module_statement_instance_state(stmt_idx) {
                ModuleInstanceState::Instantiated => return ModuleInstanceState::Instantiated,
                ModuleInstanceState::ConstEnumOnly => state = ModuleInstanceState::ConstEnumOnly,
                ModuleInstanceState::NonInstantiated => {}
            }
        }
        state
    }

    /// Instance state contributed by one statement of a module block,
    /// mirroring tsc's `getModuleInstanceStateWorker`.
    fn module_statement_instance_state(&self, stmt_idx: NodeIndex) -> ModuleInstanceState {
        use tsz_scanner::SyntaxKind;
        let Some(node) = self.get(stmt_idx) else {
            return ModuleInstanceState::NonInstantiated;
        };
        match node.kind {
            k if k == INTERFACE_DECLARATION || k == TYPE_ALIAS_DECLARATION => {
                ModuleInstanceState::NonInstantiated
            }
            ENUM_DECLARATION => {
                let is_const = self.get_enum(node).is_some_and(|enum_data| {
                    self.has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
                });
                if is_const {
                    ModuleInstanceState::ConstEnumOnly
                } else {
                    ModuleInstanceState::Instantiated
                }
            }
            k if k == IMPORT_DECLARATION || k == IMPORT_EQUALS_DECLARATION => {
                // tsc: a non-exported import never instantiates; an exported
                // one falls through to the instantiated default.
                let exported = self.get_declaration_modifiers(node).is_some_and(|mods| {
                    self.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                if exported {
                    ModuleInstanceState::Instantiated
                } else {
                    ModuleInstanceState::NonInstantiated
                }
            }
            EXPORT_DECLARATION => {
                if let Some(export_decl) = self.get_export_decl(node)
                    && export_decl.export_clause.is_some()
                {
                    let Some(clause) = self.get(export_decl.export_clause) else {
                        return ModuleInstanceState::NonInstantiated;
                    };
                    if clause.kind == NAMED_EXPORTS {
                        // Conservative: tsc resolves each specifier's alias
                        // target; treat named re-exports as instantiating.
                        ModuleInstanceState::Instantiated
                    } else {
                        // `export <declaration>` — classify the wrapped
                        // declaration itself.
                        self.module_statement_instance_state(export_decl.export_clause)
                    }
                } else {
                    ModuleInstanceState::Instantiated
                }
            }
            MODULE_DECLARATION => self.module_instance_state(stmt_idx),
            // Everything else (variables, functions, classes, expression
            // statements, control flow, `export as namespace`, export
            // assignments, ...) is runtime code.
            _ => ModuleInstanceState::Instantiated,
        }
    }

    /// Check if a statement inside a module block is a runtime value declaration.
    ///
    /// Uses tsc's inverse logic: a module is uninstantiated if it contains ONLY
    /// type-level declarations (interfaces, type aliases, non-exported imports).
    /// Any other statement (try, if, for, expression, variable, etc.) makes the
    /// module instantiated.
    fn is_runtime_module_statement(&self, node: &Node, node_idx: NodeIndex) -> bool {
        match node.kind {
            // Type-only declarations — never instantiate a module
            k if k == INTERFACE_DECLARATION || k == TYPE_ALIAS_DECLARATION => false,

            // Import declarations — non-instantiated (they don't produce runtime code
            // in the namespace itself, even if exported)
            k if k == IMPORT_DECLARATION || k == IMPORT_EQUALS_DECLARATION => false,

            // Export declarations — check what's being exported
            k if k == EXPORT_DECLARATION => {
                if let Some(export_decl) = self.get_export_decl(node)
                    && let Some(clause) = self.get(export_decl.export_clause)
                {
                    match clause.kind {
                        k if k == VARIABLE_STATEMENT
                            || k == FUNCTION_DECLARATION
                            || k == CLASS_DECLARATION
                            || k == ENUM_DECLARATION =>
                        {
                            true
                        }
                        k if k == MODULE_DECLARATION => {
                            self.is_namespace_instantiated(export_decl.export_clause)
                        }
                        // Named exports (`export { name }`) make a namespace instantiated.
                        // tsc resolves each specifier to check if it has a value meaning,
                        // but at the parser level we conservatively treat all named exports
                        // as potentially instantiating (matches tsc's practical behavior
                        // for import-alias re-export patterns).
                        k if k == NAMED_EXPORTS => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }

            // Nested namespace — recurse
            k if k == MODULE_DECLARATION => self.is_namespace_instantiated(node_idx),

            // Everything else (variables, functions, classes, enums, try/catch, if,
            // for, while, switch, expression statements, etc.) is runtime code
            _ => true,
        }
    }

    /// Get the modifier list for a declaration node, if it has one.
    ///
    /// Returns `Some(&NodeList)` for any declaration kind that carries modifiers
    /// (function, class, variable statement, enum, interface, type alias, module,
    /// method, property, constructor, accessor, parameter, import, export, etc.).
    /// Returns `None` for non-declaration nodes or nodes without modifier data.
    #[must_use]
    pub fn get_declaration_modifiers(&self, node: &Node) -> Option<&super::base::NodeList> {
        match node.kind {
            k if k == FUNCTION_DECLARATION || k == FUNCTION_EXPRESSION || k == ARROW_FUNCTION => {
                self.get_function(node).and_then(|d| d.modifiers.as_ref())
            }
            k if k == CLASS_DECLARATION || k == CLASS_EXPRESSION => {
                self.get_class(node).and_then(|d| d.modifiers.as_ref())
            }
            VARIABLE_STATEMENT => self.get_variable(node).and_then(|d| d.modifiers.as_ref()),
            ENUM_DECLARATION => self.get_enum(node).and_then(|d| d.modifiers.as_ref()),
            INTERFACE_DECLARATION => self.get_interface(node).and_then(|d| d.modifiers.as_ref()),
            TYPE_ALIAS_DECLARATION => self.get_type_alias(node).and_then(|d| d.modifiers.as_ref()),
            MODULE_DECLARATION => self.get_module(node).and_then(|d| d.modifiers.as_ref()),
            IMPORT_DECLARATION => self
                .get_import_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            EXPORT_DECLARATION => self
                .get_export_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            EXPORT_ASSIGNMENT => self
                .get_export_assignment(node)
                .and_then(|d| d.modifiers.as_ref()),
            k if k == METHOD_DECLARATION || k == METHOD_SIGNATURE => self
                .get_method_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            k if k == PROPERTY_DECLARATION || k == PROPERTY_SIGNATURE => self
                .get_property_decl(node)
                .and_then(|d| d.modifiers.as_ref()),
            CONSTRUCTOR => self
                .get_constructor(node)
                .and_then(|d| d.modifiers.as_ref()),
            k if k == GET_ACCESSOR || k == SET_ACCESSOR => {
                self.get_accessor(node).and_then(|d| d.modifiers.as_ref())
            }
            PARAMETER => self.get_parameter(node).and_then(|d| d.modifiers.as_ref()),
            INDEX_SIGNATURE => self
                .get_index_signature(node)
                .and_then(|d| d.modifiers.as_ref()),
            _ => None,
        }
    }

    /// Check whether a node is in an ambient context.
    ///
    /// A node is in an ambient context if it or any ancestor:
    /// - Has the `AMBIENT` node flag (set by parser for `.d.ts` files),
    /// - Has a `declare` keyword modifier, or
    /// - Is an interface or type alias declaration (implicitly ambient).
    ///
    /// This does **not** check the file extension (`.d.ts`); callers that need
    /// that check should do it separately since it requires filename context
    /// that `NodeArena` doesn't have.
    #[must_use]
    pub fn is_in_ambient_context(&self, idx: NodeIndex) -> bool {
        use super::flags::node_flags;

        let mut current = idx;
        for _ in 0..100 {
            let Some(node) = self.get(current) else {
                return false;
            };

            // Check the AMBIENT node flag (set by parser/binder)
            if (node.flags as u32) & node_flags::AMBIENT != 0 {
                return true;
            }

            // Interfaces and type aliases are implicitly ambient
            if node.kind == INTERFACE_DECLARATION || node.kind == TYPE_ALIAS_DECLARATION {
                return true;
            }

            // Check for `declare` keyword modifier on this node
            if let Some(mods) = self.get_declaration_modifiers(node) {
                for &mod_idx in &mods.nodes {
                    if let Some(mod_node) = self.get(mod_idx)
                        && mod_node.kind == tsz_scanner::SyntaxKind::DeclareKeyword as u16
                    {
                        return true;
                    }
                }
            }

            // Walk to parent
            if let Some(ext) = self.get_extended(current) {
                if ext.parent.is_none() {
                    return false;
                }
                current = ext.parent;
            } else {
                return false;
            }
        }
        false
    }

    /// Returns the combined `node_flags` for a `VARIABLE_DECLARATION` node,
    /// merging the node's own flags with its parent `VARIABLE_DECLARATION_LIST`
    /// flags. This is needed because the parser may place `LET`/`CONST`/`USING`
    /// flags on either the declaration or the list.
    ///
    /// Returns `0` if the node doesn't exist.
    #[must_use]
    pub fn get_variable_declaration_flags(&self, node_idx: NodeIndex) -> u32 {
        let Some(node) = self.get(node_idx) else {
            return 0;
        };
        let mut flags = node.flags as u32;
        use super::flags::node_flags;
        if !node_flags::is_block_scoped(flags)
            && let Some(ext) = self.get_extended(node_idx)
            && let Some(parent) = self.get(ext.parent)
            && parent.kind == VARIABLE_DECLARATION_LIST
        {
            flags |= parent.flags as u32;
        }
        flags
    }

    /// Returns `true` if a `VARIABLE_DECLARATION` node is declared with `const`.
    ///
    /// Handles the fact that the `CONST` flag may live on the node itself or
    /// on its parent `VARIABLE_DECLARATION_LIST`.
    #[must_use]
    pub fn is_const_variable_declaration(&self, node_idx: NodeIndex) -> bool {
        use super::flags::node_flags;
        (self.get_variable_declaration_flags(node_idx) & node_flags::CONST) != 0
    }

    /// Returns `true` if a `VARIABLE_DECLARATION` node is `const`-like, i.e.
    /// declared with `const`, `using`, or `await using`.
    ///
    /// Mirrors tsc's `isVarConstLike`: all three forms share immutable,
    /// must-initialize binding semantics, so grammar checks such as the ambient
    /// const-initializer restriction (TS1254) apply uniformly. The `CONST` bit
    /// alone is insufficient — plain `using` sets only the `USING` bit (4),
    /// while `await using` (6 = `CONST | USING`) already carries `CONST`.
    #[must_use]
    pub fn is_var_const_like_declaration(&self, node_idx: NodeIndex) -> bool {
        use super::flags::node_flags;
        (self.get_variable_declaration_flags(node_idx) & (node_flags::CONST | node_flags::USING))
            != 0
    }

    /// Get accessor data (get/set accessor).
    #[inline]
    #[must_use]
    pub fn get_accessor(&self, node: &Node) -> Option<&AccessorData> {
        if node.has_data() && node.is_accessor() {
            self.accessors.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Get JSX text data.
    #[inline]
    #[must_use]
    pub fn get_jsx_text(&self, node: &Node) -> Option<&JsxTextData> {
        use tsz_scanner::SyntaxKind;
        if node.has_data() && node.kind == SyntaxKind::JsxText as u16 {
            self.jsx_text.get(node.data_index as usize)
        } else {
            None
        }
    }

    /// Number of nodes in the arena
    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if arena is empty
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

// =============================================================================
// Index-based convenience accessors: get(index) + get_TYPE(node) in one call
// =============================================================================

/// Generate `get_*_at(index: NodeIndex) -> Option<&T>` convenience methods
/// that combine `arena.get(index)` with a typed getter in a single call.
macro_rules! define_at_accessors {
    ($($at_name:ident => $getter:ident -> $ret:ty);* $(;)?) => {
        impl NodeArena {
            $(
                #[inline]
#[must_use]
                pub fn $at_name(&self, index: NodeIndex) -> Option<&$ret> {
                    self.$getter(self.get(index)?)
                }
            )*
        }
    };
}

define_at_accessors! {
    get_identifier_at => get_identifier -> IdentifierData;
    get_literal_at => get_literal -> LiteralData;
    get_binary_expr_at => get_binary_expr -> BinaryExprData;
    get_call_expr_at => get_call_expr -> CallExprData;
    get_access_expr_at => get_access_expr -> AccessExprData;
    get_conditional_expr_at => get_conditional_expr -> ConditionalExprData;
    get_qualified_name_at => get_qualified_name -> QualifiedNameData;
    get_literal_expr_at => get_literal_expr -> LiteralExprData;
    get_property_assignment_at => get_property_assignment -> PropertyAssignmentData;
    get_type_assertion_at => get_type_assertion -> TypeAssertionData;
    get_unary_expr_at => get_unary_expr -> UnaryExprData;
    get_unary_expr_ex_at => get_unary_expr_ex -> UnaryExprDataEx;
    get_function_at => get_function -> FunctionData;
    get_class_at => get_class -> ClassData;
    get_block_at => get_block -> BlockData;
    get_source_file_at => get_source_file -> SourceFileData;
    get_variable_at => get_variable -> VariableData;
    get_variable_declaration_at => get_variable_declaration -> VariableDeclarationData;
    get_interface_at => get_interface -> InterfaceData;
    get_type_alias_at => get_type_alias -> TypeAliasData;
    get_enum_at => get_enum -> EnumData;
    get_enum_member_at => get_enum_member -> EnumMemberData;
    get_module_at => get_module -> ModuleData;
    get_module_block_at => get_module_block -> ModuleBlockData;
    get_if_statement_at => get_if_statement -> IfStatementData;
    get_loop_at => get_loop -> LoopData;
    get_for_in_of_at => get_for_in_of -> ForInOfData;
    get_switch_at => get_switch -> SwitchData;
    get_case_clause_at => get_case_clause -> CaseClauseData;
    get_try_at => get_try -> TryData;
    get_catch_clause_at => get_catch_clause -> CatchClauseData;
    get_labeled_statement_at => get_labeled_statement -> LabeledData;
    get_jump_data_at => get_jump_data -> JumpData;
    get_with_statement_at => get_with_statement -> IfStatementData;
    get_import_decl_at => get_import_decl -> ImportDeclData;
    get_import_clause_at => get_import_clause -> ImportClauseData;
    get_named_imports_at => get_named_imports -> NamedImportsData;
    get_specifier_at => get_specifier -> SpecifierData;
    get_export_decl_at => get_export_decl -> ExportDeclData;
    get_export_assignment_at => get_export_assignment -> ExportAssignmentData;
    get_import_attributes_data_at => get_import_attributes_data -> ImportAttributesData;
    get_import_attribute_data_at => get_import_attribute_data -> ImportAttributeData;
    get_parameter_at => get_parameter -> ParameterData;
    get_property_decl_at => get_property_decl -> PropertyDeclData;
    get_method_decl_at => get_method_decl -> MethodDeclData;
    get_constructor_at => get_constructor -> ConstructorData;
    get_accessor_at => get_accessor -> AccessorData;
    get_decorator_at => get_decorator -> DecoratorData;
    get_type_ref_at => get_type_ref -> TypeRefData;
    get_expression_statement_at => get_expression_statement -> ExprStatementData;
    get_return_statement_at => get_return_statement -> ReturnData;
    get_jsx_element_at => get_jsx_element -> JsxElementData;
    get_jsx_opening_at => get_jsx_opening -> JsxOpeningData;
    get_jsx_closing_at => get_jsx_closing -> JsxClosingData;
    get_jsx_fragment_at => get_jsx_fragment -> JsxFragmentData;
    get_jsx_attributes_at => get_jsx_attributes -> JsxAttributesData;
    get_jsx_attribute_at => get_jsx_attribute -> JsxAttributeData;
    get_jsx_spread_attribute_at => get_jsx_spread_attribute -> JsxSpreadAttributeData;
    get_jsx_expression_at => get_jsx_expression -> JsxExpressionData;
    get_jsx_text_at => get_jsx_text -> JsxTextData;
    get_jsx_namespaced_name_at => get_jsx_namespaced_name -> JsxNamespacedNameData;
    get_signature_at => get_signature -> SignatureData;
    get_index_signature_at => get_index_signature -> IndexSignatureData;
    get_heritage_clause_at => get_heritage_clause -> HeritageData;
    get_composite_type_at => get_composite_type -> CompositeTypeData;
    get_array_type_at => get_array_type -> ArrayTypeData;
    get_tuple_type_at => get_tuple_type -> TupleTypeData;
    get_function_type_at => get_function_type -> FunctionTypeData;
    get_type_literal_at => get_type_literal -> TypeLiteralData;
    get_conditional_type_at => get_conditional_type -> ConditionalTypeData;
    get_mapped_type_at => get_mapped_type -> MappedTypeData;
    get_indexed_access_type_at => get_indexed_access_type -> IndexedAccessTypeData;
    get_literal_type_at => get_literal_type -> LiteralTypeData;
    get_wrapped_type_at => get_wrapped_type -> WrappedTypeData;
    get_expr_type_args_at => get_expr_type_args -> ExprWithTypeArgsData;
    get_type_query_at => get_type_query -> TypeQueryData;
    get_type_operator_at => get_type_operator -> TypeOperatorData;
    get_infer_type_at => get_infer_type -> InferTypeData;
    get_template_literal_type_at => get_template_literal_type -> TemplateLiteralTypeData;
    get_named_tuple_member_at => get_named_tuple_member -> NamedTupleMemberData;
    get_type_predicate_at => get_type_predicate -> TypePredicateData;
    get_type_parameter_at => get_type_parameter -> TypeParameterData;
    get_parenthesized_at => get_parenthesized -> ParenthesizedData;
    get_template_expr_at => get_template_expr -> TemplateExprData;
    get_template_span_at => get_template_span -> TemplateSpanData;
    get_tagged_template_at => get_tagged_template -> TaggedTemplateData;
    get_spread_at => get_spread -> SpreadData;
    get_shorthand_property_at => get_shorthand_property -> ShorthandPropertyData;
    get_binding_pattern_at => get_binding_pattern -> BindingPatternData;
    get_binding_element_at => get_binding_element -> BindingElementData;
    get_computed_property_at => get_computed_property -> ComputedPropertyData
}

// NodeView, NodeInfo, and NodeAccess are in node_view.rs

// =============================================================================
// Node Kind Utilities
// =============================================================================

impl Node {
    /// Check if this is an identifier node
    #[inline]
    #[must_use]
    pub const fn is_identifier(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::Identifier as u16
    }

    /// Check if this is a string literal
    #[inline]
    #[must_use]
    pub const fn is_string_literal(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::StringLiteral as u16
    }

    /// Check if this is a numeric literal
    #[inline]
    #[must_use]
    pub const fn is_numeric_literal(&self) -> bool {
        use tsz_scanner::SyntaxKind;
        self.kind == SyntaxKind::NumericLiteral as u16
    }

    /// Check if this is a function declaration
    #[inline]
    #[must_use]
    pub const fn is_function_declaration(&self) -> bool {
        use super::syntax_kind_ext::FUNCTION_DECLARATION;
        self.kind == FUNCTION_DECLARATION
    }

    /// Check if this is a class declaration
    #[inline]
    #[must_use]
    pub const fn is_class_declaration(&self) -> bool {
        use super::syntax_kind_ext::CLASS_DECLARATION;
        self.kind == CLASS_DECLARATION
    }

    /// Check if this is any class-like node (class declaration or class expression).
    #[inline]
    #[must_use]
    pub const fn is_class_like(&self) -> bool {
        use super::syntax_kind_ext::{CLASS_DECLARATION, CLASS_EXPRESSION};
        matches!(self.kind, CLASS_DECLARATION | CLASS_EXPRESSION)
    }

    /// Check if this is any kind of function-like node
    #[inline]
    #[must_use]
    pub const fn is_function_like(&self) -> bool {
        matches!(
            self.kind,
            FUNCTION_DECLARATION
                | FUNCTION_EXPRESSION
                | ARROW_FUNCTION
                | METHOD_DECLARATION
                | CONSTRUCTOR
                | GET_ACCESSOR
                | SET_ACCESSOR
        )
    }

    /// Check if this is an anonymous function-valued expression
    /// (`function () {}`, `function name() {}`, or `(a) => {}`).
    #[inline]
    #[must_use]
    pub const fn is_function_expression_or_arrow(&self) -> bool {
        matches!(self.kind, FUNCTION_EXPRESSION | ARROW_FUNCTION)
    }

    /// Check if this is a get or set accessor declaration.
    #[inline]
    #[must_use]
    pub const fn is_accessor(&self) -> bool {
        matches!(self.kind, GET_ACCESSOR | SET_ACCESSOR)
    }

    /// Check if this is a non-arrow function-like node (creates its own `this` binding).
    ///
    /// Arrow functions capture `this` from their enclosing scope and are excluded.
    /// Class bodies also create a `this` scope but are not included here — use
    /// [`is_class_like`] for that boundary check.
    #[inline]
    #[must_use]
    pub const fn is_non_arrow_function_like(&self) -> bool {
        matches!(
            self.kind,
            FUNCTION_DECLARATION
                | FUNCTION_EXPRESSION
                | METHOD_DECLARATION
                | CONSTRUCTOR
                | GET_ACCESSOR
                | SET_ACCESSOR
        )
    }

    /// Check if this is a binding pattern (array or object destructuring)
    #[inline]
    #[must_use]
    pub const fn is_binding_pattern(&self) -> bool {
        self.kind == OBJECT_BINDING_PATTERN || self.kind == ARRAY_BINDING_PATTERN
    }

    /// Check if this is a statement
    #[inline]
    #[must_use]
    pub fn is_statement(&self) -> bool {
        (BLOCK..=DEBUGGER_STATEMENT).contains(&self.kind) || self.kind == VARIABLE_STATEMENT
    }

    /// Check if this is a declaration
    #[inline]
    #[must_use]
    pub fn is_declaration(&self) -> bool {
        (VARIABLE_DECLARATION..=EXPORT_SPECIFIER).contains(&self.kind)
    }

    /// Check if this is a type node
    #[inline]
    #[must_use]
    pub fn is_type_node(&self) -> bool {
        (TYPE_PREDICATE..=IMPORT_TYPE).contains(&self.kind)
    }
}

// Child collection methods are in node_children.rs
// (collect_name_children, collect_expression_children, collect_statement_children,
//  collect_declaration_children, collect_import_export_children, collect_type_children,
//  collect_member_children, collect_pattern_children, collect_jsx_children,
//  collect_signature_children, collect_source_children, and helper functions
//  add_opt_child, add_list, add_opt_list)

// NodeAccess trait and NodeInfo are in node_view.rs

#[cfg(test)]
mod is_missing_recovery_identifier_tests {
    use super::*;
    use crate::parser::node::NodeArena;
    use tsz_common::interner::{AstAtom, IdentText};
    use tsz_scanner::SyntaxKind;

    #[test]
    fn returns_true_for_synthesized_recovery_placeholder() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            0,
            IdentifierData {
                atom: AstAtom::NONE,
                escaped_text: IdentText::empty(),
                original_text: None,
            },
        );
        assert!(arena.is_missing_recovery_identifier(idx));
    }

    #[test]
    fn returns_false_for_real_named_identifier() {
        let mut arena = NodeArena::with_capacity(8);
        // A real identifier has a non-NONE atom AND non-empty escaped_text;
        // either condition alone is enough for the helper to reject it.
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            3,
            IdentifierData {
                atom: AstAtom(1),
                escaped_text: IdentText::from("foo"),
                original_text: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }

    #[test]
    fn returns_false_when_only_atom_is_set() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            0,
            IdentifierData {
                atom: AstAtom(1),
                escaped_text: IdentText::empty(),
                original_text: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }

    #[test]
    fn returns_false_when_only_escaped_text_is_set() {
        let mut arena = NodeArena::with_capacity(8);
        let idx = arena.add_identifier(
            SyntaxKind::Identifier as u16,
            0,
            3,
            IdentifierData {
                atom: AstAtom::NONE,
                escaped_text: IdentText::from("x"),
                original_text: None,
            },
        );
        assert!(!arena.is_missing_recovery_identifier(idx));
    }

    #[test]
    fn returns_false_for_non_identifier_node() {
        let arena = NodeArena::with_capacity(8);
        // Default-init NodeIndex points at nothing — get() returns None.
        assert!(!arena.is_missing_recovery_identifier(NodeIndex::NONE));
    }
}

// Single-kind / small-fixed-kind typed getters, table-driven.
define_kind_getters! {
    /// Get binary expression data.
    /// Returns None if node is not a binary expression or has no data.
    get_binary_expr => binary_exprs -> BinaryExprData, [BINARY_EXPRESSION];

    /// Get call expression data.
    /// Returns None if node is not a call/new expression or has no data.
    get_call_expr => call_exprs -> CallExprData, [CALL_EXPRESSION, NEW_EXPRESSION];

    /// Get access expression data (property access or element access).
    /// Returns None if node is not an access expression or has no data.
    get_access_expr => access_exprs -> AccessExprData, [PROPERTY_ACCESS_EXPRESSION, ELEMENT_ACCESS_EXPRESSION, META_PROPERTY];

    /// Get conditional expression data (ternary: a ? b : c).
    /// Returns None if node is not a conditional expression or has no data.
    get_conditional_expr => conditional_exprs -> ConditionalExprData, [CONDITIONAL_EXPRESSION];

    /// Get qualified name data (A.B syntax).
    /// Returns None if node is not a qualified name or has no data.
    get_qualified_name => qualified_names -> QualifiedNameData, [QUALIFIED_NAME];

    /// Get literal expression data (array or object literal).
    /// Returns None if node is not a literal expression or has no data.
    get_literal_expr => literal_exprs -> LiteralExprData, [ARRAY_LITERAL_EXPRESSION, OBJECT_LITERAL_EXPRESSION];

    /// Get property assignment data.
    /// Returns None if node is not a property assignment or has no data.
    get_property_assignment => property_assignments -> PropertyAssignmentData, [PROPERTY_ASSIGNMENT];

    /// Get type assertion data (as/satisfies/type assertion).
    /// Returns None if node is not a type assertion or has no data.
    get_type_assertion => type_assertions -> TypeAssertionData, [TYPE_ASSERTION, AS_EXPRESSION, SATISFIES_EXPRESSION];

    /// Get unary expression data (prefix or postfix).
    /// Returns None if node is not a unary expression or has no data.
    get_unary_expr => unary_exprs -> UnaryExprData, [PREFIX_UNARY_EXPRESSION, POSTFIX_UNARY_EXPRESSION];

    /// Get extended unary expression data (await/yield/non-null/spread).
    /// Returns None if node is not an await/yield/non-null/spread expression or has no data.
    get_unary_expr_ex => unary_exprs_ex -> UnaryExprDataEx, [AWAIT_EXPRESSION, YIELD_EXPRESSION, NON_NULL_EXPRESSION, SPREAD_ELEMENT];

    /// Get function data.
    /// Returns None if node is not a function-like node or has no data.
    get_function => functions -> FunctionData, [FUNCTION_DECLARATION, FUNCTION_EXPRESSION, ARROW_FUNCTION];

    /// Get class data.
    /// Returns None if node is not a class declaration/expression or has no data.
    get_class => classes -> ClassData, [CLASS_DECLARATION, CLASS_EXPRESSION];

    /// Get block data.
    /// Returns None if node is not a block or has no data.
    get_block => blocks -> BlockData, [BLOCK, CLASS_STATIC_BLOCK_DECLARATION, CASE_BLOCK];

    /// Get source file data.
    /// Returns None if node is not a source file or has no data.
    get_source_file => source_files -> SourceFileData, [SOURCE_FILE];

    /// Get variable data (`VariableStatement` or `VariableDeclarationList`).
    get_variable => variables -> VariableData, [VARIABLE_STATEMENT, VARIABLE_DECLARATION_LIST];

    /// Get variable declaration data.
    get_variable_declaration => variable_declarations -> VariableDeclarationData, [VARIABLE_DECLARATION];

    /// Get interface data.
    get_interface => interfaces -> InterfaceData, [INTERFACE_DECLARATION];

    /// Get type alias data.
    get_type_alias => type_aliases -> TypeAliasData, [TYPE_ALIAS_DECLARATION];

    /// Get enum data.
    get_enum => enums -> EnumData, [ENUM_DECLARATION];

    /// Get enum member data.
    get_enum_member => enum_members -> EnumMemberData, [ENUM_MEMBER];

    /// Get module data.
    get_module => modules -> ModuleData, [MODULE_DECLARATION];

    /// Get module block data.
    get_module_block => module_blocks -> ModuleBlockData, [MODULE_BLOCK];

    /// Get if statement data.
    get_if_statement => if_statements -> IfStatementData, [IF_STATEMENT];

    /// Get loop data (while, for, do-while).
    get_loop => loops -> LoopData, [WHILE_STATEMENT, DO_STATEMENT, FOR_STATEMENT];

    /// Get for-in/for-of data.
    get_for_in_of => for_in_of -> ForInOfData, [FOR_IN_STATEMENT, FOR_OF_STATEMENT];

    /// Get switch data.
    get_switch => switch_data -> SwitchData, [SWITCH_STATEMENT];

    /// Get case clause data.
    get_case_clause => case_clauses -> CaseClauseData, [CASE_CLAUSE, DEFAULT_CLAUSE];

    /// Get try data.
    get_try => try_data -> TryData, [TRY_STATEMENT];

    /// Get catch clause data.
    get_catch_clause => catch_clauses -> CatchClauseData, [CATCH_CLAUSE];

    /// Get labeled statement data.
    get_labeled_statement => labeled_data -> LabeledData, [LABELED_STATEMENT];

    /// Get jump data (break/continue statements).
    get_jump_data => jump_data -> JumpData, [BREAK_STATEMENT, CONTINUE_STATEMENT];

    /// Get with statement data (stored in if statement pool).
    get_with_statement => if_statements -> IfStatementData, [WITH_STATEMENT];

    /// Get import declaration data (handles both `IMPORT_DECLARATION` and `IMPORT_EQUALS_DECLARATION`).
    get_import_decl => import_decls -> ImportDeclData, [IMPORT_DECLARATION, IMPORT_EQUALS_DECLARATION];

    /// Get import clause data.
    get_import_clause => import_clauses -> ImportClauseData, [IMPORT_CLAUSE];

    /// Get named imports/exports data.
    /// Works for `NAMED_IMPORTS`, `NAMESPACE_IMPORT`, and `NAMED_EXPORTS` (they share the same data structure).
    get_named_imports => named_imports -> NamedImportsData, [NAMED_IMPORTS, NAMED_EXPORTS, NAMESPACE_IMPORT];

    /// Get import/export specifier data.
    get_specifier => specifiers -> SpecifierData, [IMPORT_SPECIFIER, EXPORT_SPECIFIER];

    /// Get export declaration data.
    get_export_decl => export_decls -> ExportDeclData, [EXPORT_DECLARATION, NAMESPACE_EXPORT_DECLARATION];

    /// Get export assignment data (export = expr).
    get_export_assignment => export_assignments -> ExportAssignmentData, [EXPORT_ASSIGNMENT];

    /// Get import attributes data (`with { ... }` or `assert { ... }`).
    get_import_attributes_data => import_attributes -> ImportAttributesData, [IMPORT_ATTRIBUTES];

    /// Get single import attribute data (name: value pair).
    get_import_attribute_data => import_attribute -> ImportAttributeData, [IMPORT_ATTRIBUTE];

    /// Get parameter data.
    get_parameter => parameters -> ParameterData, [PARAMETER];

    /// Get property declaration data.
    get_property_decl => property_decls -> PropertyDeclData, [PROPERTY_DECLARATION];

    /// Get method declaration data.
    get_method_decl => method_decls -> MethodDeclData, [METHOD_DECLARATION];

    /// Get constructor data.
    get_constructor => constructors -> ConstructorData, [CONSTRUCTOR];

    /// Get decorator data.
    get_decorator => decorators -> DecoratorData, [DECORATOR];

    /// Get type reference data.
    get_type_ref => type_refs -> TypeRefData, [TYPE_REFERENCE];

    /// Get expression statement data (returns the expression node index).
    get_expression_statement => expr_statements -> ExprStatementData, [EXPRESSION_STATEMENT];

    /// Get return statement data (returns the expression node index).
    get_return_statement => return_data -> ReturnData, [RETURN_STATEMENT, THROW_STATEMENT];

    /// Get JSX element data.
    get_jsx_element => jsx_elements -> JsxElementData, [JSX_ELEMENT];

    /// Get JSX opening/self-closing element data.
    get_jsx_opening => jsx_opening -> JsxOpeningData, [JSX_OPENING_ELEMENT, JSX_SELF_CLOSING_ELEMENT];

    /// Get JSX closing element data.
    get_jsx_closing => jsx_closing -> JsxClosingData, [JSX_CLOSING_ELEMENT];

    /// Get JSX fragment data.
    get_jsx_fragment => jsx_fragments -> JsxFragmentData, [JSX_FRAGMENT];

    /// Get JSX attributes data.
    get_jsx_attributes => jsx_attributes -> JsxAttributesData, [JSX_ATTRIBUTES];

    /// Get JSX attribute data.
    get_jsx_attribute => jsx_attribute -> JsxAttributeData, [JSX_ATTRIBUTE];

    /// Get JSX spread attribute data.
    get_jsx_spread_attribute => jsx_spread_attributes -> JsxSpreadAttributeData, [JSX_SPREAD_ATTRIBUTE];

    /// Get JSX expression data.
    get_jsx_expression => jsx_expressions -> JsxExpressionData, [JSX_EXPRESSION];

    /// Get JSX namespaced name data.
    get_jsx_namespaced_name => jsx_namespaced_names -> JsxNamespacedNameData, [JSX_NAMESPACED_NAME];
}
