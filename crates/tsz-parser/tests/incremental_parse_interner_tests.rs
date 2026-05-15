//! Regression tests for incremental parser / arena interner coherence.
//!
//! `ParserState::parse_source_file_statements_from_offset` reuses the existing
//! arena and scanner. New identifiers interned by the scanner during the
//! suffix parse must remain resolvable through the arena's interner; otherwise
//! `NodeArena::resolve_identifier_text` (and any caller using atom-based
//! resolution like the binder, LSP, or diagnostic display) silently returns
//! the empty string instead of the actual identifier text.
//!
//! See `docs/plan/ROADMAP.md` Workstream 7.

use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::parser::node_view::NodeAccess;
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

/// Find every `Identifier` node currently stored in the arena and pair the
/// stored atom with the text the arena resolves for it.
fn collect_identifier_atom_text(arena: &NodeArena) -> Vec<(Atom, String, String)> {
    let mut out = Vec::new();
    for raw in 0..arena.len() {
        let idx = crate::parser::NodeIndex(u32::try_from(raw).expect("node index fits in u32"));
        let Some(node) = arena.get(idx) else { continue };
        if node.kind != SyntaxKind::Identifier as u16 {
            continue;
        }
        let Some(data) = arena.get_identifier(node) else {
            continue;
        };
        out.push((
            data.atom,
            data.escaped_text.clone(),
            arena.resolve_identifier_text(data).to_string(),
        ));
    }
    out
}

#[test]
fn incremental_parse_keeps_arena_interner_coherent_with_new_identifier() {
    // Initial parse: a single declaration with one identifier "alpha".
    let initial_source = "let alpha = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), initial_source.to_string());
    let _root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "initial source should parse cleanly, got diagnostics: {:?}",
        parser.get_diagnostics()
    );

    // Pick an identifier that does NOT appear in the initial source so the
    // scanner has to mint a brand-new atom during the incremental parse.
    let new_identifier = "uniquely_named_after_edit";
    let edited_source = format!("let alpha = 1; let {new_identifier} = 2;");

    // Incremental reparse from the position after the first statement. This
    // is the scenario the LSP triggers when the user appends new code.
    let resume_offset =
        u32::try_from(initial_source.len()).expect("initial source length should fit in u32");
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        edited_source,
        resume_offset,
    );

    // The suffix should produce exactly one new top-level statement.
    assert_eq!(
        result.statements.nodes.len(),
        1,
        "expected one new statement from incremental parse, got {}",
        result.statements.nodes.len()
    );

    // Walk every identifier and confirm that every non-`NONE` atom resolves
    // through the arena's interner to its escaped_text. Returning "" means
    // the arena's interner is stale relative to the scanner's.
    let mut found_new = false;
    for (atom, escaped_text, resolved) in collect_identifier_atom_text(&parser.arena) {
        if atom == Atom::NONE {
            continue;
        }
        assert_eq!(
            resolved, escaped_text,
            "arena interner returned `{resolved}` for atom {atom:?} but \
             the identifier's text is `{escaped_text}` — incremental parse \
             left the arena interner stale relative to the scanner"
        );
        if escaped_text == new_identifier {
            found_new = true;
        }
    }
    assert!(
        found_new,
        "did not find an identifier with text `{new_identifier}` in the \
         arena after incremental parse"
    );
}

#[test]
fn incremental_parse_resolves_new_identifier_through_get_identifier_text() {
    // Same scenario as above but exercises the public `get_identifier_text`
    // accessor used by callers like the binder and LSP.
    let initial_source = "let alpha = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), initial_source.to_string());
    let _root = parser.parse_source_file();

    let new_identifier = "freshly_appended_binding";
    let edited_source = format!("let alpha = 1; let {new_identifier} = 2;");
    let resume_offset =
        u32::try_from(initial_source.len()).expect("initial source length should fit in u32");
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        edited_source,
        resume_offset,
    );

    assert_eq!(result.statements.nodes.len(), 1);

    // Find the identifier node by walking children of the new variable
    // statement until we hit the binding name.
    let stmt = result.statements.nodes[0];
    let binding_name_idx = find_first_identifier_descendant(&parser.arena, stmt)
        .expect("expected to find the binding identifier on the new statement");

    let resolved = parser
        .arena
        .get_identifier_text(binding_name_idx)
        .expect("binding identifier should resolve through arena interner");
    assert_eq!(
        resolved, new_identifier,
        "arena.get_identifier_text returned `{resolved}` but the source \
         introduced identifier `{new_identifier}` — interner coherence broke"
    );
}

/// Snapshot pipeline serialization (`docs/plan/PERFORMANCE_PLAN.md` T3.1): the snapshot
/// pipeline serializes a parsed `NodeArena` to disk and restores it on a later run.
/// The arena's
/// internal `Interner` field used to be `#[serde(skip)]`, which silently
/// stripped identifier text on round-trip. This test pins the post-fix
/// invariant: serialise -> deserialise -> resolve must yield the same text
/// for every `Identifier` node.
#[test]
fn node_arena_round_trips_identifier_text_via_serde_json() {
    let source = "\nconst helloWorld = 1;\nfunction compute(input: string): string {\n    const intermediate = input + helloWorld;\n    return intermediate;\n}\ninterface Cache {\n    storedValue: number;\n}\n".to_string();

    let mut parser = ParserState::new("snapshot_round_trip.ts".to_string(), source);
    let _root = parser.parse_source_file();
    let arena = parser.into_arena();

    let original_idents = collect_identifier_atom_text(&arena);
    assert!(
        !original_idents.is_empty(),
        "fixture should produce at least one Identifier node"
    );

    // Round-trip through serde_json (the prototype snapshot format).
    let json = serde_json::to_string(&arena).expect("NodeArena should serialize via serde_json");
    let restored: NodeArena =
        serde_json::from_str(&json).expect("NodeArena should deserialize via serde_json");
    let restored_idents = collect_identifier_atom_text(&restored);

    assert_eq!(
        restored_idents.len(),
        original_idents.len(),
        "identifier count must match after round-trip"
    );
    for (before, after) in original_idents.iter().zip(restored_idents.iter()) {
        assert_eq!(
            before, after,
            "identifier (atom, escaped_text, resolved) must match across round-trip"
        );
    }

    // Resolving an arbitrary atom from the original through the restored
    // interner must give the same text — this is what guards against atom
    // index drift between the two interners.
    if let Some((atom, _, original_text)) = original_idents.first() {
        let restored_text = restored.interner.resolve(*atom).to_string();
        assert_eq!(&restored_text, original_text);
    }
}

fn find_first_identifier_descendant(
    arena: &NodeArena,
    root: crate::parser::NodeIndex,
) -> Option<crate::parser::NodeIndex> {
    let mut stack = vec![root];
    while let Some(idx) = stack.pop() {
        let Some(node) = arena.get(idx) else { continue };
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(idx);
        }
        // Push children in reverse so the leftmost descendant is visited first.
        let mut children = arena.get_children(idx);
        children.reverse();
        stack.extend(children);
    }
    None
}
