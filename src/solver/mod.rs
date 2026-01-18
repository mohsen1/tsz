use std::collections::{BinaryHeap, HashMap, HashSet};
use std::hash::Hash;
use std::cmp::Ordering;

/// Defines the strategy for processing the worklist.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorklistStrategy {
    /// Depth-First Search (LIFO). Useful for deep, localized exploration.
    Stack,
    /// Priority Queue (Best-First). Useful for finding shortest paths or minimal costs.
    Priority,
}

/// A generic worklist that can operate as either a Stack or a Priority Queue.
///
/// `T` is the item type.
/// `P` is the priority type (required only for Strategy::Priority).
pub struct Worklist<T, P> {
    strategy: WorklistStrategy,
    // Stack storage
    stack: Vec<T>,
    // Priority Queue storage
    // Note: BinaryHeap is a Max-Heap. For Min-Heap behavior (typical for Dijkstra/A*),
    // P must implement Ord in reverse (e.g. Reverse<i32>) or logic must handle it.
    priority: BinaryHeap<PrioritizedItem<T, P>>,
}

/// Wrapper to sort items by priority.
/// We implement PartialEq and Eq based on the item and priority to satisfy BinaryHeap requirements.
#[derive(Debug, Clone, Eq)]
struct PrioritizedItem<T, P> {
    item: T,
    priority: P,
}

impl<T: PartialEq, P: PartialEq> PartialEq for PrioritizedItem<T, P> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.item == other.item
    }
}

impl<T: PartialEq, P: PartialEq + Ord> PartialOrd for PrioritizedItem<T, P> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: PartialEq, P: PartialEq + Ord> Ord for PrioritizedItem<T, P> {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap. If we want min-priority behavior, we should reverse
        // the comparison here, or wrap P in std::cmp::Reverse. 
        // Assuming P is set up correctly (e.g. low cost = high priority in Ord), 
        // or standard max-heap behavior is desired.
        // Here we implement Max-Heap behavior on P.
        other.priority.cmp(&self.priority)
            .then_with(|| self.item.cmp(&other.item)) // Tie-breaker
    }
}

impl<T, P> Worklist<T, P> {
    /// Creates a new empty worklist with the specified strategy.
    pub fn new(strategy: WorklistStrategy) -> Self {
        Self {
            strategy,
            stack: Vec::new(),
            priority: BinaryHeap::new(),
        }
    }

    /// Clears the worklist, removing all items.
    pub fn clear(&mut self) {
        self.stack.clear();
        self.priority.clear();
    }

    /// Returns true if the worklist contains no items.
    pub fn is_empty(&self) -> bool {
        match self.strategy {
            WorklistStrategy::Stack => self.stack.is_empty(),
            WorklistStrategy::Priority => self.priority.is_empty(),
        }
    }

    /// Pushes an item onto the worklist.
    /// 
    /// * If strategy is Stack, `priority` is ignored.
    /// * If strategy is Priority, `priority` determines the order.
    pub fn push(&mut self, item: T, priority: P) {
        match self.strategy {
            WorklistStrategy::Stack => {
                self.stack.push(item);
            }
            WorklistStrategy::Priority => {
                self.priority.push(PrioritizedItem { item, priority });
            }
        }
    }

    /// Pops an item from the worklist.
    /// Returns None if the worklist is empty.
    pub fn pop(&mut self) -> Option<T> {
        match self.strategy {
            WorklistStrategy::Stack => self.stack.pop(),
            WorklistStrategy::Priority => self.priority.pop().map(|p| p.item),
        }
    }
}

/// A generic processing loop for fixed-point iteration or graph traversal.
///
/// # Type Parameters
/// * `N`: The type of the Node/State being processed.
/// * `Ctx`: The type of the context/mutable data stored during the loop.
/// * `E`: The type of Error returned by the processing function.
///
/// # Arguments
/// * `initial_items`: Items to seed the worklist.
/// * `worklist`: The configured worklist.
/// * `ctx`: The mutable context passed to the processor.
/// * `processor`: A closure that processes a node.
///   - Arguments: (&node, &mut context)
///   - Returns: Result<Vec<(NextNode, Priority)>, Error>
///
/// # Returns
/// * `Result<(), E>`: Ok if the loop completes without error, Err if the processor fails.
pub fn process_loop<N, Ctx, E, P>(
    initial_items: Vec<(N, P)>,
    mut worklist: Worklist<N, P>,
    ctx: &mut Ctx,
    mut processor: impl FnMut(&N, &mut Ctx) -> Result<Vec<(N, P)>, E>,
) -> Result<(), E>
where
    N: Eq + Hash + Clone, // Node must be hashable to allow visited set optimization (optional but common)
    P: Clone + Ord,       // Priority must be comparable
{
    // Initialize worklist
    for (item, prio) in initial_items {
        worklist.push(item, prio);
    }

    // Optional: Optimization to prevent re-processing the same node state indefinitely
    // unless the specific algorithm requires revisiting (e.g., Dijkstra can revisit, 
    // but basic dataflow analysis often needs a visited set).
    // Since this is generic, we will NOT enforce a visited set here to allow algorithms
    // that require revisiting nodes with better costs. The strategy should be handled
    // inside the `processor` or via specific wrapper functions.
    
    while !worklist.is_empty() {
        let current = worklist.pop().expect("Worklist was not empty");

        // Process the current node
        let neighbors = processor(&current, ctx)?;

        // Push neighbors onto the worklist
        for (next_node, priority) in neighbors {
            worklist.push(next_node, priority);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_lifo() {
        let mut wl = Worklist::new(WorklistStrategy::Stack);
        wl.push(1, 0); // Priority ignored
        wl.push(2, 0);
        wl.push(3, 0);

        assert_eq!(wl.pop(), Some(3));
        assert_eq!(wl.pop(), Some(2));
        assert_eq!(wl.pop(), Some(1));
        assert_eq!(wl.pop(), None);
    }

    #[test]
    fn test_priority_ordering() {
        // Using Reverse<i32> because BinaryHeap is a Max-Heap, 
        // and we usually want Min-Priority (lower number = higher priority).
        let mut wl = Worklist::new(WorklistStrategy::Priority);
        wl.push("Low", std::cmp::Reverse(10));
        wl.push("High", std::cmp::Reverse(1));
        wl.push("Mid", std::cmp::Reverse(5));

        assert_eq!(wl.pop(), Some("High"));
        assert_eq!(wl.pop(), Some("Mid"));
        assert_eq!(wl.pop(), Some("Low"));
    }

    #[test]
    fn test_process_loop_simple_counter() {
        let mut ctx = 0;
        let initial = vec![(10, 0)];
        
        // Process a linear chain 10 -> 9 -> ... -> 0
        let res = process_loop(initial, Worklist::new(WorklistStrategy::Stack), &mut ctx, |n, _ctx| {
            if *n > 0 {
                Ok(vec![((*n - 1), 0)])
            } else {
                Ok(vec![])
            }
        });

        assert!(res.is_ok());
        // We don't check ctx here because we didn't mutate it, just checking no crash
    }
}
```
