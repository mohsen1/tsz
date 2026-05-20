//! Type constraint collection for generic type inference.
//!
//! This module implements the structural walker that collects type constraints
//! when inferring generic type parameters from argument types. It handles
//! recursive traversal of complex type structures (objects, functions, tuples,
//! conditionals, mapped types, etc.) to extract inference candidates.

mod reverse_mapped;
mod signatures;
mod walker;
