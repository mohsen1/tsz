use super::*;

#[test]
fn test_simple_inheritance() {
    let graph = InheritanceGraph::new();
    let parent = SymbolId(1);
    let child = SymbolId(2);

    graph.add_inheritance(child, &[parent]);

    assert!(graph.is_derived_from(child, parent));
    assert!(!graph.is_derived_from(parent, child));
}

#[test]
fn test_transitive_inheritance() {
    let graph = InheritanceGraph::new();
    let a = SymbolId(1);
    let b = SymbolId(2);
    let c = SymbolId(3);

    // A -> B -> C
    graph.add_inheritance(b, &[a]);
    graph.add_inheritance(c, &[b]);

    assert!(graph.is_derived_from(c, a)); // Transitive
    assert!(graph.is_derived_from(c, b));
    assert!(!graph.is_derived_from(a, c));
}

#[test]
fn test_diamond_inheritance() {
    let graph = InheritanceGraph::new();
    let a = SymbolId(1);
    let b = SymbolId(2);
    let c = SymbolId(3);
    let d = SymbolId(4);

    // Diamond: A is top, B and C extend A, D extends both B and C
    graph.add_inheritance(b, &[a]);
    graph.add_inheritance(c, &[a]);
    graph.add_inheritance(d, &[b, c]);

    assert!(graph.is_derived_from(d, a)); // D derives from A through both paths
    assert!(graph.is_derived_from(d, b));
    assert!(graph.is_derived_from(d, c));
}

#[test]
fn test_cycle_detection() {
    let graph = InheritanceGraph::new();
    let a = SymbolId(1);
    let b = SymbolId(2);
    let c = SymbolId(3);

    // A -> B
    graph.add_inheritance(b, &[a]);

    // Check if adding C -> A would create cycle (A -> B -> C -> A)
    // This should not create a cycle yet
    assert!(!graph.detects_cycle(c, a));

    // Now add the cycle-creating edge
    graph.add_inheritance(a, &[c]);

    // Now parent is derived from child, so detects_cycle should return true
    assert!(graph.detects_cycle(a, b));
}

#[test]
fn test_multiple_inheritance() {
    let graph = InheritanceGraph::new();
    let a = SymbolId(1);
    let b = SymbolId(2);
    let c = SymbolId(3);

    // C extends both A and B
    graph.add_inheritance(c, &[a, b]);

    assert!(graph.is_derived_from(c, a));
    assert!(graph.is_derived_from(c, b));

    let mro = graph.get_resolution_order(c);
    // MRO should be C, then A, then B (or C, B, A depending on order)
    assert_eq!(mro[0], c);
}

#[test]
fn test_common_ancestor() {
    let graph = InheritanceGraph::new();
    let a = SymbolId(1);
    let b = SymbolId(2);
    let c = SymbolId(3);
    let d = SymbolId(4);

    // Diamond: A at top, B and C extend A, D extends both
    graph.add_inheritance(b, &[a]);
    graph.add_inheritance(c, &[a]);
    graph.add_inheritance(d, &[b, c]);

    // Common ancestor of B and C should be A
    assert_eq!(graph.find_common_ancestor(b, c), Some(a));

    // Common ancestor of D and B should be B
    assert_eq!(graph.find_common_ancestor(d, b), Some(b));
}

#[test]
fn test_no_common_ancestor() {
    let graph = InheritanceGraph::new();
    let a = SymbolId(1);
    let b = SymbolId(2);
    let c = SymbolId(3);
    let d = SymbolId(4);

    // Two separate chains: A->B and C->D
    graph.add_inheritance(b, &[a]);
    graph.add_inheritance(d, &[c]);

    // No common ancestor
    assert_eq!(graph.find_common_ancestor(b, d), None);
}
