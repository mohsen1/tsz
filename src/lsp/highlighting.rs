//! Document Highlighting implementation for LSP.
//!
//! Provides "highlight all occurrences" functionality that shows all
//! references to the symbol at the cursor position, distinguishing
//! between reads (references) and writes (assignments).
//!
//! Also supports keyword highlighting for matching control flow keywords:
//! if/else, try/catch/finally, switch/case/default, while/do.

use crate::binder::BinderState;
use crate::lsp::position::{LineMap, Position, Range};
use crate::lsp::references::FindReferences;
use crate::lsp::utils::find_node_at_offset;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;

/// The kind of highlight - distinguishes between reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum DocumentHighlightKind {
    /// The symbol is being read (referenced).
    Read = 1,
    /// The symbol is being written (assigned to).
    Write = 2,
    /// The symbol is being read and written (text, like +=).
    Text = 3,
}

/// A document highlight (a single occurrence of the symbol).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentHighlight {
    /// The range of the symbol occurrence.
    pub range: Range,
    /// The kind of highlight (read vs write).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<DocumentHighlightKind>,
}

impl DocumentHighlight {
    /// Create a new document highlight.
    pub fn new(range: Range, kind: Option<DocumentHighlightKind>) -> Self {
        Self { range, kind }
    }

    /// Create a read highlight.
    pub fn read(range: Range) -> Self {
        Self {
            range,
            kind: Some(DocumentHighlightKind::Read),
        }
    }

    /// Create a write highlight.
    pub fn write(range: Range) -> Self {
        Self {
            range,
            kind: Some(DocumentHighlightKind::Write),
        }
    }

    /// Create a text highlight (read and write).
    pub fn text(range: Range) -> Self {
        Self {
            range,
            kind: Some(DocumentHighlightKind::Text),
        }
    }
}

/// Provider for document highlighting.
pub struct DocumentHighlightProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    source_text: &'a str,
}

impl<'a> DocumentHighlightProvider<'a> {
    /// Create a new document highlight provider.
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        source_text: &'a str,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            source_text,
        }
    }

    /// Get all highlights for the symbol at the given position.
    ///
    /// Returns a list of all occurrences of the symbol, each with a range
    /// and optionally a kind (read/write) to distinguish the access pattern.
    ///
    /// If the cursor is on a control flow keyword (if, else, try, catch, etc.),
    /// this returns matching keyword highlights instead.
    pub fn get_document_highlights(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<DocumentHighlight>> {
        // First, check if we're on a keyword that should trigger keyword highlighting
        if let Some(kw_highlights) = self.get_keyword_highlights(position) {
            if !kw_highlights.is_empty() {
                return Some(kw_highlights);
            }
        }

        // Use FindReferences to get all occurrences
        let finder = FindReferences::new(
            self.arena,
            self.binder,
            self.line_map,
            "<current>".to_string(),
            self.source_text,
        );

        let locations = finder.find_references(root, position)?;

        // Convert locations to highlights with AST-based write detection
        let highlights: Vec<DocumentHighlight> = locations
            .into_iter()
            .map(|loc| {
                let kind = self.detect_access_kind_ast(loc.range, &finder);
                DocumentHighlight::new(loc.range, kind)
            })
            .collect();

        if highlights.is_empty() {
            None
        } else {
            Some(highlights)
        }
    }

    /// Get keyword highlights for matching control flow keywords.
    ///
    /// When the cursor is on a keyword like `if`, this returns highlights
    /// for the matching `else` (and vice versa). Similarly for try/catch/finally,
    /// switch/case/default, and while/do.
    fn get_keyword_highlights(&self, position: Position) -> Option<Vec<DocumentHighlight>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }

        let node = self.arena.get(node_idx)?;
        let _kind = node.kind;

        // Check if this is a keyword token or if we're inside a statement that
        // starts with a keyword at our cursor position
        let keyword_kind = self.get_keyword_at_offset(offset);

        if keyword_kind.is_none() {
            return None;
        }

        let kw = keyword_kind.unwrap();

        match kw {
            SyntaxKind::IfKeyword | SyntaxKind::ElseKeyword => {
                self.highlight_if_else(node_idx, offset)
            }
            SyntaxKind::TryKeyword | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword => {
                self.highlight_try_catch_finally(node_idx, offset)
            }
            SyntaxKind::SwitchKeyword | SyntaxKind::CaseKeyword | SyntaxKind::DefaultKeyword => {
                self.highlight_switch_case(node_idx, offset)
            }
            SyntaxKind::WhileKeyword | SyntaxKind::DoKeyword => {
                self.highlight_while_do(node_idx, offset)
            }
            SyntaxKind::ReturnKeyword => self.highlight_return(node_idx, offset),
            SyntaxKind::BreakKeyword | SyntaxKind::ContinueKeyword => {
                self.highlight_break_continue(node_idx, offset)
            }
            _ => None,
        }
    }

    /// Determine the keyword at the given offset by checking the source text.
    fn get_keyword_at_offset(&self, offset: u32) -> Option<SyntaxKind> {
        let src = self.source_text;
        let off = offset as usize;

        // Try to read a keyword-like word starting at or around the offset
        // We look for a word boundary and then check if it's a keyword
        let start = self.find_word_start(off);
        let end = self.find_word_end(off);

        if start >= end || end > src.len() {
            return None;
        }

        let word = &src[start..end];

        match word {
            "if" => Some(SyntaxKind::IfKeyword),
            "else" => Some(SyntaxKind::ElseKeyword),
            "try" => Some(SyntaxKind::TryKeyword),
            "catch" => Some(SyntaxKind::CatchKeyword),
            "finally" => Some(SyntaxKind::FinallyKeyword),
            "switch" => Some(SyntaxKind::SwitchKeyword),
            "case" => Some(SyntaxKind::CaseKeyword),
            "default" => Some(SyntaxKind::DefaultKeyword),
            "while" => Some(SyntaxKind::WhileKeyword),
            "do" => Some(SyntaxKind::DoKeyword),
            "return" => Some(SyntaxKind::ReturnKeyword),
            "break" => Some(SyntaxKind::BreakKeyword),
            "continue" => Some(SyntaxKind::ContinueKeyword),
            _ => None,
        }
    }

    fn find_word_start(&self, offset: usize) -> usize {
        let bytes = self.source_text.as_bytes();
        let mut start = offset;
        while start > 0
            && ((bytes[start - 1] as char).is_alphanumeric() || bytes[start - 1] == b'_')
        {
            start -= 1;
        }
        start
    }

    fn find_word_end(&self, offset: usize) -> usize {
        let bytes = self.source_text.as_bytes();
        let len = bytes.len();
        let mut end = offset;
        while end < len && ((bytes[end] as char).is_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        end
    }

    /// Create a Range for a keyword of known length at the given byte offset.
    fn keyword_range(&self, offset: u32, keyword_len: u32) -> Range {
        let start = self.line_map.offset_to_position(offset, self.source_text);
        let end = self
            .line_map
            .offset_to_position(offset + keyword_len, self.source_text);
        Range::new(start, end)
    }

    /// Find the parent if-statement for a node near the given offset.
    #[allow(dead_code)]
    fn find_enclosing_if_statement(&self, _node_idx: NodeIndex, offset: u32) -> Option<NodeIndex> {
        // Walk all nodes to find the if statement that contains this offset
        // and whose keyword is at or near this offset
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::IF_STATEMENT && node.pos <= offset && node.end > offset
            {
                // Check if the "if" keyword is at the start of this node
                let keyword_start = self.skip_whitespace_forward(node.pos as usize) as u32;
                if keyword_start == offset
                    || (keyword_start <= offset && offset < keyword_start + 2)
                {
                    return Some(NodeIndex(i as u32));
                }
            }
        }
        // Fallback: find the tightest if statement containing offset
        let mut best = NodeIndex::NONE;
        let mut best_len = u32::MAX;
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::IF_STATEMENT && node.pos <= offset && node.end > offset
            {
                let len = node.end - node.pos;
                if len < best_len {
                    best_len = len;
                    best = NodeIndex(i as u32);
                }
            }
        }
        if best.is_some() { Some(best) } else { None }
    }

    /// Find the enclosing try statement for a node near the given offset.
    #[allow(dead_code)]
    fn find_enclosing_try_statement(&self, offset: u32) -> Option<NodeIndex> {
        let mut best = NodeIndex::NONE;
        let mut best_len = u32::MAX;
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::TRY_STATEMENT
                && node.pos <= offset
                && node.end > offset
            {
                let len = node.end - node.pos;
                if len < best_len {
                    best_len = len;
                    best = NodeIndex(i as u32);
                }
            }
        }
        if best.is_some() { Some(best) } else { None }
    }

    /// Find the enclosing switch statement for a node near the given offset.
    fn find_enclosing_switch_statement(&self, offset: u32) -> Option<NodeIndex> {
        let mut best = NodeIndex::NONE;
        let mut best_len = u32::MAX;
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::SWITCH_STATEMENT
                && node.pos <= offset
                && node.end > offset
            {
                let len = node.end - node.pos;
                if len < best_len {
                    best_len = len;
                    best = NodeIndex(i as u32);
                }
            }
        }
        if best.is_some() { Some(best) } else { None }
    }

    /// Skip whitespace forward from an offset and return the new offset.
    fn skip_whitespace_forward(&self, offset: usize) -> usize {
        let bytes = self.source_text.as_bytes();
        let mut i = offset;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    }

    /// Highlight if/else keyword pairs.
    ///
    /// When on `if`, highlights the matching `else` (and `else if` chains).
    /// When on `else`, highlights the matching `if`.
    fn highlight_if_else(
        &self,
        _node_idx: NodeIndex,
        offset: u32,
    ) -> Option<Vec<DocumentHighlight>> {
        // Find the if-statement that directly owns this keyword
        let if_stmt_idx = self.find_owning_if_statement(offset)?;
        let if_node = self.arena.get(if_stmt_idx)?;
        let if_data = self.arena.get_if_statement(if_node)?;

        let mut highlights = Vec::new();

        // Highlight the "if" keyword at the start of this statement
        let if_kw_offset = self.skip_whitespace_forward(if_node.pos as usize) as u32;
        highlights.push(DocumentHighlight::text(self.keyword_range(if_kw_offset, 2)));

        // If there's an else clause, highlight the "else" keyword
        if !if_data.else_statement.is_none() {
            if let Some(else_node) = self.arena.get(if_data.else_statement) {
                // The "else" keyword appears between the then statement end
                // and the else statement start
                let then_node = self.arena.get(if_data.then_statement);
                if let Some(then) = then_node {
                    let search_start = then.end as usize;
                    let search_end = else_node.end as usize;
                    if let Some(else_offset) =
                        self.find_keyword_in_range(search_start, search_end, "else")
                    {
                        highlights.push(DocumentHighlight::text(
                            self.keyword_range(else_offset as u32, 4),
                        ));
                    }
                }
            }
        }

        if highlights.len() <= 1 {
            // Only the "if" keyword, no matching else - still return it
            // but only if we're actually on the if keyword
            return if highlights.is_empty() {
                None
            } else {
                Some(highlights)
            };
        }

        Some(highlights)
    }

    /// Find the if statement that "owns" the keyword at the given offset.
    /// This handles both the `if` keyword and the `else` keyword.
    fn find_owning_if_statement(&self, offset: u32) -> Option<NodeIndex> {
        let word_start = self.find_word_start(offset as usize);
        let word_end = self.find_word_end(offset as usize);
        let word = &self.source_text[word_start..word_end];

        if word == "else" {
            // Find the if-statement whose else branch contains this offset
            // The else keyword is between the then-statement end and else-statement start
            for (i, node) in self.arena.nodes.iter().enumerate() {
                if node.kind == syntax_kind_ext::IF_STATEMENT {
                    if let Some(if_data) = self.arena.get_if_statement(node) {
                        if !if_data.else_statement.is_none() {
                            if let Some(then_node) = self.arena.get(if_data.then_statement) {
                                // Check if the "else" keyword is between then.end and else.start
                                let then_end = then_node.end as usize;
                                if let Some(else_kw_off) =
                                    self.find_keyword_in_range(then_end, node.end as usize, "else")
                                {
                                    if else_kw_off == word_start {
                                        return Some(NodeIndex(i as u32));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            return None;
        }

        if word == "if" {
            // Find the if-statement whose "if" keyword is at this position
            for (i, node) in self.arena.nodes.iter().enumerate() {
                if node.kind == syntax_kind_ext::IF_STATEMENT {
                    let kw_start = self.skip_whitespace_forward(node.pos as usize);
                    if kw_start == word_start {
                        return Some(NodeIndex(i as u32));
                    }
                }
            }
            return None;
        }

        None
    }

    /// Highlight try/catch/finally keyword groups.
    fn highlight_try_catch_finally(
        &self,
        _node_idx: NodeIndex,
        offset: u32,
    ) -> Option<Vec<DocumentHighlight>> {
        // Find the innermost try statement
        let try_idx = self.find_owning_try_statement(offset)?;
        let try_node = self.arena.get(try_idx)?;
        let try_data = self.arena.get_try(try_node)?;

        let mut highlights = Vec::new();

        // Highlight the "try" keyword
        let try_kw_offset = self.skip_whitespace_forward(try_node.pos as usize) as u32;
        highlights.push(DocumentHighlight::text(
            self.keyword_range(try_kw_offset, 3),
        ));

        // Highlight "catch" if present
        if !try_data.catch_clause.is_none() {
            if let Some(catch_node) = self.arena.get(try_data.catch_clause) {
                let catch_kw_offset = self.skip_whitespace_forward(catch_node.pos as usize) as u32;
                highlights.push(DocumentHighlight::text(
                    self.keyword_range(catch_kw_offset, 5),
                ));
            }
        }

        // Highlight "finally" if present
        if !try_data.finally_block.is_none() {
            if let Some(finally_node) = self.arena.get(try_data.finally_block) {
                // The "finally" keyword is right before the finally block
                // We need to search backward from the block start
                let search_start = if !try_data.catch_clause.is_none() {
                    if let Some(catch_node) = self.arena.get(try_data.catch_clause) {
                        catch_node.end as usize
                    } else {
                        try_data.try_block.0 as usize
                    }
                } else if let Some(try_block) = self.arena.get(try_data.try_block) {
                    try_block.end as usize
                } else {
                    try_node.pos as usize
                };

                if let Some(finally_kw_offset) =
                    self.find_keyword_in_range(search_start, finally_node.end as usize, "finally")
                {
                    highlights.push(DocumentHighlight::text(
                        self.keyword_range(finally_kw_offset as u32, 7),
                    ));
                }
            }
        }

        if highlights.is_empty() {
            None
        } else {
            Some(highlights)
        }
    }

    /// Find the try statement that owns the keyword at the given offset.
    fn find_owning_try_statement(&self, offset: u32) -> Option<NodeIndex> {
        let word_start = self.find_word_start(offset as usize);
        let word_end = self.find_word_end(offset as usize);
        let word = &self.source_text[word_start..word_end];

        // For "try" keyword, find the try statement starting at this position
        if word == "try" {
            for (i, node) in self.arena.nodes.iter().enumerate() {
                if node.kind == syntax_kind_ext::TRY_STATEMENT {
                    let kw_start = self.skip_whitespace_forward(node.pos as usize);
                    if kw_start == word_start {
                        return Some(NodeIndex(i as u32));
                    }
                }
            }
            return None;
        }

        // For "catch" keyword, find the try statement that has a catch clause at this position
        if word == "catch" {
            for (i, node) in self.arena.nodes.iter().enumerate() {
                if node.kind == syntax_kind_ext::TRY_STATEMENT {
                    if let Some(try_data) = self.arena.get_try(node) {
                        if !try_data.catch_clause.is_none() {
                            if let Some(catch_node) = self.arena.get(try_data.catch_clause) {
                                let catch_kw_start =
                                    self.skip_whitespace_forward(catch_node.pos as usize);
                                if catch_kw_start == word_start {
                                    return Some(NodeIndex(i as u32));
                                }
                            }
                        }
                    }
                }
            }
            return None;
        }

        // For "finally" keyword, find the try statement that has a finally block
        if word == "finally" {
            for (i, node) in self.arena.nodes.iter().enumerate() {
                if node.kind == syntax_kind_ext::TRY_STATEMENT {
                    if let Some(try_data) = self.arena.get_try(node) {
                        if !try_data.finally_block.is_none() {
                            // Check if the finally keyword is within this try statement
                            if node.pos <= offset && node.end > offset {
                                // Verify the finally keyword position
                                let search_start = if !try_data.catch_clause.is_none() {
                                    if let Some(catch_node) = self.arena.get(try_data.catch_clause)
                                    {
                                        catch_node.end as usize
                                    } else {
                                        node.pos as usize
                                    }
                                } else if let Some(try_block) = self.arena.get(try_data.try_block) {
                                    try_block.end as usize
                                } else {
                                    node.pos as usize
                                };
                                if let Some(finally_kw) = self.find_keyword_in_range(
                                    search_start,
                                    node.end as usize,
                                    "finally",
                                ) {
                                    if finally_kw == word_start {
                                        return Some(NodeIndex(i as u32));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            return None;
        }

        None
    }

    /// Highlight switch/case/default keyword groups.
    fn highlight_switch_case(
        &self,
        _node_idx: NodeIndex,
        offset: u32,
    ) -> Option<Vec<DocumentHighlight>> {
        let switch_idx = self.find_owning_switch_statement(offset)?;
        let switch_node = self.arena.get(switch_idx)?;
        let switch_data = self.arena.get_switch(switch_node)?;

        let mut highlights = Vec::new();

        // Highlight the "switch" keyword
        let switch_kw_offset = self.skip_whitespace_forward(switch_node.pos as usize) as u32;
        highlights.push(DocumentHighlight::text(
            self.keyword_range(switch_kw_offset, 6),
        ));

        // Highlight all case/default clauses in the case block
        if let Some(case_block_node) = self.arena.get(switch_data.case_block) {
            if let Some(block_data) = self.arena.get_block(case_block_node) {
                for &clause_idx in &block_data.statements.nodes {
                    if let Some(clause_node) = self.arena.get(clause_idx) {
                        let kw_offset =
                            self.skip_whitespace_forward(clause_node.pos as usize) as u32;
                        if clause_node.kind == syntax_kind_ext::CASE_CLAUSE {
                            highlights
                                .push(DocumentHighlight::text(self.keyword_range(kw_offset, 4)));
                        } else if clause_node.kind == syntax_kind_ext::DEFAULT_CLAUSE {
                            highlights
                                .push(DocumentHighlight::text(self.keyword_range(kw_offset, 7)));
                        }
                    }
                }
            }
        }

        if highlights.is_empty() {
            None
        } else {
            Some(highlights)
        }
    }

    /// Find the switch statement that owns the keyword at the given offset.
    fn find_owning_switch_statement(&self, offset: u32) -> Option<NodeIndex> {
        let word_start = self.find_word_start(offset as usize);
        let word_end = self.find_word_end(offset as usize);
        let word = &self.source_text[word_start..word_end];

        if word == "switch" {
            for (i, node) in self.arena.nodes.iter().enumerate() {
                if node.kind == syntax_kind_ext::SWITCH_STATEMENT {
                    let kw_start = self.skip_whitespace_forward(node.pos as usize);
                    if kw_start == word_start {
                        return Some(NodeIndex(i as u32));
                    }
                }
            }
            return None;
        }

        // For case/default, find the enclosing switch statement
        if word == "case" || word == "default" {
            // Find the case/default clause at this offset
            for node in self.arena.nodes.iter() {
                if (node.kind == syntax_kind_ext::CASE_CLAUSE
                    || node.kind == syntax_kind_ext::DEFAULT_CLAUSE)
                    && node.pos <= offset
                    && node.end > offset
                {
                    let kw_start = self.skip_whitespace_forward(node.pos as usize);
                    if kw_start == word_start {
                        // Now find the parent switch statement
                        return self.find_enclosing_switch_statement(offset);
                    }
                }
            }
            return None;
        }

        None
    }

    /// Highlight while/do keyword pairs.
    fn highlight_while_do(
        &self,
        _node_idx: NodeIndex,
        offset: u32,
    ) -> Option<Vec<DocumentHighlight>> {
        let word_start = self.find_word_start(offset as usize);
        let word_end = self.find_word_end(offset as usize);
        let word = &self.source_text[word_start..word_end];

        if word == "while" {
            // Check if this is a do-while's "while" or a standalone while
            // For do-while, the "while" comes after the do-block
            if let Some(do_stmt_idx) = self.find_do_while_for_while_keyword(word_start) {
                // This is the "while" of a do-while loop
                let do_node = self.arena.get(do_stmt_idx)?;
                let mut highlights = Vec::new();

                let do_kw_offset = self.skip_whitespace_forward(do_node.pos as usize) as u32;
                highlights.push(DocumentHighlight::text(self.keyword_range(do_kw_offset, 2)));
                highlights.push(DocumentHighlight::text(
                    self.keyword_range(word_start as u32, 5),
                ));

                return Some(highlights);
            }

            // Standalone while loop - just highlight the "while" keyword
            let mut highlights = Vec::new();
            highlights.push(DocumentHighlight::text(
                self.keyword_range(word_start as u32, 5),
            ));
            return Some(highlights);
        }

        if word == "do" {
            // Find the do-while statement
            for node in self.arena.nodes.iter() {
                if node.kind == syntax_kind_ext::DO_STATEMENT {
                    let kw_start = self.skip_whitespace_forward(node.pos as usize);
                    if kw_start == word_start {
                        let mut highlights = Vec::new();
                        highlights.push(DocumentHighlight::text(
                            self.keyword_range(word_start as u32, 2),
                        ));

                        // Find the matching "while" keyword
                        if let Some(loop_data) = self.arena.get_loop(node) {
                            if let Some(stmt_node) = self.arena.get(loop_data.statement) {
                                if let Some(while_kw) = self.find_keyword_in_range(
                                    stmt_node.end as usize,
                                    node.end as usize,
                                    "while",
                                ) {
                                    highlights.push(DocumentHighlight::text(
                                        self.keyword_range(while_kw as u32, 5),
                                    ));
                                }
                            }
                        }

                        return Some(highlights);
                    }
                }
            }
            return None;
        }

        None
    }

    /// Find a do-while statement whose "while" keyword is at the given position.
    fn find_do_while_for_while_keyword(&self, while_kw_start: usize) -> Option<NodeIndex> {
        for (i, node) in self.arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::DO_STATEMENT {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    if let Some(stmt_node) = self.arena.get(loop_data.statement) {
                        if let Some(while_kw) = self.find_keyword_in_range(
                            stmt_node.end as usize,
                            node.end as usize,
                            "while",
                        ) {
                            if while_kw == while_kw_start {
                                return Some(NodeIndex(i as u32));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Highlight return keyword.
    fn highlight_return(
        &self,
        _node_idx: NodeIndex,
        offset: u32,
    ) -> Option<Vec<DocumentHighlight>> {
        let word_start = self.find_word_start(offset as usize);
        let mut highlights = Vec::new();
        highlights.push(DocumentHighlight::text(
            self.keyword_range(word_start as u32, 6),
        ));
        Some(highlights)
    }

    /// Highlight break/continue keywords.
    fn highlight_break_continue(
        &self,
        _node_idx: NodeIndex,
        offset: u32,
    ) -> Option<Vec<DocumentHighlight>> {
        let word_start = self.find_word_start(offset as usize);
        let word_end = self.find_word_end(offset as usize);
        let word = &self.source_text[word_start..word_end];

        let kw_len = word.len() as u32;
        let mut highlights = Vec::new();
        highlights.push(DocumentHighlight::text(
            self.keyword_range(word_start as u32, kw_len),
        ));
        Some(highlights)
    }

    /// Find a keyword string within a byte range of the source text.
    fn find_keyword_in_range(&self, start: usize, end: usize, keyword: &str) -> Option<usize> {
        let src = self.source_text;
        let search_area = src.get(start..end)?;
        let kw_len = keyword.len();

        // Find the keyword, making sure it's at a word boundary
        let mut search_from = 0;
        while search_from < search_area.len() {
            if let Some(pos) = search_area[search_from..].find(keyword) {
                let abs_pos = start + search_from + pos;
                let rel_end = search_from + pos + kw_len;

                // Check word boundaries
                let at_word_start = search_from + pos == 0
                    || !src
                        .as_bytes()
                        .get(abs_pos - 1)
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
                let at_word_end = rel_end >= search_area.len()
                    || !search_area
                        .as_bytes()
                        .get(rel_end)
                        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');

                if at_word_start && at_word_end {
                    return Some(abs_pos);
                }

                search_from += pos + 1;
            } else {
                break;
            }
        }

        None
    }

    /// Detect read/write access using AST-based analysis, falling back to text heuristics.
    ///
    /// This method first tries to find the AST node at the reference location and
    /// uses the `is_write_access_node` method from FindReferences for accurate
    /// detection. If the AST lookup fails, it falls back to text-based heuristics.
    fn detect_access_kind_ast(
        &self,
        range: Range,
        finder: &FindReferences,
    ) -> Option<DocumentHighlightKind> {
        // Try AST-based detection first
        if let Some(start_offset) = self
            .line_map
            .position_to_offset(range.start, self.source_text)
        {
            let node_idx = find_node_at_offset(self.arena, start_offset);
            if node_idx.is_some() {
                let is_write = finder.is_write_access_node(node_idx);
                return if is_write {
                    Some(DocumentHighlightKind::Write)
                } else {
                    Some(DocumentHighlightKind::Read)
                };
            }
        }

        // Fallback to text-based heuristic
        self.detect_access_kind(range)
    }

    /// Detect whether a reference is a read or write (fallback text-based heuristic).
    ///
    /// This is used as a fallback when AST-based detection is not available.
    fn detect_access_kind(&self, range: Range) -> Option<DocumentHighlightKind> {
        let start_offset = self
            .line_map
            .position_to_offset(range.start, self.source_text)?;
        let end_offset = self
            .line_map
            .position_to_offset(range.end, self.source_text)?;

        // Look at a small window before the identifier to detect assignment
        let context_start = start_offset.saturating_sub(20);
        let context_end = if end_offset + 20 < self.source_text.len() as u32 {
            end_offset + 20
        } else {
            self.source_text.len() as u32
        };

        let context = &self.source_text[context_start as usize..context_end as usize];

        // Check for assignment patterns before the identifier
        let before = context
            .get(..(start_offset - context_start) as usize)
            .unwrap_or("");
        let after = context
            .get((end_offset - context_start) as usize..)
            .unwrap_or("");

        // Check if this is a write (assignment)
        let is_write = self.is_write_context(before, after);

        // Check if this is a compound assignment (read and write)
        let is_text = self.is_compound_assignment(before);

        if is_text {
            Some(DocumentHighlightKind::Text)
        } else if is_write {
            Some(DocumentHighlightKind::Write)
        } else {
            Some(DocumentHighlightKind::Read)
        }
    }

    /// Check if the identifier is in a write context (assignment).
    fn is_write_context(&self, before: &str, after: &str) -> bool {
        let before_trimmed = before.trim();

        // Check for assignment operators (=, :=, etc.)
        // But exclude comparison operators (==, ===, !=, !==) and arrow (=>)
        // and generic defaults (<T = Default>).
        if before_trimmed.ends_with('=')
            && !before_trimmed.ends_with("==")
            && !before_trimmed.ends_with("!=")
            && !before_trimmed.ends_with("=>")
            && !before_trimmed.ends_with("<=")
        {
            return true;
        }

        // Check for named compound/colon assignment operators
        if before_trimmed.ends_with(":=")
            || before_trimmed.ends_with("+=")
            || before_trimmed.ends_with("-=")
            || before_trimmed.ends_with("*=")
            || before_trimmed.ends_with("/=")
            || before_trimmed.ends_with("%=")
            || before_trimmed.ends_with("&=")
            || before_trimmed.ends_with("|=")
            || before_trimmed.ends_with("^=")
            || before_trimmed.ends_with("<<=")
            || before_trimmed.ends_with(">>=")
            || before_trimmed.ends_with(">>>=")
        {
            return true;
        }

        // Check for variable declaration keywords (var, let, const)
        let before_trimmed_lower = before_trimmed.to_lowercase();
        let words: Vec<&str> = before_trimmed_lower.split_whitespace().collect();
        if !words.is_empty() {
            let last_word = words.last().unwrap();
            if *last_word == "var"
                || *last_word == "let"
                || *last_word == "const"
                || *last_word == "function"
                || *last_word == "class"
                || *last_word == "interface"
                || *last_word == "type"
                || *last_word == "enum"
                || *last_word == "import"
                || *last_word == "catch"
            {
                return true;
            }
        }

        // Check for for-in / for-of loop variables
        if before_trimmed.ends_with('(') {
            let prefix = before_trimmed.trim_end_matches('(').trim_end();
            if prefix.ends_with("for") {
                return true;
            }
        }

        // Check for catch clause: `catch (`
        if before_trimmed.ends_with('(') {
            let prefix = before_trimmed.trim_end_matches('(').trim_end();
            if prefix.ends_with("catch") {
                return true;
            }
        }

        // Check for object/array literal property
        if before_trimmed.ends_with('{')
            || before_trimmed.ends_with('[')
            || before_trimmed.ends_with(',')
        {
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with(':') || after_trimmed.starts_with('?') {
                return true;
            }
        }

        // Check for destructuring assignment pattern
        if before_trimmed.ends_with('{') || (before_trimmed.ends_with(',') && after.contains('}')) {
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with('}')
                || after_trimmed.starts_with(',')
                || after_trimmed.contains("} =")
            {
                return true;
            }
        }
        if before_trimmed.ends_with('[') || (before_trimmed.ends_with(',') && after.contains(']')) {
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with(']')
                || after_trimmed.starts_with(',')
                || after_trimmed.contains("] =")
            {
                return true;
            }
        }

        false
    }

    /// Check if this is a compound assignment (+=, -=, etc.).
    fn is_compound_assignment(&self, before: &str) -> bool {
        let before_trimmed = before.trim_end();
        before_trimmed.ends_with("+=")
            || before_trimmed.ends_with("-=")
            || before_trimmed.ends_with("*=")
            || before_trimmed.ends_with("/=")
            || before_trimmed.ends_with("%=")
            || before_trimmed.ends_with("&=")
            || before_trimmed.ends_with("|=")
            || before_trimmed.ends_with("^=")
            || before_trimmed.ends_with("<<=")
            || before_trimmed.ends_with(">>=")
            || before_trimmed.ends_with(">>>=")
    }
}

#[cfg(test)]
mod highlighting_tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::lsp::position::LineMap;
    use crate::parser::ParserState;

    /// Helper to create a provider from source text.
    fn make_provider(source: &str) -> (ParserState, BinderState, NodeIndex) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        {
            let arena = parser.get_arena();
            binder.bind_source_file(arena, root);
        }
        (parser, binder, root)
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_document_highlight_simple_variable() {
        let source = "let x = 1;\nlet y = x + 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'x' at position (0, 4) - the declaration
        let pos = Position::new(0, 4);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some(), "Should find highlights for 'x'");
        let highlights = highlights.unwrap();

        // Should have at least 2 occurrences: declaration and usage
        assert!(highlights.len() >= 2, "Should have at least 2 highlights");

        // All highlights should have a kind assigned
        assert!(highlights.iter().all(|h| h.kind.is_some()));
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_document_highlight_function() {
        let source = "function foo() {\n  return 1;\n}\nfoo();\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'foo' at the call site (3, 0)
        let pos = Position::new(3, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();

        // Should have at least 2 occurrences: declaration and call
        assert!(highlights.len() >= 2, "Should have at least 2 highlights");

        // All highlights should have a kind assigned
        assert!(highlights.iter().all(|h| h.kind.is_some()));
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_document_highlight_compound_assignment() {
        let source = "let count = 0;\ncount += 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'count' at the compound assignment
        let pos = Position::new(1, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();

        // Should have at least 2 occurrences
        assert!(highlights.len() >= 2, "Should have at least 2 highlights");

        // All highlights should have a kind assigned
        assert!(highlights.iter().all(|h| h.kind.is_some()));
    }

    #[test]
    fn test_document_highlight_no_symbol() {
        let source = "let x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Position on the number literal '1', not an identifier
        let pos = Position::new(0, 8);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_none(), "Should not highlight non-identifier");
    }

    #[test]
    fn test_document_highlight_read_kind() {
        let source = "let x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Test that we get highlights
        let pos = Position::new(0, 4);
        let highlights = provider.get_document_highlights(root, pos);
        assert!(highlights.is_some());
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_document_highlight_structs() {
        let source = "let x = 1;\nconsole.log(x);\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(0, 4);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();
        assert!(highlights.len() >= 2);
    }

    /// Standalone test helper that calls `is_write_context` on a real provider.
    fn test_is_write(source: &str, before: &str, after: &str) -> bool {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
        provider.is_write_context(before, after)
    }

    fn test_is_compound(source: &str, before: &str) -> bool {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();
        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);
        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);
        provider.is_compound_assignment(before)
    }

    // ---- Tests for Bug 1 & Bug 2 fixes: duplicate conditions ----

    #[test]
    fn test_write_context_simple_assignment() {
        let src = "let x = 1;";
        assert!(test_is_write(src, "x = ", "1;"));
    }

    #[test]
    fn test_write_context_var_declaration() {
        let src = "var x = 1;";
        assert!(test_is_write(src, "var ", "= 1;"));
    }

    #[test]
    fn test_write_context_let_declaration() {
        let src = "let x = 1;";
        assert!(test_is_write(src, "let ", "= 1;"));
    }

    #[test]
    fn test_write_context_const_declaration() {
        let src = "const x = 1;";
        assert!(test_is_write(src, "const ", "= 1;"));
    }

    // ---- Tests for false positive fixes (===, !==, =>) ----

    #[test]
    fn test_triple_equals_is_not_write() {
        let src = "if (x === y) {}";
        assert!(
            !test_is_write(src, "x === ", ") {}"),
            "=== should NOT be detected as a write"
        );
    }

    #[test]
    fn test_double_equals_is_not_write() {
        let src = "if (x == y) {}";
        assert!(
            !test_is_write(src, "x == ", ") {}"),
            "== should NOT be detected as a write"
        );
    }

    #[test]
    fn test_not_equals_is_not_write() {
        let src = "if (x !== y) {}";
        assert!(
            !test_is_write(src, "x !== ", ") {}"),
            "!== should NOT be detected as a write"
        );
    }

    #[test]
    fn test_not_double_equals_is_not_write() {
        let src = "if (x != y) {}";
        assert!(
            !test_is_write(src, "x != ", ") {}"),
            "!= should NOT be detected as a write"
        );
    }

    #[test]
    fn test_arrow_is_not_write() {
        let src = "const f = (x) => x + 1;";
        assert!(
            !test_is_write(src, "(x) => ", "+ 1;"),
            "=> should NOT be detected as assignment"
        );
    }

    #[test]
    fn test_less_than_equals_is_not_write() {
        let src = "if (x <= y) {}";
        assert!(
            !test_is_write(src, "x <= ", ") {}"),
            "<= should NOT be detected as a write"
        );
    }

    // ---- Tests for new keyword detection: import, catch ----

    #[test]
    fn test_import_is_write() {
        let src = "import { x } from 'mod';";
        assert!(
            test_is_write(src, "import ", "} from 'mod';"),
            "import specifier should be a write"
        );
    }

    #[test]
    fn test_catch_is_write() {
        let src = "try {} catch (e) {}";
        assert!(
            test_is_write(src, "catch ", ") {}"),
            "catch clause variable should be a write"
        );
    }

    // ---- Tests for for-loop detection ----

    #[test]
    fn test_for_loop_variable_is_write() {
        let src = "let items = []; for (let x of items) {}";
        assert!(
            test_is_write(src, "for (", " of items) {}"),
            "for-of loop variable should be a write"
        );
    }

    #[test]
    fn test_catch_paren_is_write() {
        let src = "try {} catch (e) {}";
        assert!(
            test_is_write(src, "catch (", ") {}"),
            "catch( variable should be a write"
        );
    }

    // ---- Tests for object destructuring (Bug 2 fix) ----

    #[test]
    fn test_object_destructuring_property_with_colon() {
        let src = "const { a: b } = obj;";
        assert!(
            test_is_write(src, "{ ", ": b } = obj;"),
            "Object destructuring property should be a write"
        );
    }

    #[test]
    fn test_array_destructuring_first_element() {
        let src = "const [a, b] = arr;";
        assert!(
            test_is_write(src, "[", ", b] = arr;"),
            "Array destructuring element should be a write"
        );
    }

    #[test]
    fn test_array_destructuring_bracket() {
        let src = "const [a] = arr;";
        assert!(
            test_is_write(src, "[", "] = arr;"),
            "Array destructuring single element should be a write"
        );
    }

    // ---- Tests for compound assignment detection ----

    #[test]
    fn test_compound_plus_equals() {
        let src = "x += 1;";
        assert!(test_is_compound(src, "x +="));
    }

    #[test]
    fn test_compound_minus_equals() {
        let src = "x -= 1;";
        assert!(test_is_compound(src, "x -="));
    }

    #[test]
    fn test_not_compound_for_simple_equals() {
        let src = "x = 1;";
        assert!(!test_is_compound(src, "x ="));
    }

    // ---- Test that function keyword is still detected ----

    #[test]
    fn test_function_declaration_is_write() {
        let src = "function foo() {}";
        assert!(test_is_write(src, "function ", "() {}"));
    }

    #[test]
    fn test_class_declaration_is_write() {
        let src = "class Foo {}";
        assert!(test_is_write(src, "class ", "{}"));
    }

    #[test]
    fn test_enum_declaration_is_write() {
        let src = "enum Color {}";
        assert!(test_is_write(src, "enum ", "{}"));
    }

    // ---- Test that plain reads are not writes ----

    #[test]
    fn test_plain_read_is_not_write() {
        let src = "console.log(x);";
        assert!(
            !test_is_write(src, "console.log(", ");"),
            "A plain read reference should not be a write"
        );
    }

    #[test]
    fn test_addition_is_not_write() {
        let src = "let z = x + y;";
        assert!(
            !test_is_write(src, "x + ", ";"),
            "Addition operand should not be a write"
        );
    }

    // ---- NEW TESTS: AST-based write detection ----

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_highlight_write_access_via_ast() {
        // Test that variable declarations are detected as writes via the AST path
        let source = "let x = 1;\nx = 2;\nconsole.log(x);\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(0, 4); // 'x' in declaration
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some(), "Should find highlights");
        let highlights = highlights.unwrap();

        // Should have 3 occurrences: declaration, assignment, read
        assert!(
            highlights.len() >= 3,
            "Should have at least 3 highlights, got {}",
            highlights.len()
        );

        // Check that we have both write and read kinds
        let has_write = highlights
            .iter()
            .any(|h| h.kind == Some(DocumentHighlightKind::Write));
        let has_read = highlights
            .iter()
            .any(|h| h.kind == Some(DocumentHighlightKind::Read));
        assert!(has_write, "Should have at least one write highlight");
        assert!(has_read, "Should have at least one read highlight");
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_highlight_function_declaration_is_write() {
        // Function name should be marked as write at declaration
        let source = "function greet() {}\ngreet();\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(0, 9); // 'greet' in function declaration
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();
        assert!(highlights.len() >= 2, "Should have at least 2 highlights");

        // First occurrence (declaration) should be write, second (call) should be read
        let has_write = highlights
            .iter()
            .any(|h| h.kind == Some(DocumentHighlightKind::Write));
        let has_read = highlights
            .iter()
            .any(|h| h.kind == Some(DocumentHighlightKind::Read));
        assert!(has_write, "Declaration should be a write");
        assert!(has_read, "Call should be a read");
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_highlight_parameter_is_write() {
        // Function parameter should be marked as write at declaration
        let source = "function add(a: number, b: number) {\n  return a + b;\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(0, 13); // 'a' in parameter
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find highlights for parameter 'a'"
        );
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 2,
            "Should have at least 2 highlights (param + usage)"
        );

        let has_write = highlights
            .iter()
            .any(|h| h.kind == Some(DocumentHighlightKind::Write));
        assert!(has_write, "Parameter declaration should be a write");
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_highlight_multiple_reads() {
        // Variable used multiple times should have multiple read highlights
        let source = "let val = 10;\nlet a = val;\nlet b = val;\nlet c = val;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(0, 4); // 'val' in declaration
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 4,
            "Should have at least 4 highlights (1 write + 3 reads), got {}",
            highlights.len()
        );

        let write_count = highlights
            .iter()
            .filter(|h| h.kind == Some(DocumentHighlightKind::Write))
            .count();
        let read_count = highlights
            .iter()
            .filter(|h| h.kind == Some(DocumentHighlightKind::Read))
            .count();
        assert!(write_count >= 1, "Should have at least 1 write");
        assert!(
            read_count >= 3,
            "Should have at least 3 reads, got {}",
            read_count
        );
    }

    // ---- NEW TESTS: Keyword highlighting ----

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_highlight_if_keyword() {
        let source = "if (true) {\n  console.log('yes');\n} else {\n  console.log('no');\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'if' keyword at (0, 0)
        let pos = Position::new(0, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find keyword highlights for 'if'"
        );
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 2,
            "Should highlight both 'if' and 'else', got {}",
            highlights.len()
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_highlight_else_keyword() {
        let source = "if (true) {\n  console.log('yes');\n} else {\n  console.log('no');\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'else' keyword at line 2
        let pos = Position::new(2, 2);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find keyword highlights for 'else'"
        );
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 2,
            "Should highlight both 'if' and 'else', got {}",
            highlights.len()
        );
    }

    #[test]
    fn test_highlight_try_catch_finally_keywords() {
        let source = "try {\n  foo();\n} catch (e) {\n  bar();\n} finally {\n  baz();\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'try' keyword at (0, 0)
        let pos = Position::new(0, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find keyword highlights for 'try'"
        );
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 2,
            "Should highlight try/catch/finally keywords, got {}",
            highlights.len()
        );
    }

    #[test]
    fn test_highlight_catch_keyword() {
        let source = "try {\n  foo();\n} catch (e) {\n  bar();\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'catch' keyword at line 2
        let pos = Position::new(2, 2);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find keyword highlights for 'catch'"
        );
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 2,
            "Should highlight try and catch, got {}",
            highlights.len()
        );
    }

    #[test]
    fn test_highlight_switch_case_default_keywords() {
        let source = "switch (x) {\n  case 1:\n    break;\n  case 2:\n    break;\n  default:\n    break;\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'switch' keyword at (0, 0)
        let pos = Position::new(0, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find keyword highlights for 'switch'"
        );
        let highlights = highlights.unwrap();
        // Should highlight: switch, case, case, default = 4
        assert!(
            highlights.len() >= 4,
            "Should highlight switch + all case/default, got {}",
            highlights.len()
        );
    }

    #[test]
    fn test_highlight_while_keyword() {
        let source = "while (true) {\n  break;\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'while' keyword at (0, 0)
        let pos = Position::new(0, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find keyword highlights for 'while'"
        );
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 1,
            "Should have at least 1 highlight for 'while'"
        );
    }

    #[test]
    #[ignore] // TODO: Fix this test
    fn test_highlight_do_while_keywords() {
        let source = "do {\n  foo();\n} while (true);\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Highlight 'do' keyword at (0, 0)
        let pos = Position::new(0, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find keyword highlights for 'do'"
        );
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 2,
            "Should highlight both 'do' and 'while', got {}",
            highlights.len()
        );
    }

    // ---- NEW TESTS: keyword highlighting edge cases ----

    #[test]
    fn test_highlight_if_without_else() {
        // An if without else should still highlight the "if" keyword
        let source = "if (true) {\n  foo();\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(0, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some(), "Should find highlight for lone 'if'");
        let highlights = highlights.unwrap();
        assert_eq!(
            highlights.len(),
            1,
            "Should have exactly 1 highlight for 'if' without else"
        );
    }

    #[test]
    fn test_highlight_try_without_finally() {
        // try/catch without finally
        let source = "try {\n  foo();\n} catch (e) {\n  bar();\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(0, 0);
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some());
        let highlights = highlights.unwrap();
        assert_eq!(
            highlights.len(),
            2,
            "Should highlight 'try' and 'catch', got {}",
            highlights.len()
        );
    }

    #[test]
    fn test_highlight_return_keyword() {
        let source = "function f() {\n  return 1;\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(1, 2); // 'return' keyword
        let highlights = provider.get_document_highlights(root, pos);

        assert!(highlights.is_some(), "Should find highlight for 'return'");
        let highlights = highlights.unwrap();
        assert!(
            highlights.len() >= 1,
            "Should have at least 1 highlight for 'return'"
        );
    }

    #[test]
    fn test_highlight_case_from_case_keyword() {
        // When on a "case" keyword, should highlight all cases + switch
        let source = "switch (x) {\n  case 1:\n    break;\n  default:\n    break;\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        let pos = Position::new(1, 2); // 'case' keyword
        let highlights = provider.get_document_highlights(root, pos);

        assert!(
            highlights.is_some(),
            "Should find keyword highlights for 'case'"
        );
        let highlights = highlights.unwrap();
        // Should highlight: switch, case, default = 3
        assert!(
            highlights.len() >= 3,
            "Should highlight switch + case + default, got {}",
            highlights.len()
        );
    }

    #[test]
    fn test_debug_if_statement_positions() {
        let source = "if (true) {\n  console.log(\'yes\');\n} else {\n  console.log(\'no\');\n}\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let _root = parser.parse_source_file();
        let arena = parser.get_arena();

        let mut binder = BinderState::new();
        binder.bind_source_file(arena, _root);

        let line_map = LineMap::build(source);
        let provider = DocumentHighlightProvider::new(arena, &binder, &line_map, source);

        // Print all IF_STATEMENT nodes and their positions
        for (i, node) in arena.nodes.iter().enumerate() {
            if node.kind == syntax_kind_ext::IF_STATEMENT {
                eprintln!(
                    "IF_STATEMENT at index {}: pos={}, end={}",
                    i, node.pos, node.end
                );
                let kw_start = provider.skip_whitespace_forward(node.pos as usize);
                eprintln!("  skip_whitespace_forward(pos={})={}", node.pos, kw_start);
                eprintln!(
                    "  text at kw_start: '{}'",
                    &source[kw_start..kw_start.min(source.len()) + 10.min(source.len() - kw_start)]
                );
                if let Some(if_data) = arena.get_if_statement(node) {
                    if let Some(then_node) = arena.get(if_data.then_statement) {
                        eprintln!("  then: pos={}, end={}", then_node.pos, then_node.end);
                    }
                    if !if_data.else_statement.is_none() {
                        if let Some(else_node) = arena.get(if_data.else_statement) {
                            eprintln!(
                                "  else_statement: pos={}, end={}, kind={}",
                                else_node.pos, else_node.end, else_node.kind
                            );
                            if let Some(then_node) = arena.get(if_data.then_statement) {
                                let search_start = then_node.end as usize;
                                let search_end = else_node.end as usize;
                                let search_text =
                                    &source[search_start..search_end.min(source.len())];
                                eprintln!(
                                    "  search range: {}..{}, text: '{}'",
                                    search_start, search_end, search_text
                                );
                                // Try to find "else" in this range
                                if let Some(else_pos) =
                                    provider.find_keyword_in_range(search_start, search_end, "else")
                                {
                                    eprintln!("  FOUND 'else' at offset {}", else_pos);
                                } else {
                                    eprintln!("  DID NOT find 'else' in range");
                                }
                            }
                        }
                    }
                }
            }
        }

        // Also test find_owning_if_statement
        eprintln!(
            "\nfind_owning_if_statement(0)={:?}",
            provider.find_owning_if_statement(0)
        );
    }
}
