```rust
//! Flow narrowing analysis.
//!
//! This module is responsible for taking the broad type information (e.g., "this variable is an integer or a string")
//! derived from the unification solver and refining it based on control flow (e.g., "in this block, it is definitely a string").

use std::collections::HashMap;

use crate::ir::{BasicBlock, Location, Statement, Terminator};
use crate::type_system::{Ty, TyKind};
use crate::solver::{Solver, Worklist, WorklistDependency};

/// Context for the narrowing analysis.
pub struct NarrowingContext<'a> {
    /// Reference to the solver's type storage.
    types: &'a mut TyStore,
    /// Map from basic block IDs to their definitions.
    blocks: &'a HashMap<BasicBlock, BlockData>,
    /// The current state of narrowing for variables in each block.
    block_states: HashMap<BasicBlock, BlockState>,
}

/// Stores type information for a specific block.
#[derive(Debug, Clone)]
struct BlockState {
    /// Map from local variable index to its narrowed type within this block.
    local_types: HashMap<usize, Ty>,
}

impl<'a> NarrowingContext<'a> {
    /// Creates a new narrowing context.
    pub fn new(
        types: &'a mut TyStore,
        blocks: &'a HashMap<BasicBlock, BlockData>,
    ) -> Self {
        NarrowingContext {
            types,
            blocks,
            block_states: HashMap::new(),
        }
    }

    /// Returns the narrowed type of a local at a specific location.
    /// If no specific narrowing has occurred, returns the original type from the solver.
    pub fn type_at(&self, block: BasicBlock, local: usize) -> Ty {
        self.block_states
            .get(&block)
            .and_then(|state| state.local_types.get(&local))
            .copied()
            .unwrap_or_else(|| self.types.base_type(local))
    }

    /// Processes the worklist to iteratively refine types until a fixed point is reached.
    pub fn process(&mut self, worklist: &mut Worklist<BasicBlock>) {
        while let Some(block_id) = worklist.pop() {
            if let Some(block_data) = self.blocks.get(&block_id) {
                self.analyze_block(block_id, block_data, worklist);
            }
        }
    }

    /// Analyzes a single block, applying narrowings and propagating changes to successors.
    fn analyze_block(
        &mut self,
        block_id: BasicBlock,
        block_data: &BlockData,
        worklist: &mut Worklist<BasicBlock>,
    ) {
        // Retrieve the current state for this block.
        // In a more complex implementation, we might need to merge states from predecessors here.
        let state = self.block_states.entry(block_id).or_insert_with(|| {
            let mut initial_state = BlockState {
                local_types: HashMap::new(),
            };
            
            // Initialize with base types (could be optimized to lazy load)
            // We usually don't populate the map here unless we track modifications.
            // For this refactoring, we assume `type_at` handles fallback to base types.
            initial_state
        });

        // Track changes within this block to determine if successors need invalidation.
        let mut modified = false;

        // 1. Process Statements
        for stmt in &block_data.statements {
            self.apply_statement_narrowing(stmt, state, &mut modified);
        }

        // 2. Process Terminator
        if let Some(terminator) = &block_data.terminator {
            self.apply_terminator_narrowing(terminator, state, worklist, &mut modified);
        }

        // If we modified the state of *this* block based on new info, 
        // we might need to re-process predecessors. 
        // However, for a standard dataflow analysis, we usually propagate forward.
        // The `Worklist` handles the order.
        // If `state` changed, we add successors to the worklist.
        if modified {
            if let Some(terminator) = &block_data.terminator {
                for successor in terminator.successors() {
                    worklist.insert(successor);
                }
            }
        }
    }

    fn apply_statement_narrowing(
        &mut self,
        stmt: &Statement,
        state: &mut BlockState,
        modified: &mut bool,
    ) {
        match stmt {
            Statement::Assign(loc, rval) => {
                // For now, we don't narrow significantly on assignment alone,
                // but we might update the block state if RHS type is known.
                // This is a placeholder for type-specific narrowing logic.
                if let Some(base_ty) = self.types.base_type_opt(loc.local) {
                    if state.local_types.insert(loc.local, base_ty).is_none() {
                        *modified = true;
                    }
                }
            }
            Statement::Narrow(loc, target_ty) => {
                // Check if the narrowing is valid (intersection of current and target)
                let current = self.type_at(/* current block implied by state ownership */ 0, loc.local); // Hack: block id unused in simplified logic
                
                // Calculate intersection (simplified logic)
                if self.types.is_subtype(current, *target_ty) {
                    // Apply narrowing
                    if state.local_types.get(&loc.local) != Some(target_ty) {
                        state.local_types.insert(loc.local, *target_ty);
                        *modified = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn apply_terminator_narrowing(
        &mut self,
        terminator: &Terminator,
        state: &mut BlockState,
        worklist: &mut Worklist<BasicBlock>,
        modified: &mut bool,
    ) {
        match terminator {
            Terminator::If(cond, then_block, else_block) => {
                // In a full implementation, we would branch the state here,
                // creating specialized states for the `then` and `else` blocks.
                // e.g. if cond is "x is Some(T)", then_block gets x: Some(T).
                // For this refactoring, we ensure dependencies are marked.
                worklist.insert(*then_block);
                worklist.insert(*else_block);
            }
            Terminator::SwitchInt { .. } => {
                // Same as above for switch arms.
                for succ in terminator.successors() {
                    worklist.insert(succ);
                }
            }
            Terminator::Return => {}
            Terminator::Unreachable => {}
        }
    }
}

// --- Stub Types for Compilation Context --- //
// (These would normally be imported from crate::ir, crate::type_system, etc.)

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BasicBlock(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Location {
    pub block: BasicBlock,
    pub local: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum Statement {
    Assign(Location, RValue),
    Narrow(Location, Ty),
    Nop,
}

#[derive(Clone, Copy, Debug)]
pub enum RValue {
    Use(usize),
    Constant(i32),
}

#[derive(Clone, Debug)]
pub enum Terminator {
    If(usize, BasicBlock, BasicBlock),
    SwitchInt { discr: usize, targets: Vec<BasicBlock> },
    Return,
    Unreachable,
}

impl Terminator {
    fn successors(&self) -> Vec<BasicBlock> {
        match self {
            Terminator::If(_, t, e) => vec![*t, *e],
            Terminator::SwitchInt { targets, .. } => targets.clone(),
            Terminator::Return | Terminator::Unreachable => vec![],
        }
    }
}

pub struct BlockData {
    pub statements: Vec<Statement>,
    pub terminator: Option<Terminator>,
}

// --- Solver Infrastructure Stubs --- //

pub struct TyStore;

impl TyStore {
    pub fn base_type(&self, _local: usize) -> Ty {
        Ty::mk_unknown()
    }

    pub fn base_type_opt(&self, _local: usize) -> Option<Ty> {
        Some(Ty::mk_unknown())
    }

    pub fn is_subtype(&self, _ty1: Ty, _ty2: Ty) -> bool {
        true
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Ty(u32);

impl Ty {
    pub fn mk_unknown() -> Self {
        Ty(0)
    }
}

// --- Worklist Infrastructure --- //

/// A simple worklist implementation for fixed-point iteration.
pub struct Worklist<T> {
    list: Vec<T>,
    set: std::collections::HashSet<T>,
}

impl<T: std::hash::Hash + Eq + Copy> Worklist<T> {
    pub fn new() -> Self {
        Worklist {
            list: Vec::new(),
            set: std::collections::HashSet::new(),
        }
    }

    pub fn insert(&mut self, item: T) {
        if self.set.insert(item) {
            self.list.push(item);
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if let Some(item) = self.list.pop() {
            self.set.remove(&item);
            Some(item)
        } else {
            None
        }
    }
}

pub trait Worklist
