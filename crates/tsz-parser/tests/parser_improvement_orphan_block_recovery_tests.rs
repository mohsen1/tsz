//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — orphan block recovery.

use crate::parser::test_fixture::parse_source;

#[test]
fn test_orphan_catch_block_emits_ts1005() {
    // catch block without try should emit TS1005: 'try' expected
    let source = r"
function fn() {
    catch(x) { }
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 error for orphan catch block, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error message mentions 'try'
    let has_try_expected = diagnostics
        .iter()
        .any(|d| d.code == 1005 && d.message.to_lowercase().contains("try"));
    assert!(
        has_try_expected,
        "Expected TS1005 error to mention 'try', got diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_orphan_finally_block_emits_ts1005() {
    // finally block without try should emit TS1005: 'try' expected
    let source = r"
function fn() {
    finally { }
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 error for orphan finally block, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error message mentions 'try'
    let has_try_expected = diagnostics
        .iter()
        .any(|d| d.code == 1005 && d.message.to_lowercase().contains("try"));
    assert!(
        has_try_expected,
        "Expected TS1005 error to mention 'try', got diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_multiple_orphan_blocks_emit_separate_ts1005() {
    // Multiple orphan catch/finally blocks should each emit TS1005
    let source = r"
function fn() {
    finally { }
    catch (x) { }
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 2,
        "Expected 2 TS1005 errors for two orphan blocks, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );
}
