use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;

/// Decide whether the hoisted `var` for an ES5-downleveled block-scoped
/// declaration (enum or namespace/module) must be initialized to `void 0`.
///
/// Rule (matches `tsc`): when a non-ambient, non-const enum or an instantiated
/// namespace is downleveled to the `var Name; (function (Name) { ... })(Name ||
/// (Name = {}));` pattern, the hoisted `var` is emitted as `var Name = void 0;`
/// *iff* the declaration lives in a lexical block scope nested below the nearest
/// function / module / source-file boundary. A control-flow block
/// (`if`/`for`/`while`/`try`/bare `{}`), or a switch `case`/`default` clause, is
/// such a nested block scope; a function-like body, a namespace body
/// (`MODULE_BLOCK`), and the top-level source file are not.
///
/// The `void 0` initializer exists because the binding is logically
/// block-scoped: hoisting the `var` to the enclosing function scope would
/// otherwise let a stale value leak across re-entry to the block, so `tsc`
/// resets it to `undefined` at the point the block-scoped declaration would have
/// been introduced. (At ES2015+ targets `tsc` emits a properly block-scoped
/// `let` with no reset, so callers only apply this when the keyword stays `var`.)
///
/// This is keyed purely on AST node kinds along the parent chain — it never
/// inspects identifier text, file names, or printer output.
pub(crate) fn block_scoped_hoist_needs_void_zero(arena: &NodeArena, decl_idx: NodeIndex) -> bool {
    let mut current = decl_idx;
    // Bounded walk up the parent chain to the first statement-container.
    for _ in 0..64 {
        let Some(ext) = arena.get_extended(current) else {
            return false;
        };
        let parent = ext.parent;
        if parent.is_none() {
            return false;
        }
        let Some(parent_node) = arena.get(parent) else {
            return false;
        };
        match parent_node.kind {
            // Top-level of the file and namespace bodies are scope tops: the
            // hoisted var is the canonical declaration, no reset needed.
            syntax_kind_ext::SOURCE_FILE | syntax_kind_ext::MODULE_BLOCK => return false,
            // Switch `case`/`default` clauses introduce a block scope without a
            // wrapping `Block` node; declarations there need the reset.
            syntax_kind_ext::CASE_CLAUSE | syntax_kind_ext::DEFAULT_CLAUSE => return true,
            // A `Block` is a function-like body when its parent is a
            // function/method/accessor/constructor — that is a scope top, no
            // reset. Any other `Block` (control-flow or bare) is a nested block
            // scope and needs the reset.
            syntax_kind_ext::BLOCK => return !block_is_function_like_body(arena, parent),
            // Not a statement container yet (e.g. a wrapping list); keep walking
            // up toward the enclosing scope.
            _ => current = parent,
        }
    }
    false
}

/// Whether the given `Block` node is the body of a function-like declaration
/// (function/method/accessor/constructor/arrow), as opposed to a control-flow
/// or standalone block.
fn block_is_function_like_body(arena: &NodeArena, block_idx: NodeIndex) -> bool {
    let Some(ext) = arena.get_extended(block_idx) else {
        return false;
    };
    let block_parent = ext.parent;
    if block_parent.is_none() {
        return false;
    }
    let Some(parent_node) = arena.get(block_parent) else {
        return false;
    };
    matches!(
        parent_node.kind,
        syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::FUNCTION_EXPRESSION
            | syntax_kind_ext::ARROW_FUNCTION
            | syntax_kind_ext::METHOD_DECLARATION
            | syntax_kind_ext::CONSTRUCTOR
            | syntax_kind_ext::GET_ACCESSOR
            | syntax_kind_ext::SET_ACCESSOR
    )
}
