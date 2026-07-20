//! Transform utilities for syntax analysis.
//!
//! Common functions used by ES5 transformations.

use crate::parser::{NodeArena, NodeIndex, node::NodeAccess, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReferenceTarget {
    Arguments,
    NewTarget,
    This,
    Super,
}

impl ReferenceTarget {
    const fn identifier_name(self) -> Option<&'static str> {
        match self {
            Self::Arguments => Some("arguments"),
            Self::NewTarget => None,
            Self::This => Some("this"),
            Self::Super => Some("super"),
        }
    }

    const fn include_keyword_check(self) -> bool {
        matches!(self, Self::This | Self::Super)
    }
}

/// Check if an AST node contains a reference to `this` or `super`.
#[must_use]
pub fn contains_this_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    contains_target_reference(arena, node_idx, ReferenceTarget::This)
}

/// Check if an AST node contains a literal `this` keyword reference, excluding
/// `super`.
///
/// [`contains_this_reference`] intentionally also matches `super` because an
/// ES5-lowered arrow that calls `super.m(...)` threads the captured `_this`
/// receiver. Deciding whether a *static* member initializer needs the
/// class-value alias (`var _a; _a = Class;`) is a different question: a bare
/// static `super.x` access lowers to `_super.x` and never references the class
/// value, so it must not force the alias. This shares the same lexical
/// boundaries as [`contains_this_reference`] — transparent through arrows and
/// computed member names (so `this` inside a nested class expression's computed
/// name is still found), opaque through ordinary functions and non-computed
/// class member bodies — but only the `this` keyword counts.
#[must_use]
pub fn contains_this_keyword_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };
    if node.kind == SyntaxKind::ThisKeyword as u16 {
        return true;
    }
    if node.is_identifier()
        && arena
            .get_identifier(node)
            .is_some_and(|identifier| identifier.escaped_text == "this")
    {
        return true;
    }
    target_reference_children(arena, node_idx, ReferenceTarget::This)
        .into_iter()
        .any(|child_idx| contains_this_keyword_reference(arena, child_idx))
}

/// Check whether an ES5-lowered arrow body must capture lexical `this`.
///
/// An arrow captures `this` when it spells `this`, OR when it contains a
/// `super.m(...)` / `super[e](...)` **call**: at ES5 such a call lowers to
/// `_super.prototype.m.call(_this, ...)`, threading the captured receiver.
/// A bare `super.x` / `super[e]` property **access** lowers to
/// `_super.prototype.x` and references no `this`, so it does not by itself
/// force a `var _this = this;` capture. The lexical-boundary rules match
/// [`contains_this_reference`]: nested non-arrow functions stop propagation
/// while arrows and computed member names stay in scope.
#[must_use]
pub fn arrow_captures_lexical_this(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };

    // A literal `this` keyword (or `this` identifier from recovery) captures.
    if node.kind == SyntaxKind::ThisKeyword as u16 {
        return true;
    }
    if node.is_identifier()
        && arena
            .get_identifier(node)
            .is_some_and(|identifier| identifier.escaped_text == "this")
    {
        return true;
    }

    // A `super(...)`-shaped call where the callee resolves to `super` threads
    // the captured receiver into the lowered `.call(_this, ...)` form.
    if (node.kind == syntax_kind_ext::CALL_EXPRESSION)
        && let Some(call) = arena.get_call_expr(node)
        && call_expression_callee_is_super(arena, call.expression)
    {
        return true;
    }

    target_reference_children(arena, node_idx, ReferenceTarget::This)
        .into_iter()
        .any(|child_idx| arrow_captures_lexical_this(arena, child_idx))
}

/// Whether a call-expression callee is a direct `super` member call:
/// `super.m`, `super[e]`, `super(...)`, or those wrapped in parentheses.
///
/// Only a member access whose immediate base is `super` counts. A chained
/// access such as `super.a.b()` is a normal call on `super.a` (which lowers
/// to `_super.prototype.a`), not a super call, so it does not capture `this`.
fn call_expression_callee_is_super(arena: &NodeArena, callee_idx: NodeIndex) -> bool {
    let callee_idx = unwrap_parentheses(arena, callee_idx);
    let Some(node) = arena.get(callee_idx) else {
        return false;
    };
    if node.kind == SyntaxKind::SuperKeyword as u16 {
        return true;
    }
    if (node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        || node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
        && let Some(access) = arena.get_access_expr(node)
    {
        let base_idx = unwrap_parentheses(arena, access.expression);
        return arena
            .get(base_idx)
            .is_some_and(|base| base.kind == SyntaxKind::SuperKeyword as u16);
    }
    false
}

/// Unwrap nested parenthesized expressions to the inner expression.
fn unwrap_parentheses(arena: &NodeArena, mut idx: NodeIndex) -> NodeIndex {
    while let Some(node) = arena.get(idx) {
        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(paren) = arena.get_parenthesized(node)
        {
            idx = paren.expression;
            continue;
        }
        break;
    }
    idx
}

/// Collect `this` references that appear in computed member names of a class.
///
/// This follows the same scope rules as `contains_this_reference`: nested non-arrow
/// functions stop lexical `this` propagation, while computed member names remain
/// part of class-evaluation semantics.
#[must_use]
pub fn collect_class_computed_name_this_references(
    arena: &NodeArena,
    class_idx: NodeIndex,
) -> Vec<NodeIndex> {
    let Some(class_node) = arena.get(class_idx) else {
        return Vec::new();
    };
    let Some(class_data) = arena.get_class(class_node) else {
        return Vec::new();
    };

    let mut refs = Vec::new();
    for &member_idx in &class_data.members.nodes {
        let Some(member) = arena.get(member_idx) else {
            continue;
        };

        let name_idx = match member.kind {
            kind if kind == syntax_kind_ext::PROPERTY_DECLARATION => {
                arena.get_property_decl(member).map(|prop| prop.name)
            }
            kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
                arena.get_method_decl(member).map(|method| method.name)
            }
            kind if kind == syntax_kind_ext::GET_ACCESSOR
                || kind == syntax_kind_ext::SET_ACCESSOR =>
            {
                arena.get_accessor(member).map(|accessor| accessor.name)
            }
            _ => None,
        };

        if let Some(name_idx) = name_idx
            && arena
                .get(name_idx)
                .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        {
            collect_target_references(arena, name_idx, ReferenceTarget::This, &mut refs);
        }
    }

    refs
}

/// Check if an AST node contains a reference to `super`.
#[must_use]
pub fn contains_super_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    contains_target_reference(arena, node_idx, ReferenceTarget::Super)
}

/// Check if a node contains a reference to `arguments`.
///
/// This is used to determine if an arrow function needs to capture the parent's
/// `arguments` object for ES5 downleveling.
///
/// Important: Regular functions have their own `arguments`, so we don't recurse
/// into them. Only arrow functions inherit the parent's `arguments`.
#[must_use]
pub fn contains_arguments_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    contains_target_reference(arena, node_idx, ReferenceTarget::Arguments)
}

/// Check if a node contains `new.target` in the current lexical context.
///
/// Regular functions and classes own a new `new.target` binding. Arrow functions
/// inherit it from the surrounding function-like body.
#[must_use]
pub fn contains_new_target_reference(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    contains_target_reference(arena, node_idx, ReferenceTarget::NewTarget)
}

/// Check if a node contains an async arrow function in the current lexical
/// context. Nested regular functions and classes form new lexical `this`
/// boundaries, while nested arrow functions remain in the same context.
#[must_use]
pub fn contains_async_arrow_function(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };

    match node.kind {
        kind if kind == syntax_kind_ext::ARROW_FUNCTION => {
            let Some(func) = arena.get_function(node) else {
                return false;
            };
            if func.is_async {
                return true;
            }
            let mut children = Vec::new();
            for &param_idx in &func.parameters.nodes {
                let Some(param_node) = arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = arena.get_parameter(param_node) else {
                    continue;
                };
                if param.initializer.is_some() {
                    children.push(param.initializer);
                }
            }
            if func.body.is_some() {
                children.push(func.body);
            }
            children
                .into_iter()
                .any(|child_idx| contains_async_arrow_function(arena, child_idx))
        }
        kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::CLASS_EXPRESSION =>
        {
            false
        }
        _ => arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| contains_async_arrow_function(arena, child_idx)),
    }
}

/// Check whether an expression contains an arrow in the current lexical
/// `this` scope.
///
/// Expression wrappers (arrays, objects, calls, conditionals, comma
/// expressions, parentheses, and assertions) are transparent. A nested
/// class's decorators/heritage, class-element and parameter decorators, and
/// computed member names are also evaluated in the enclosing lexical `this`
/// scope. Ordinary function bodies, member/constructor bodies, and nested class
/// field initializers are opaque because an arrow below one of those nodes
/// captures a different `this`. The arrow body itself need not be traversed:
/// finding the arrow is the complete answer.
#[must_use]
pub fn contains_lexical_arrow_function(arena: &NodeArena, node_idx: NodeIndex) -> bool {
    let mut traversal = Vec::new();
    contains_lexical_arrow_function_with_scratch(arena, node_idx, &mut traversal)
}

/// Allocation-reusing form of [`contains_lexical_arrow_function`].
///
/// `traversal` is cleared on entry and can be retained across several class
/// initializers, keeping the scan to at most one buffer allocation per class.
#[must_use]
pub fn contains_lexical_arrow_function_with_scratch(
    arena: &NodeArena,
    node_idx: NodeIndex,
    traversal: &mut Vec<NodeIndex>,
) -> bool {
    traversal.clear();
    traversal.push(node_idx);

    while let Some(current) = traversal.pop() {
        let Some(node) = arena.get(current) else {
            continue;
        };
        if node.kind == syntax_kind_ext::ARROW_FUNCTION {
            traversal.clear();
            return true;
        }
        append_lexical_arrow_children(arena, current, traversal);
    }

    false
}

/// Whether `child_idx` is evaluated in its parent's enclosing lexical `this`
/// scope according to the same boundary routing used by transform scans.
///
/// This direct-edge predicate lets checker parent walks cross nested class
/// heritage/decorators and computed-name wrappers without opening method
/// bodies or class field initializers.
#[must_use]
pub fn child_is_in_enclosing_lexical_this_scope(
    arena: &NodeArena,
    parent_idx: NodeIndex,
    child_idx: NodeIndex,
) -> bool {
    let Some(parent) = arena.get(parent_idx) else {
        return false;
    };
    match parent.kind {
        kind if kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::CLASS_EXPRESSION =>
        {
            arena.get_class(parent).is_some_and(|class_data| {
                decorator_is_child(arena, class_data.modifiers.as_ref(), child_idx)
                    || class_data
                        .heritage_clauses
                        .as_ref()
                        .is_some_and(|clauses| clauses.nodes.contains(&child_idx))
                    || (class_data.members.nodes.contains(&child_idx)
                        && class_element_has_enclosing_lexical_header(arena, child_idx))
            })
        }
        kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
            arena.get_method_decl(parent).is_some_and(|method| {
                decorator_is_child(arena, method.modifiers.as_ref(), child_idx)
                    || computed_name_is_child(arena, method.name, child_idx)
                    || parameter_with_decorators_is_child(arena, &method.parameters, child_idx)
            })
        }
        kind if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR => {
            arena.get_accessor(parent).is_some_and(|accessor| {
                decorator_is_child(arena, accessor.modifiers.as_ref(), child_idx)
                    || computed_name_is_child(arena, accessor.name, child_idx)
                    || parameter_with_decorators_is_child(arena, &accessor.parameters, child_idx)
            })
        }
        kind if kind == syntax_kind_ext::PROPERTY_DECLARATION => {
            arena.get_property_decl(parent).is_some_and(|property| {
                decorator_is_child(arena, property.modifiers.as_ref(), child_idx)
                    || computed_name_is_child(arena, property.name, child_idx)
            })
        }
        kind if kind == syntax_kind_ext::CONSTRUCTOR => {
            arena.get_constructor(parent).is_some_and(|constructor| {
                decorator_is_child(arena, constructor.modifiers.as_ref(), child_idx)
                    || parameter_with_decorators_is_child(arena, &constructor.parameters, child_idx)
            })
        }
        kind if kind == syntax_kind_ext::PARAMETER => {
            arena.get_parameter(parent).is_some_and(|parameter| {
                decorator_is_child(arena, parameter.modifiers.as_ref(), child_idx)
            })
        }
        kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION =>
        {
            false
        }
        _ => lexical_arrow_children(arena, parent_idx).contains(&child_idx),
    }
}

/// Find the nearest class that owns lexical `this` for `node_idx`.
///
/// Class decorators, heritage expressions, computed member names, and
/// class-element/parameter decorator expressions execute outside the class's
/// own `this` scope, so this walk crosses those exact edges. Member bodies,
/// field initializers, parameter initializers, and static blocks stop at their
/// containing class. Ordinary functions and object-literal methods own `this`
/// without a class and therefore terminate the search.
#[must_use]
pub fn nearest_enclosing_lexical_this_class(
    arena: &NodeArena,
    node_idx: NodeIndex,
) -> Option<NodeIndex> {
    let mut current = node_idx;
    let mut child = NodeIndex::NONE;
    let mut class_member_scope_owns_this = false;
    let mut iterations = 0;

    while current.is_some() {
        iterations += 1;
        if iterations > 1024 {
            return None;
        }
        let node = arena.get(current)?;
        match node.kind {
            kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
                || kind == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                return None;
            }
            kind if kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if class_member_scope_owns_this {
                    return None;
                }
            }
            kind if kind == syntax_kind_ext::CLASS_DECLARATION
                || kind == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                if class_member_scope_owns_this
                    || !child.is_some()
                    || !child_is_in_enclosing_lexical_this_scope(arena, current, child)
                {
                    return Some(current);
                }
            }
            kind if (kind == syntax_kind_ext::PROPERTY_DECLARATION
                || kind == syntax_kind_ext::METHOD_DECLARATION
                || kind == syntax_kind_ext::GET_ACCESSOR
                || kind == syntax_kind_ext::SET_ACCESSOR
                || kind == syntax_kind_ext::CONSTRUCTOR
                || kind == syntax_kind_ext::PARAMETER)
                && child.is_some()
                && !child_is_in_enclosing_lexical_this_scope(arena, current, child) =>
            {
                class_member_scope_owns_this = true;
            }
            _ => {}
        }

        child = current;
        current = arena.get_extended(current)?.parent;
    }
    None
}

/// Children that remain in the current lexical-`this` scope while searching
/// for arrows. Kept separate from `target_reference_children`: emitter target
/// scans may start at a property/constructor node and must inspect its runtime
/// children, whereas entering that node through a nested class must remain
/// opaque except for a computed name.
fn lexical_arrow_children(arena: &NodeArena, node_idx: NodeIndex) -> Vec<NodeIndex> {
    let mut children = Vec::new();
    append_lexical_arrow_children(arena, node_idx, &mut children);
    children
}

fn append_lexical_arrow_children(
    arena: &NodeArena,
    node_idx: NodeIndex,
    children: &mut Vec<NodeIndex>,
) {
    let Some(node) = arena.get(node_idx) else {
        return;
    };
    match node.kind {
        kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION => {}
        kind if kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::CLASS_EXPRESSION =>
        {
            if let Some(class_data) = arena.get_class(node) {
                if let Some(modifiers) = class_data.modifiers.as_ref() {
                    append_decorator_children(arena, modifiers, children);
                }
                if let Some(heritage_clauses) = class_data.heritage_clauses.as_ref() {
                    children.extend(heritage_clauses.nodes.iter().copied());
                }
                for &member_idx in &class_data.members.nodes {
                    if class_element_has_enclosing_lexical_header(arena, member_idx) {
                        children.push(member_idx);
                    }
                }
            }
        }
        kind if kind == syntax_kind_ext::PROPERTY_DECLARATION => {
            let Some(property) = arena.get_property_decl(node) else {
                return;
            };
            append_lexical_property_header_children(
                arena,
                property.modifiers.as_ref(),
                property.name,
                children,
            );
        }
        kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
            let Some(method) = arena.get_method_decl(node) else {
                return;
            };
            append_lexical_callable_member_header_children(
                arena,
                method.modifiers.as_ref(),
                Some(method.name),
                &method.parameters,
                children,
            );
        }
        kind if kind == syntax_kind_ext::CONSTRUCTOR => {
            let Some(constructor) = arena.get_constructor(node) else {
                return;
            };
            append_lexical_callable_member_header_children(
                arena,
                constructor.modifiers.as_ref(),
                None,
                &constructor.parameters,
                children,
            );
        }
        kind if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR => {
            let Some(accessor) = arena.get_accessor(node) else {
                return;
            };
            append_lexical_callable_member_header_children(
                arena,
                accessor.modifiers.as_ref(),
                Some(accessor.name),
                &accessor.parameters,
                children,
            );
        }
        kind if kind == syntax_kind_ext::PARAMETER => {
            if let Some(modifiers) = arena
                .get_parameter(node)
                .and_then(|parameter| parameter.modifiers.as_ref())
            {
                append_decorator_children(arena, modifiers, children);
            }
        }
        _ => arena.append_children(node_idx, children),
    }
}

fn class_element_has_enclosing_lexical_header(arena: &NodeArena, member_idx: NodeIndex) -> bool {
    let Some(member) = arena.get(member_idx) else {
        return false;
    };
    match member.kind {
        kind if kind == syntax_kind_ext::PROPERTY_DECLARATION => {
            arena.get_property_decl(member).is_some_and(|property| {
                has_decorators(arena, property.modifiers.as_ref())
                    || computed_member_name_child(arena, property.name).is_some()
            })
        }
        kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
            arena.get_method_decl(member).is_some_and(|method| {
                has_decorators(arena, method.modifiers.as_ref())
                    || computed_member_name_child(arena, method.name).is_some()
                    || method
                        .parameters
                        .nodes
                        .iter()
                        .any(|&parameter| parameter_has_decorators(arena, parameter))
            })
        }
        kind if kind == syntax_kind_ext::CONSTRUCTOR => {
            arena.get_constructor(member).is_some_and(|constructor| {
                has_decorators(arena, constructor.modifiers.as_ref())
                    || constructor
                        .parameters
                        .nodes
                        .iter()
                        .any(|&parameter| parameter_has_decorators(arena, parameter))
            })
        }
        kind if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR => {
            arena.get_accessor(member).is_some_and(|accessor| {
                has_decorators(arena, accessor.modifiers.as_ref())
                    || computed_member_name_child(arena, accessor.name).is_some()
                    || accessor
                        .parameters
                        .nodes
                        .iter()
                        .any(|&parameter| parameter_has_decorators(arena, parameter))
            })
        }
        _ => false,
    }
}

fn append_lexical_property_header_children(
    arena: &NodeArena,
    modifiers: Option<&crate::parser::NodeList>,
    name: NodeIndex,
    children: &mut Vec<NodeIndex>,
) {
    if let Some(modifiers) = modifiers {
        append_decorator_children(arena, modifiers, children);
    }
    if let Some(name) = computed_member_name_child(arena, name) {
        children.push(name);
    }
}

fn append_lexical_callable_member_header_children(
    arena: &NodeArena,
    modifiers: Option<&crate::parser::NodeList>,
    name: Option<NodeIndex>,
    parameters: &crate::parser::NodeList,
    children: &mut Vec<NodeIndex>,
) {
    if let Some(modifiers) = modifiers {
        append_decorator_children(arena, modifiers, children);
    }
    if let Some(name) = name.and_then(|name| computed_member_name_child(arena, name)) {
        children.push(name);
    }
    children.extend(
        parameters
            .nodes
            .iter()
            .copied()
            .filter(|&parameter| parameter_has_decorators(arena, parameter)),
    );
}

fn append_decorator_children(
    arena: &NodeArena,
    modifiers: &crate::parser::NodeList,
    children: &mut Vec<NodeIndex>,
) {
    children.extend(modifiers.nodes.iter().copied().filter(|&modifier| {
        arena
            .get(modifier)
            .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
    }));
}

fn decorator_is_child(
    arena: &NodeArena,
    modifiers: Option<&crate::parser::NodeList>,
    child_idx: NodeIndex,
) -> bool {
    modifiers.is_some_and(|modifiers| {
        modifiers.nodes.contains(&child_idx)
            && arena
                .get(child_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
    })
}

fn has_decorators(arena: &NodeArena, modifiers: Option<&crate::parser::NodeList>) -> bool {
    modifiers.is_some_and(|modifiers| {
        modifiers.nodes.iter().any(|&modifier| {
            arena
                .get(modifier)
                .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
        })
    })
}

fn parameter_has_decorators(arena: &NodeArena, parameter_idx: NodeIndex) -> bool {
    arena
        .get(parameter_idx)
        .and_then(|parameter| arena.get_parameter(parameter))
        .is_some_and(|parameter| has_decorators(arena, parameter.modifiers.as_ref()))
}

fn parameter_with_decorators_is_child(
    arena: &NodeArena,
    parameters: &crate::parser::NodeList,
    child_idx: NodeIndex,
) -> bool {
    parameters.nodes.contains(&child_idx) && parameter_has_decorators(arena, child_idx)
}

fn contains_target_reference(
    arena: &NodeArena,
    node_idx: NodeIndex,
    target: ReferenceTarget,
) -> bool {
    let Some(node) = arena.get(node_idx) else {
        return false;
    };

    if target.include_keyword_check() {
        match target {
            ReferenceTarget::This
                if node.kind == SyntaxKind::ThisKeyword as u16
                    || node.kind == SyntaxKind::SuperKeyword as u16 =>
            {
                return true;
            }
            ReferenceTarget::Super if node.kind == SyntaxKind::SuperKeyword as u16 => {
                return true;
            }
            _ => {}
        }
    }

    if target == ReferenceTarget::NewTarget
        && node.kind == syntax_kind_ext::META_PROPERTY
        && let Some(access) = arena.get_access_expr(node)
        && arena
            .get(access.expression)
            .is_some_and(|kw| kw.kind == SyntaxKind::NewKeyword as u16)
        && arena
            .get(access.name_or_argument)
            .and_then(|name| arena.get_identifier(name))
            .is_some_and(|identifier| identifier.escaped_text == "target")
    {
        return true;
    }

    if node.is_identifier()
        && let Some(target_name) = target.identifier_name()
        && let Some(identifier) = arena.get_identifier(node)
    {
        return identifier.escaped_text == target_name;
    }

    target_reference_children(arena, node_idx, target)
        .into_iter()
        .any(|child_idx| contains_target_reference(arena, child_idx, target))
}

fn collect_target_references(
    arena: &NodeArena,
    node_idx: NodeIndex,
    target: ReferenceTarget,
    refs: &mut Vec<NodeIndex>,
) {
    let Some(node) = arena.get(node_idx) else {
        return;
    };

    if target.include_keyword_check() {
        match target {
            ReferenceTarget::This if node.kind == SyntaxKind::ThisKeyword as u16 => {
                refs.push(node_idx);
                return;
            }
            ReferenceTarget::Super if node.kind == SyntaxKind::SuperKeyword as u16 => {
                refs.push(node_idx);
                return;
            }
            _ => {}
        }
    }

    if node.is_identifier()
        && let Some(target_name) = target.identifier_name()
        && let Some(identifier) = arena.get_identifier(node)
        && identifier.escaped_text == target_name
    {
        refs.push(node_idx);
        return;
    }

    for child_idx in target_reference_children(arena, node_idx, target) {
        collect_target_references(arena, child_idx, target, refs);
    }
}

fn target_reference_children(
    arena: &NodeArena,
    node_idx: NodeIndex,
    target: ReferenceTarget,
) -> Vec<NodeIndex> {
    let Some(node) = arena.get(node_idx) else {
        return Vec::new();
    };

    match node.kind {
        kind if kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION =>
        {
            Vec::new()
        }
        kind if kind == syntax_kind_ext::CLASS_DECLARATION
            || kind == syntax_kind_ext::CLASS_EXPRESSION =>
        {
            if let Some(class_data) = arena.get_class(node) {
                let mut children = Vec::new();
                if let Some(modifiers) = class_data.modifiers.as_ref() {
                    children.extend(modifiers.nodes.iter().copied());
                }
                if let Some(heritage_clauses) = class_data.heritage_clauses.as_ref() {
                    children.extend(heritage_clauses.nodes.iter().copied());
                }
                for &member_idx in &class_data.members.nodes {
                    push_computed_member_name(arena, member_idx, &mut children);
                }
                children
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
            if let Some(method) = arena.get_method_decl(node) {
                let mut children = Vec::new();
                if target == ReferenceTarget::This
                    || target == ReferenceTarget::Super
                    || arena
                        .get(method.name)
                        .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                {
                    children.push(method.name);
                }
                children
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR => {
            if let Some(accessor) = arena.get_accessor(node) {
                let mut children = Vec::new();
                if target == ReferenceTarget::This
                    || target == ReferenceTarget::Super
                    || arena
                        .get(accessor.name)
                        .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                {
                    children.push(accessor.name);
                }
                children
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::ARROW_FUNCTION => {
            if let Some(func) = arena.get_function(node) {
                let mut children = Vec::new();
                for &param_idx in &func.parameters.nodes {
                    let Some(param_node) = arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = arena.get_parameter(param_node) else {
                        continue;
                    };
                    if param.initializer.is_some() {
                        children.push(param.initializer);
                    }
                }
                if func.body.is_some() {
                    children.push(func.body);
                }
                children
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
            if let Some(data) = arena.get_property_assignment(node) {
                let mut children = Vec::new();
                if arena
                    .get(data.name)
                    .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                {
                    children.push(data.name);
                }
                children.push(data.initializer);
                children
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::VARIABLE_STATEMENT
            || kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST =>
        {
            if let Some(var_stmt) = arena.get_variable(node) {
                var_stmt.declarations.nodes.clone()
            } else {
                Vec::new()
            }
        }
        kind if kind == syntax_kind_ext::VARIABLE_DECLARATION => {
            if let Some(decl) = arena.get_variable_declaration(node) {
                if decl.initializer.is_none() {
                    Vec::new()
                } else {
                    vec![decl.initializer]
                }
            } else {
                Vec::new()
            }
        }
        _ => arena.get_children(node_idx),
    }
}

fn computed_member_name(arena: &NodeArena, member_idx: NodeIndex) -> Option<NodeIndex> {
    let member = arena.get(member_idx)?;

    let name_idx = match member.kind {
        kind if kind == syntax_kind_ext::PROPERTY_DECLARATION => {
            arena.get_property_decl(member).map(|prop| prop.name)
        }
        kind if kind == syntax_kind_ext::METHOD_DECLARATION => {
            arena.get_method_decl(member).map(|method| method.name)
        }
        kind if kind == syntax_kind_ext::GET_ACCESSOR || kind == syntax_kind_ext::SET_ACCESSOR => {
            arena.get_accessor(member).map(|accessor| accessor.name)
        }
        _ => None,
    };

    name_idx.filter(|&name_idx| {
        arena
            .get(name_idx)
            .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
    })
}

fn computed_member_name_child(arena: &NodeArena, name_idx: NodeIndex) -> Option<NodeIndex> {
    arena
        .get(name_idx)
        .is_some_and(|name| name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        .then_some(name_idx)
}

fn computed_name_is_child(arena: &NodeArena, name_idx: NodeIndex, child_idx: NodeIndex) -> bool {
    name_idx == child_idx && computed_member_name_child(arena, name_idx).is_some()
}

fn push_computed_member_name(
    arena: &NodeArena,
    member_idx: NodeIndex,
    children: &mut Vec<NodeIndex>,
) {
    if let Some(name_idx) = computed_member_name(arena, member_idx) {
        children.push(name_idx);
    }
}

/// Check if a node is a private identifier (#field)
#[must_use]
pub fn is_private_identifier(arena: &NodeArena, name_idx: NodeIndex) -> bool {
    let Some(node) = arena.get(name_idx) else {
        return false;
    };
    node.kind == SyntaxKind::PrivateIdentifier as u16
}
