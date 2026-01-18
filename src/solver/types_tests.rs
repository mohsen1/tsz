```rust
use super::*;
use crate::grid::Grid;

#[test]
fn test_worklist_solve_simple() {
    let grid_str = "
        530070000
        600195000
        098000060
        800060003
        400803001
        700020006
        060000280
        000419005
        000080079
    ";

    let grid = Grid::from_str(grid_str.trim()).expect("Failed to parse grid");
    let solver = SudokuSolver::new(100_000);

    let solution = solver.solve(grid);

    assert!(solution.is_some(), "Solver should return a result");
    
    let sol = solution.unwrap();
    assert!(sol.solved, "The puzzle should be solved");
    
    // Verify a few cells to ensure correctness
    assert_eq!(sol.grid.get(0, 0), 5);
    assert_eq!(sol.grid.get(0, 1), 3);
    assert_eq!(sol.grid.get(0, 4), 7);
    
    // Check center box
    assert_eq!(sol.grid.get(4, 4), 8);
    
    // Verify no duplicates in rows/cols/boxes (implicit in is_full + validity, 
    // but let's check basic grid integrity)
    assert!(sol.grid.is_valid()); 
}

#[test]
fn test_worklist_solve_empty() {
    let grid = Grid::new(); // Empty grid
    let solver = SudokuSolver::new(500_000); // Empty grid takes more iterations

    let solution = solver.solve(grid);

    assert!(solution.is_some());
    let sol = solution.unwrap();
    assert!(sol.solved, "An empty grid should produce a valid Sudoku");
    assert!(sol.grid.is_valid());
}

#[test]
fn test_worklist_no_solution() {
    // Invalid grid (duplicate 5 in top left box)
    let grid_str = "
        553070000
        600195000
        098000060
        800060003
        400803001
        700020006
        060000280
        000419005
        000080079
    ";
    
    let grid = Grid::from_str(grid_str.trim()).expect("Failed to parse grid");
    let solver = SudokuSolver::new(100_000);

    let solution = solver.solve(grid);
    assert!(solution.is_some());
    let sol = solution.unwrap();
    assert!(!sol.solved, "Unsolvable puzzle should return solved: false");
}

#[test]
fn test_worklist_iterations_count() {
    // A puzzle that requires some backtracking
    let grid_str = "
        010000000
        000000200
        003000000
        000400000
        000050000
        000006000
        000000700
        008000000
        000000090
    ";
    
    let grid = Grid::from_str(grid_str.trim()).expect("Failed to parse grid");
    let solver = SudokuSolver::new(1_000_000);

    let solution = solver.solve(grid);
    assert!(solution.is_some());
    let sol = solution.unwrap();
    // We don't know exact iterations without a reference solver, 
    // but it should be > 0 for a hard puzzle.
    assert!(sol.iterations > 0);
}
```
