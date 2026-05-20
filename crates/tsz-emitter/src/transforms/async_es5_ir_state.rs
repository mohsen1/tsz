use crate::transforms::ir::IRNode;
use tsz_parser::parser::NodeIndex;

/// State for tracking async function transformation
#[derive(Debug, Default)]
pub struct AsyncTransformState {
    /// Current label counter for generator switch/case
    pub label_counter: u32,
    /// Whether we're currently inside an async function body
    pub in_async_body: bool,
    /// Whether any await expressions were found (determines if we need switch/case)
    pub has_await: bool,
    /// Whether the body references `arguments` (needs `var arguments_1 = arguments;`)
    pub captures_arguments: bool,
    /// Generated name used for captured `arguments` references.
    pub arguments_capture_name: String,
}

impl AsyncTransformState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset for a new async function
    pub fn reset(&mut self) {
        self.label_counter = 0;
        self.in_async_body = false;
        self.has_await = false;
        self.captures_arguments = false;
        self.arguments_capture_name.clear();
    }

    /// Get the next label number
    pub const fn next_label(&mut self) -> u32 {
        let label = self.label_counter;
        self.label_counter += 1;
        label
    }
}

pub(super) enum SuspendedAssignmentTarget {
    Property(String),
    Element(Box<IRNode>),
}

pub(super) enum ForInAssignmentTarget {
    Direct(Box<IRNode>),
    SuspendedProperty {
        object_suspension: NodeIndex,
        property: String,
    },
    SuspendedElement {
        object: ForInSuspendedObject,
        index: ForInSuspendedElementIndex,
    },
}

pub(super) enum ForInSuspendedObject {
    Direct(Box<IRNode>),
    Suspended(NodeIndex),
}

pub(super) enum ForInSuspendedElementIndex {
    Direct(Box<IRNode>),
    Suspended(NodeIndex),
}
