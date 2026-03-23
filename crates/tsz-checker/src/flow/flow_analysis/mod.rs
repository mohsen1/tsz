//! Flow Analysis Module
//!
//! This module contains flow analysis utilities for:
//!
//! ## Property Assignment Tracking
//! - Tracking property assignments in constructors and class bodies
//! - Detecting property used before assignment (TS2565)
//! - Tracking definite assignment of class properties
//! - Analyzing control flow in constructors
//!
//! ## Definite Assignment Analysis
//! - Checking variables are assigned before use (TS2454)
//! - TDZ (Temporal Dead Zone) checking for static blocks and computed properties
//! - Flow-based assignment tracking through control flow
//!
//! ## Type Narrowing
//! - typeof-based type narrowing
//! - Discriminated union narrowing
//! - Instance type narrowing
//!
//! The analysis is flow-sensitive and handles:
//! - If/else branches
//! - Switch statements
//! - Try/catch/finally blocks
//! - Loop statements
//! - Return/throw exits

mod core;
pub(crate) mod definite;
pub(crate) mod tdz;
pub(crate) mod usage;

pub(crate) use self::core::{ComputedKey, PropertyKey};
