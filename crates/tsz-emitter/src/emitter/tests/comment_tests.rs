use super::*;

#[test]
fn test_trailing_comments_parsing() {
    let text = "constructor(public p3:any) {} // OK";
    //                                       ^
    //                                       position 29 (after the closing brace)
    let comments = get_trailing_comment_ranges(text, 29);
    assert_eq!(comments.len(), 1);
    assert_eq!(
        &text[comments[0].pos as usize..comments[0].end as usize],
        "// OK"
    );
}

#[test]
fn test_trailing_comments_with_space() {
    let text = "} // OK\n";
    let comments = get_trailing_comment_ranges(text, 1); // after }
    assert_eq!(comments.len(), 1);
    assert_eq!(
        &text[comments[0].pos as usize..comments[0].end as usize],
        "// OK"
    );
}
