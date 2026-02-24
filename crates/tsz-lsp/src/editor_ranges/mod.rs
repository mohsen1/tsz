//! Editor range providers (folding ranges, selection ranges).
//!
//! Both features compute structural ranges from the AST for editor UI:
//! - **Folding**: collapsible regions for code blocks, imports, comments, `#region`
//! - **Selection Range**: expand/shrink selection by semantic boundaries

pub mod folding;
pub mod selection_range;
