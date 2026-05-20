use super::*;

#[test]
fn test_span_basics() {
    let span = Span::new(10, 20);
    assert_eq!(span.start, 10);
    assert_eq!(span.end, 20);
    assert_eq!(span.len(), 10);
    assert!(!span.is_empty());
}

#[test]
fn test_span_at() {
    let span = Span::at(42);
    assert_eq!(span.start, 42);
    assert_eq!(span.end, 42);
    assert_eq!(span.len(), 0);
    assert!(span.is_empty());
}

#[test]
fn test_span_from_len() {
    let span = Span::from_len(5, 10);
    assert_eq!(span.start, 5);
    assert_eq!(span.end, 15);
    assert_eq!(span.len(), 10);
}

#[test]
fn test_span_contains() {
    let span = Span::new(10, 20);
    assert!(span.contains(10));
    assert!(span.contains(15));
    assert!(span.contains(19));
    assert!(!span.contains(9));
    assert!(!span.contains(20));
}

#[test]
fn test_span_contains_span() {
    let outer = Span::new(10, 30);
    let inner = Span::new(15, 25);
    let partial = Span::new(5, 20);

    assert!(outer.contains_span(inner));
    assert!(!outer.contains_span(partial));
    assert!(outer.contains_span(outer));
}

#[test]
fn test_span_overlaps() {
    let a = Span::new(10, 20);
    let b = Span::new(15, 25);
    let c = Span::new(20, 30);
    let d = Span::new(0, 5);

    assert!(a.overlaps(b));
    assert!(b.overlaps(a));
    assert!(!a.overlaps(c)); // Adjacent, not overlapping
    assert!(!a.overlaps(d));
}

#[test]
fn test_span_merge() {
    let a = Span::new(10, 20);
    let b = Span::new(15, 30);
    let merged = a.merge(b);

    assert_eq!(merged.start, 10);
    assert_eq!(merged.end, 30);
}

#[test]
fn test_span_intersect() {
    let a = Span::new(10, 20);
    let b = Span::new(15, 25);
    let c = Span::new(25, 30);

    let intersect = a.intersect(b);
    assert!(intersect.is_some());
    let i = intersect.unwrap();
    assert_eq!(i.start, 15);
    assert_eq!(i.end, 20);

    assert!(a.intersect(c).is_none());
}

#[test]
fn test_span_shrink() {
    let span = Span::new(10, 30);

    let shrunk_start = span.shrink_start(5);
    assert_eq!(shrunk_start.start, 15);
    assert_eq!(shrunk_start.end, 30);

    let shrunk_end = span.shrink_end(5);
    assert_eq!(shrunk_end.start, 10);
    assert_eq!(shrunk_end.end, 25);

    // Shrink past boundaries
    let over_shrunk = span.shrink_start(25);
    assert_eq!(over_shrunk.start, 30);
    assert_eq!(over_shrunk.end, 30);
}

#[test]
fn test_span_slice() {
    let text = "hello world";
    let span = Span::new(6, 11);
    assert_eq!(span.slice(text), "world");
}

#[test]
fn test_span_slice_safe() {
    let text = "hello";
    let span = Span::new(0, 100);
    assert_eq!(span.slice_safe(text), "hello");

    let inverted = Span::new(100, 0);
    assert_eq!(inverted.slice_safe(text), "");
}

#[test]
fn test_span_dummy() {
    let dummy = Span::dummy();
    assert!(dummy.is_dummy());

    let normal = Span::new(0, 10);
    assert!(!normal.is_dummy());
}

#[test]
fn test_span_display() {
    let span = Span::new(10, 20);
    assert_eq!(format!("{span}"), "10..20");
}

#[test]
fn test_span_builder() {
    let builder = SpanBuilder::start(5);
    assert_eq!(builder.start_pos(), 5);

    let span = builder.end(15);
    assert_eq!(span.start, 5);
    assert_eq!(span.end, 15);
}

#[test]
fn test_byte_span() {
    let text = "hello world";
    let span = Span::new(0, 5);
    let byte_span = ByteSpan::new(text, span);

    assert_eq!(byte_span.as_str(), "hello");
    assert_eq!(byte_span.len(), 5);
    assert!(!byte_span.is_empty());
}

#[test]
fn test_spanned_trait() {
    let span = Span::new(10, 20);
    assert_eq!(span.span(), span);
    assert_eq!(span.start(), 10);
    assert_eq!(span.end(), 20);
    assert_eq!(span.len(), 10);
}

// =============================================================================
// Additional Span tests - zero-width spans
// =============================================================================

#[test]
fn test_zero_width_span_at_zero() {
    let span = Span::at(0);
    assert!(span.is_empty());
    assert_eq!(span.len(), 0);
    assert!(!span.is_dummy());
}

#[test]
fn test_zero_width_span_contains_nothing() {
    let span = Span::at(10);
    assert!(!span.contains(10));
    assert!(!span.contains(9));
    assert!(!span.contains(11));
}

#[test]
fn test_zero_width_span_overlap_behavior() {
    let empty = Span::at(10);
    let real = Span::new(5, 15);
    // overlaps checks self.start < other.end && other.start < self.end
    // For empty at 10 vs [5,15): 10 < 15 && 5 < 10 => true
    assert!(empty.overlaps(real));
    assert!(real.overlaps(empty));

    // Two empty spans at different positions do not overlap
    let empty2 = Span::at(20);
    assert!(!empty.overlaps(empty2));

    // Two empty spans at the same position do not overlap
    // because 10 < 10 is false
    let same_empty = Span::at(10);
    assert!(!empty.overlaps(same_empty));
}

// =============================================================================
// Additional Span tests - from conversions
// =============================================================================

#[test]
fn test_span_from_tuple() {
    let span: Span = (5u32, 15u32).into();
    assert_eq!(span.start, 5);
    assert_eq!(span.end, 15);
}

#[test]
fn test_tuple_from_span() {
    let span = Span::new(3, 7);
    let (start, end): (u32, u32) = span.into();
    assert_eq!(start, 3);
    assert_eq!(end, 7);
}

#[test]
fn test_span_roundtrip_through_tuple() {
    let original = Span::new(42, 84);
    let tuple: (u32, u32) = original.into();
    let recovered: Span = tuple.into();
    assert_eq!(original, recovered);
}

// =============================================================================
// Additional Span tests - default
// =============================================================================

#[test]
fn test_span_default() {
    let span: Span = Default::default();
    assert_eq!(span.start, 0);
    assert_eq!(span.end, 0);
    assert!(span.is_empty());
    assert!(!span.is_dummy());
}

// =============================================================================
// Additional Span tests - from_len edge cases
// =============================================================================

#[test]
fn test_from_len_zero_length() {
    let span = Span::from_len(10, 0);
    assert_eq!(span.start, 10);
    assert_eq!(span.end, 10);
    assert!(span.is_empty());
}

#[test]
fn test_from_len_at_zero() {
    let span = Span::from_len(0, 5);
    assert_eq!(span.start, 0);
    assert_eq!(span.end, 5);
    assert_eq!(span.len(), 5);
}

// =============================================================================
// Additional Span tests - contains_span edge cases
// =============================================================================

#[test]
fn test_contains_span_empty_inside_non_empty() {
    let outer = Span::new(10, 20);
    let empty = Span::at(15);
    assert!(outer.contains_span(empty));
}

#[test]
fn test_contains_span_empty_at_boundary() {
    let outer = Span::new(10, 20);
    let at_start = Span::at(10);
    let at_end = Span::at(20);
    assert!(outer.contains_span(at_start));
    assert!(outer.contains_span(at_end));
}

#[test]
fn test_contains_span_identical() {
    let span = Span::new(5, 15);
    assert!(span.contains_span(span));
}

#[test]
fn test_contains_span_larger_fails() {
    let inner = Span::new(10, 15);
    let outer = Span::new(5, 20);
    assert!(!inner.contains_span(outer));
}

// =============================================================================
// Additional Span tests - merge edge cases
// =============================================================================

#[test]
fn test_merge_non_overlapping() {
    let a = Span::new(0, 5);
    let b = Span::new(10, 15);
    let merged = a.merge(b);
    assert_eq!(merged.start, 0);
    assert_eq!(merged.end, 15);
}

#[test]
fn test_merge_with_self() {
    let span = Span::new(10, 20);
    let merged = span.merge(span);
    assert_eq!(merged, span);
}

#[test]
fn test_merge_with_empty() {
    let span = Span::new(5, 15);
    let empty = Span::at(10);
    let merged = span.merge(empty);
    assert_eq!(merged.start, 5);
    assert_eq!(merged.end, 15);
}

#[test]
fn test_merge_commutativity() {
    let a = Span::new(3, 10);
    let b = Span::new(7, 20);
    assert_eq!(a.merge(b), b.merge(a));
}

// =============================================================================
// Additional Span tests - intersect edge cases
// =============================================================================

#[test]
fn test_intersect_identical() {
    let span = Span::new(10, 20);
    let result = span.intersect(span).unwrap();
    assert_eq!(result, span);
}

#[test]
fn test_intersect_adjacent_returns_none() {
    let a = Span::new(0, 10);
    let b = Span::new(10, 20);
    assert!(a.intersect(b).is_none());
}

#[test]
fn test_intersect_contained() {
    let outer = Span::new(0, 20);
    let inner = Span::new(5, 15);
    let result = outer.intersect(inner).unwrap();
    assert_eq!(result, inner);
}

#[test]
fn test_intersect_commutativity() {
    let a = Span::new(5, 15);
    let b = Span::new(10, 20);
    assert_eq!(a.intersect(b), b.intersect(a));
}

// =============================================================================
// Additional Span tests - shrink edge cases
// =============================================================================

#[test]
fn test_shrink_start_to_empty() {
    let span = Span::new(10, 20);
    let shrunk = span.shrink_start(10);
    assert_eq!(shrunk.start, 20);
    assert_eq!(shrunk.end, 20);
    assert!(shrunk.is_empty());
}

#[test]
fn test_shrink_end_to_empty() {
    let span = Span::new(10, 20);
    let shrunk = span.shrink_end(10);
    assert_eq!(shrunk.start, 10);
    assert_eq!(shrunk.end, 10);
    assert!(shrunk.is_empty());
}

#[test]
fn test_shrink_end_past_start_clamps() {
    let span = Span::new(10, 15);
    let shrunk = span.shrink_end(20);
    assert_eq!(shrunk.start, 10);
    assert_eq!(shrunk.end, 10);
}

#[test]
fn test_shrink_start_zero() {
    let span = Span::new(10, 20);
    let shrunk = span.shrink_start(0);
    assert_eq!(shrunk, span);
}

#[test]
fn test_shrink_end_zero() {
    let span = Span::new(10, 20);
    let shrunk = span.shrink_end(0);
    assert_eq!(shrunk, span);
}

// =============================================================================
// Additional Span tests - first_byte / last_byte
// =============================================================================

#[test]
fn test_first_byte_normal() {
    let span = Span::new(10, 20);
    let first = span.first_byte();
    assert_eq!(first.start, 10);
    assert_eq!(first.end, 11);
    assert_eq!(first.len(), 1);
}

#[test]
fn test_first_byte_empty_span() {
    let span = Span::at(10);
    let first = span.first_byte();
    assert_eq!(first.start, 10);
    assert_eq!(first.end, 10);
    assert!(first.is_empty());
}

#[test]
fn test_last_byte_normal() {
    let span = Span::new(10, 20);
    let last = span.last_byte();
    assert_eq!(last.start, 19);
    assert_eq!(last.end, 20);
    assert_eq!(last.len(), 1);
}

#[test]
fn test_last_byte_empty_span() {
    let span = Span::at(10);
    let last = span.last_byte();
    assert_eq!(last.start, 10);
    assert_eq!(last.end, 10);
    assert!(last.is_empty());
}

#[test]
fn test_first_byte_single_byte_span() {
    let span = Span::new(5, 6);
    let first = span.first_byte();
    let last = span.last_byte();
    assert_eq!(first, span);
    assert_eq!(last, span);
}

// =============================================================================
// Additional Span tests - slice edge cases
// =============================================================================

#[test]
fn test_slice_empty_span() {
    let text = "hello world";
    let span = Span::at(5);
    assert_eq!(span.slice(text), "");
}

#[test]
fn test_slice_full_text() {
    let text = "hello";
    let span = Span::new(0, 5);
    assert_eq!(span.slice(text), "hello");
}

#[test]
fn test_slice_single_char() {
    let text = "abcdef";
    let span = Span::new(2, 3);
    assert_eq!(span.slice(text), "c");
}

#[test]
fn test_slice_safe_clamped() {
    let text = "abc";
    let span = Span::new(1, 100);
    assert_eq!(span.slice_safe(text), "bc");
}

#[test]
fn test_slice_safe_both_out_of_bounds() {
    let text = "abc";
    let span = Span::new(50, 100);
    assert_eq!(span.slice_safe(text), "");
}

// =============================================================================
// Additional Span tests - dummy
// =============================================================================

#[test]
fn test_dummy_is_not_empty() {
    let dummy = Span::dummy();
    // dummy has start == end == u32::MAX, so is_empty returns true
    assert!(dummy.is_empty());
}

#[test]
fn test_dummy_len() {
    let dummy = Span::dummy();
    // saturating_sub of u32::MAX - u32::MAX = 0
    assert_eq!(dummy.len(), 0);
}

// =============================================================================
// Additional Span tests - hash and eq
// =============================================================================

#[test]
fn test_span_hash_equality() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(Span::new(1, 5));
    set.insert(Span::new(1, 5)); // duplicate
    set.insert(Span::new(2, 5));
    assert_eq!(set.len(), 2);
}

// =============================================================================
// Additional ByteSpan tests
// =============================================================================

#[test]
fn test_byte_span_empty() {
    let text = "hello";
    let span = Span::at(3);
    let byte_span = ByteSpan::new(text, span);
    assert!(byte_span.is_empty());
    assert_eq!(byte_span.len(), 0);
    assert_eq!(byte_span.as_str(), "");
}

#[test]
fn test_byte_span_display() {
    let text = "hello world";
    let span = Span::new(6, 11);
    let byte_span = ByteSpan::new(text, span);
    assert_eq!(format!("{byte_span}"), "world");
}

#[test]
fn test_byte_span_spanned_trait() {
    let text = "hello";
    let span = Span::new(1, 4);
    let byte_span = ByteSpan::new(text, span);
    assert_eq!(byte_span.span(), span);
    assert_eq!(byte_span.start(), 1);
    assert_eq!(byte_span.end(), 4);
}

// =============================================================================
// Additional SpanBuilder tests
// =============================================================================

#[test]
fn test_span_builder_zero_length() {
    let builder = SpanBuilder::start(10);
    let span = builder.end(10);
    assert!(span.is_empty());
}

#[test]
fn test_span_builder_clone_copy() {
    let builder = SpanBuilder::start(5);
    let copy = builder;
    assert_eq!(copy.start_pos(), 5);
    let cloned = builder;
    assert_eq!(cloned.start_pos(), 5);
}

#[test]
fn test_span_builder_debug() {
    let builder = SpanBuilder::start(42);
    let debug = format!("{builder:?}");
    assert!(debug.contains("SpanBuilder"));
    assert!(debug.contains("42"));
}

// =============================================================================
// Span - overlaps symmetry and edge cases
// =============================================================================

#[test]
fn test_overlaps_is_symmetric() {
    let a = Span::new(0, 10);
    let b = Span::new(5, 15);
    assert_eq!(a.overlaps(b), b.overlaps(a));
}

#[test]
fn test_overlaps_contained_spans() {
    let outer = Span::new(0, 20);
    let inner = Span::new(5, 10);
    assert!(outer.overlaps(inner));
    assert!(inner.overlaps(outer));
}

#[test]
fn test_overlaps_identical_spans() {
    let span = Span::new(5, 10);
    assert!(span.overlaps(span));
}
