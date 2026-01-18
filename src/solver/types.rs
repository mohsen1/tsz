```rust
use crate::grid::Grid;
use crate::solver::metadata::Metadata;
use crate::solver::strategy::Strategy;
use std::fmt;

// Wrapper for the results
#[derive(Debug, Clone)]
pub struct Solution {
    pub grid: Grid,
    pub iterations: usize,
    pub solved: bool,
}

impl fmt::Display for Solution {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Solved: {} in {} iterations\nGrid:\n{}",
            self.solved, self.iterations, self.grid
        )
    }
}

#[derive(Debug, Clone)]
pub enum BacktrackReason {
    InvalidState,
    NoCandidates,
    MaxDepth,
}

/// The SudokuSolver encapsulates the logic for solving a Sudoku puzzle.
/// It operates on a Grid and uses Metadata to track possible candidates.
pub struct SudokuSolver {
    pub max_iterations: usize,
}

impl SudokuSolver {
    pub fn new(max_iterations: usize) -> Self {
        SudokuSolver { max_iterations }
    }

    /// Solves the Sudoku puzzle using a Worklist pattern instead of recursion.
    /// 
    /// # Algorithm
    /// 1. Initialize the worklist with the starting grid state.
    /// 2. Loop while there is work to do (worklist is not empty).
    /// 3. Pop a state from the worklist.
    /// 4. Validate the state; if invalid, continue to the next item (backtrack).
    /// 5. Check for completion; if complete, return the solution.
    /// 6. Calculate candidates for the current state.
    /// 7. If no candidates are available, continue (backtrack).
    /// 8. Push new states onto the worklist for each candidate value.
    pub fn solve(&self, initial_grid: Grid) -> Option<Solution> {
        let mut iterations = 0;
        
        // Worklist stores the state (grid and metadata) of branches we need to explore.
        let mut worklist: Vec<(Grid, Metadata)> = Vec::new();
        
        // Initial state: Push the starting grid and its associated metadata.
        let initial_metadata = Metadata::new(&initial_grid);
        worklist.push((initial_grid, initial_metadata));

        while let Some((current_grid, mut metadata)) = worklist.pop() {
            iterations += 1;

            if iterations > self.max_iterations {
                println!("Max iterations reached");
                continue;
            }

            // 1. Validate Current State
            // If the current grid configuration is invalid (duplicate numbers in row/col/box),
            // we discard this state and effectively backtrack by continuing to the next loop iteration.
            if !metadata.is_valid(&current_grid) {
                continue;
            }

            // 2. Check for Solution
            // If the grid is full and valid, we have found a solution.
            if current_grid.is_full() {
                return Some(Solution {
                    grid: current_grid,
                    iterations,
                    solved: true,
                });
            }

            // 3. Determine Next Cell and Candidates
            // Use the Strategy to find the best cell to fill and get possible values.
            let strategy = Strategy::new(&current_grid, &metadata);
            
            // If we can't find a cell to fill (e.g., no empty cells but grid isn't full? unlikely but safe)
            // or strategy logic fails, we backtrack.
            let (row, col) = match strategy.get_best_cell() {
                Some(cell) => cell,
                None => continue, 
            };

            let candidates = strategy.get_candidates(row, col);

            // If no valid candidates exist for the chosen cell, this path is a dead end.
            if candidates.is_empty() {
                continue;
            }

            // 4. Expand Search
            // For each candidate, create a new state and push it to the worklist.
            // Note: We iterate in reverse so that candidates are processed in order
            // (since pop() takes from the end).
            for value in candidates.into_iter().rev() {
                let mut new_grid = current_grid.clone();
                new_grid.set(row, col, value);

                // Update metadata. If the update implies an invalid state (e.g., constraint violation),
                // the metadata update function might handle it, but we re-check validity at the top of the loop.
                metadata.update(&mut new_grid, row, col, value);
                
                // Clone the updated metadata for the new branch
                let new_metadata = metadata.clone();
                
                worklist.push((new_grid, new_metadata));
            }
        }

        // If worklist is empty, no solution was found.
        Some(Solution {
            // Return the initial grid or the last failed state depending on preference.
            // Usually returning the initial state or a partial failure is expected.
            // We return the initial grid wrapped in Some to indicate 'process finished', 
            // but with solved: false.
            grid: initial_grid, 
            iterations,
            solved: false,
        })
    }
}
```
