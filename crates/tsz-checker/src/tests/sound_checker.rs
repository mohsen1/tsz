use super::*;

#[test]
fn test_sticky_freshness_basic() {
    let mut tracker = StickyFreshnessTracker::new();

    let sym = SymbolId(1);
    assert!(!tracker.is_binding_fresh(sym));

    tracker.mark_binding_fresh(sym, TypeId::OBJECT);
    assert!(tracker.is_binding_fresh(sym));
    assert_eq!(tracker.get_fresh_source_type(sym), Some(TypeId::OBJECT));

    tracker.consume_freshness(sym);
    assert!(!tracker.is_binding_fresh(sym));
}

#[test]
fn test_sticky_freshness_transfer() {
    let mut tracker = StickyFreshnessTracker::new();

    let from = SymbolId(1);
    let to = SymbolId(2);

    tracker.mark_binding_fresh(from, TypeId::OBJECT);
    tracker.transfer_freshness(from, to);

    assert!(tracker.is_binding_fresh(from));
    assert!(tracker.is_binding_fresh(to));
}

#[test]
fn test_sticky_freshness_property() {
    let mut tracker = StickyFreshnessTracker::new();

    let sym = SymbolId(1);
    let prop_hash = 12345u32;

    assert!(!tracker.is_property_fresh(sym, prop_hash));

    tracker.mark_property_fresh(sym, prop_hash);
    assert!(tracker.is_property_fresh(sym, prop_hash));
    assert!(!tracker.is_property_fresh(sym, 99999));
}
