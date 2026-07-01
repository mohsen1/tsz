use crate::enums::evaluator::EnumValue;
use rustc_hash::FxHashMap;
use tsz_parser::parser::NodeIndex;

/// A class field initializer entry:
/// (`field_name`, `initializer_node`, `init_end`, `leading_comments`,
/// `trailing_comments`, `source_order`).
pub(crate) type FieldInit = (String, NodeIndex, u32, Vec<String>, Vec<String>, u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivateFieldStorageKind {
    WeakMap,
    Value,
}

/// A private field constructor initializer entry:
/// (`weakmap_name`, `has_initializer`, `initializer_node`, `leading_comments`,
/// `trailing_comments`, `source_order`, `storage_kind`).
pub(crate) type PrivateFieldConstructorInit = (
    String,
    bool,
    NodeIndex,
    Vec<String>,
    Vec<String>,
    u32,
    PrivateFieldStorageKind,
);

#[derive(Debug, Clone)]
pub(crate) struct StaticPrivateInit {
    pub(crate) storage_name: String,
    pub(crate) initializer: NodeIndex,
    pub(crate) storage_kind: PrivateFieldStorageKind,
    /// Source position of the declaring member, used to interleave the value
    /// initialization with sibling public static field inits and static blocks
    /// in source order (tsc's initialization-order semantics).
    pub(crate) member_pos: u32,
}

/// A const enum entry scoped to a specific region of the source.
/// File-level const enums use `(0, u32::MAX)` so they match any position.
/// Function-scoped const enums use the enclosing function's `(pos, end)`.
#[derive(Debug, Clone)]
pub(crate) struct ScopedConstEnum {
    pub scope_start: u32,
    pub scope_end: u32,
    pub values: FxHashMap<String, EnumValue>,
}

/// Info about a private class member for lowering.
/// Determines the kind argument for `__classPrivateFieldGet`/`__classPrivateFieldSet`.
#[derive(Debug, Clone)]
pub(crate) struct PrivateMemberInfo {
    /// The kind: "f" for field, "m" for method, "a" for accessor.
    pub kind: &'static str,
    /// For static fields: the function ref variable name (e.g., `_C_field`).
    /// For methods: the function variable name (e.g., `_C_method`).
    /// For accessors: the getter variable name (e.g., `_C_prop_get`).
    pub fn_ref: Option<String>,
    /// For accessors: the setter variable name (e.g., `_C_prop_set`).
    pub setter_ref: Option<String>,
    /// The WeakSet/class-alias variable used as the `state` argument.
    /// For instance methods/accessors: `_ClassName_instances`.
    pub state_var: Option<String>,
}

/// Info about a private accessor function to emit after the class body.
#[derive(Debug, Clone)]
pub(crate) struct PrivateAccessorDef {
    /// The variable name (e.g., `_C_prop_get`).
    pub var_name: String,
    /// The body node index, or `None` for invalid no-body accessors that `tsc`
    /// recovers as empty extracted helpers.
    pub body: Option<NodeIndex>,
    /// Optional setter parameter node index.
    pub param: Option<NodeIndex>,
    /// Whether the extracted accessor function is async.
    pub is_async: bool,
}

/// Info about a private method function to emit after the class body.
#[derive(Debug, Clone)]
pub(crate) struct PrivateMethodDef {
    /// The variable name (e.g., `_C_method`).
    pub var_name: String,
    /// The body node index.
    pub body: NodeIndex,
    /// Method parameter node indices.
    pub params: Vec<NodeIndex>,
    /// Whether the extracted method function is async.
    pub is_async: bool,
    /// Whether the extracted method function is a generator.
    pub is_generator: bool,
}

/// How a class property name should be emitted in `ClassName.name = ...` assignments.
#[derive(Clone)]
pub(crate) enum PropertyNameEmit {
    /// Identifier: `ClassName.foo = ...`
    Dot(String),
    /// String literal: `ClassName["foo"] = ...`
    Bracket(String),
    /// Numeric literal: `ClassName[0] = ...`
    BracketNumeric(String),
}
