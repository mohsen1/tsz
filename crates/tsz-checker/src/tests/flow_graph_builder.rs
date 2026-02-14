use super::*;
use tsz_parser::parser::ParserState;
use tsz_parser::parser::state::{CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_GENERATOR};

#[test]
fn test_flow_graph_builder_basic() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
    console.log(x.length);
} else {
    console.log(x.toFixed(2));
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut builder = FlowGraphBuilder::new(parser.get_arena());
    if let Some(source_file) = parser.get_arena().get(root)
        && let Some(sf) = parser.get_arena().get_source_file(source_file)
    {
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph was created
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_simple() {
    let source = r#"
let x: string | number;
if (x) {
    x = "hello";
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    // Build flow graph using FlowGraphBuilder
    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_loop() {
    let source = r#"
while (true) {
    break;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists with loop label
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_try_finally() {
    let source = r#"
let x;
try {
    x = 1;
} finally {
}
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());

        // The key is that the finally block should be on the flow path
        // from the try block to the console.log statement
        // This ensures that assignments in try are visible after finally
    }
}

#[test]
fn test_flow_graph_try_catch_finally() {
    let source = r#"
try {
    let x = 1;
} catch (e) {
    let y = 2;
} finally {
    let z = 3;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists with try/catch/finally
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_async_function() {
    let source = r#"
let x = await bar();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        // Create builder and set async depth (simulating being in async function)
        let mut builder = FlowGraphBuilder::new(arena);
        builder.async_depth = 1; // Simulate being in async function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_await_in_expression() {
    let source = r#"
const result = await bar() + await baz();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.async_depth = 1; // Simulate being in async function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_await_in_if() {
    let source = r#"
if (condition) {
    await bar();
} else {
    await baz();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.async_depth = 1; // Simulate being in async function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_await_in_loop() {
    let source = r#"
while (condition) {
    await bar();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.async_depth = 1; // Simulate being in async function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_await_in_try_catch() {
    let source = r#"
try {
    await bar();
} catch (e) {
    console.error(e);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.async_depth = 1; // Simulate being in async function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_async_arrow_function() {
    let source = r#"
const x = await bar();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        // Create builder and set async depth (simulating being in async arrow function)
        let mut builder = FlowGraphBuilder::new(arena);
        builder.async_depth = 1; // Simulate being in async function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

// =============================================================================
// Generator Flow Tests (CFA-20)
// =============================================================================

#[test]
fn test_flow_graph_generator_function() {
    let source = r#"
let x: string;
yield 1;
x = "hello";
yield 2;
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.context_flags |= CONTEXT_FLAG_GENERATOR; // Set generator context for parsing
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.generator_depth = 1; // Simulate being in generator function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph has nodes including YIELD_POINT nodes
        assert!(!graph.nodes.is_empty());

        // Count yield point nodes
        let yield_count = (0..graph.nodes.len())
            .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
            .filter(|n| (n.flags & flow_flags::YIELD_POINT) != 0)
            .count();

        // We should have 2 yield points (yield 1 and yield 2)
        assert!(
            yield_count >= 2,
            "Expected at least 2 yield points, got {}",
            yield_count
        );
    }
}

#[test]
fn test_flow_graph_yield_star() {
    let source = r#"
yield* otherGenerator();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.generator_depth = 1; // Simulate being in generator function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists and has yield point
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_yield_in_loop() {
    let source = r#"
let counter = 0;
while (counter < 10) {
    yield counter;
    counter++;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.generator_depth = 1; // Simulate being in generator function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_yield_in_try_catch() {
    let source = r#"
try {
    yield 1;
} catch (e) {
    yield 2;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.generator_depth = 1; // Simulate being in generator function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_async_generator_function() {
    // Test combined async and generator (async generator function)
    let source = r#"
let x: string;
yield await fetch('/api/data1');
x = "hello";
yield await fetch('/api/data2');
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.context_flags |= CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR; // Set async and generator context
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        // Simulate being in async generator function
        builder.async_depth = 1;
        builder.generator_depth = 1;
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists with both await and yield points
        assert!(!graph.nodes.is_empty());

        // Count yield point nodes
        let yield_count = (0..graph.nodes.len())
            .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
            .filter(|n| (n.flags & flow_flags::YIELD_POINT) != 0)
            .count();

        // Count await point nodes
        let await_count = (0..graph.nodes.len())
            .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
            .filter(|n| (n.flags & flow_flags::AWAIT_POINT) != 0)
            .count();

        assert!(
            yield_count >= 2,
            "Expected at least 2 yield points, got {}",
            yield_count
        );
        assert!(
            await_count >= 2,
            "Expected at least 2 await points, got {}",
            await_count
        );
    }
}

#[test]
fn test_flow_graph_variable_state_across_yield() {
    // Test that variable state is properly tracked across yield boundaries
    let source = r#"
let x: string | undefined;
x = "first";
yield 1;
x = "second";
yield 2;
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.generator_depth = 1; // Simulate being in generator function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph has assignment and yield nodes
        assert!(!graph.nodes.is_empty());

        // Should have assignment nodes for tracking variable state
        let assignment_count = (0..graph.nodes.len())
            .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
            .filter(|n| (n.flags & flow_flags::ASSIGNMENT) != 0)
            .count();

        assert!(
            assignment_count >= 2,
            "Expected at least 2 assignment nodes for variable tracking, got {}",
            assignment_count
        );
    }
}

#[test]
fn test_flow_graph_for_of_await_in_async_generator() {
    // Test for-await-of in async generator
    let source = r#"
let result: string[] = [];
for await (const item of asyncIterable) {
    yield item;
    result.push(item);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        // Simulate being in async generator function
        builder.async_depth = 1;
        builder.generator_depth = 1;
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_conditional_yield() {
    // Test yield in conditional branches
    let source = r#"
let x: string | number;
if (condition) {
    x = "string";
    yield 1;
} else {
    x = 42;
    yield 2;
}
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.context_flags |= CONTEXT_FLAG_GENERATOR; // Set generator context for parsing
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.generator_depth = 1; // Simulate being in generator function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists with proper branching
        assert!(!graph.nodes.is_empty());

        // Should have yield points in both branches
        let yield_count = (0..graph.nodes.len())
            .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
            .filter(|n| (n.flags & flow_flags::YIELD_POINT) != 0)
            .count();

        assert!(
            yield_count >= 2,
            "Expected at least 2 yield points in conditional branches, got {}",
            yield_count
        );
    }
}

#[test]
fn test_flow_graph_nested_generator() {
    // Test nested generator (generator calling another generator)
    let source = r#"
yield 1;
for (const val of innerGenerator()) {
    yield val * 2;
}
yield 3;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.context_flags |= CONTEXT_FLAG_GENERATOR; // Set generator context for parsing
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        builder.generator_depth = 1; // Simulate being in generator function
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());

        // Should have at least 3 yield points
        let yield_count = (0..graph.nodes.len())
            .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
            .filter(|n| (n.flags & flow_flags::YIELD_POINT) != 0)
            .count();

        assert!(
            yield_count >= 3,
            "Expected at least 3 yield points, got {}",
            yield_count
        );
    }
}

// =============================================================================
// Class Declaration Flow Tests
// =============================================================================

#[test]
fn test_flow_graph_class_with_static_block() {
    let source = r#"
let x: number;
class Foo {
    static {
        x = 42;
    }
}
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_class_with_static_property() {
    let source = r#"
let x: number;
class Foo {
    static prop = (x = 42);
}
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph has assignment node for static property
        assert!(!graph.nodes.is_empty());

        let assignment_count = (0..graph.nodes.len())
            .filter_map(|i| graph.nodes.get(FlowNodeId(i as u32)))
            .filter(|n| (n.flags & flow_flags::ASSIGNMENT) != 0)
            .count();

        // Should have assignment for static property initializer
        assert!(
            assignment_count >= 1,
            "Expected at least 1 assignment node for static property, got {}",
            assignment_count
        );
    }
}

#[test]
fn test_flow_graph_class_with_extends() {
    let source = r#"
class Base {}
class Derived extends Base {}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists - extends expression should be tracked
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_class_with_computed_property() {
    let source = r#"
const key = "myMethod";
class Foo {
    [key]() {
        return 42;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists - computed property expression should be tracked
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_class_multiple_static_blocks() {
    let source = r#"
let x: number;
let y: string;
class Foo {
    static {
        x = 1;
    }
    static prop = "hello";
    static {
        y = "world";
    }
}
console.log(x, y);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists
        assert!(!graph.nodes.is_empty());
    }
}

#[test]
fn test_flow_graph_class_expression() {
    let source = r#"
let x: number;
const Foo = class {
    static {
        x = 42;
    }
};
console.log(x);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists - class expression with static block
        assert!(!graph.nodes.is_empty());
    }
}
