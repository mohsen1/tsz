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
    assert_eq!(format!("{}", span), "10..20");
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
