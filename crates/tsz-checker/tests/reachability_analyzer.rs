use super::*;
use crate::flow_graph_builder::FlowGraphBuilder;
use tsz_parser::parser::ParserState;

#[test]
fn test_unreachable_after_return() {
    let source = r"
{
    return;
    let x = 1;  // Unreachable
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        let analyzer = ReachabilityAnalyzer::new(graph, arena);

        // Should have unreachable code
        assert!(analyzer.has_unreachable_code());
        assert!(analyzer.unreachable_count() > 0);
    }
}

#[test]
fn test_unreachable_after_throw() {
    let source = r"
{
    throw new Error();
    let x = 1;  // Unreachable
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        let analyzer = ReachabilityAnalyzer::new(graph, arena);

        // Should have unreachable code
        assert!(analyzer.has_unreachable_code());
    }
}

#[test]
fn test_unreachable_after_break() {
    let source = r"
while (true) {
    break;
    let x = 1;  // Unreachable
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        let analyzer = ReachabilityAnalyzer::new(graph, arena);

        // Should have unreachable code
        assert!(analyzer.has_unreachable_code());
    }
}

#[test]
fn test_unreachable_after_continue() {
    let source = r"
while (true) {
    continue;
    let x = 1;  // Unreachable
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        let analyzer = ReachabilityAnalyzer::new(graph, arena);

        // Should have unreachable code
        assert!(analyzer.has_unreachable_code());
    }
}

#[test]
fn test_reachable_code() {
    let source = r"
{
    let x = 1;
    let y = 2;
    return x + y;
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        let analyzer = ReachabilityAnalyzer::new(graph, arena);

        // All code before return is reachable
        // The return itself is reachable
        // No code after return, so no unreachable code
        assert!(!analyzer.has_unreachable_code() || analyzer.unreachable_count() == 0);
    }
}

#[test]
fn test_multiple_unreachable_sections() {
    let source = r"
{
    return;
    let x = 1;  // Unreachable
    let y = 2;  // Unreachable
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        let analyzer = ReachabilityAnalyzer::new(graph, arena);

        // Should have multiple unreachable nodes
        assert!(analyzer.unreachable_count() >= 2);
    }
}
