use crate::lexer::{Lexer, Token, TokenKind, Span};
use crate::ast::{Arena, ThinNode, NodeData, BinaryOp, UnaryOp};
use crate::error::ParseError;

/// The Parser struct transforms a sequence of Tokens into an Abstract Syntax Tree (AST).
/// 
/// # Memory Management
/// The parser owns an `Arena<NodeData>`. All AST nodes are allocated in this arena.
/// The parser returns `ThinNode` handles, which are opaque wrappers around `usize` indices.
pub struct Parser<'input> {
    lexer: Lexer<'input>,
    /// Storage for all AST nodes. We return `ThinNode` handles (indices) into this arena.
    arena: Arena<NodeData>,
    /// A buffer for the current and peeked tokens to facilitate LL(1) look-ahead.
    current_token: Option<Token<'input>>,
    peek_token: Option<Token<'input>>,
}

impl<'input> Parser<'input> {
    /// Creates a new parser for the given input string.
    pub fn new(input: &'input str) -> Self {
        let mut lexer = Lexer::new(input);
        
        // Initial population of the token buffer.
        // We assume the Lexer now yields zero-copy `Span`s internally, 
        // represented here by the `Token` struct holding `&str`.
        let current_token = lexer.next_token();
        let peek_token = lexer.next_token();

        Parser {
            lexer,
            arena: Arena::new(),
            current_token,
            peek_token,
        }
    }

    /// Returns the underlying Arena, consuming the parser.
    /// This allows the caller to take ownership of the AST data.
    pub fn into_arena(self) -> Arena<NodeData> {
        self.arena
    }

    // --- Token Buffer Management ---

    /// Advances the lexer, filling the token buffer.
    /// This replaces the old `alloc` token consumption logic.
    fn advance(&mut self) {
        self.current_token = self.peek_token.take();
        self.peek_token = self.lexer.next_token();
    }

    /// Returns the kind of the current token.
    fn current_kind(&self) -> Option<TokenKind> {
        self.current_token.as_ref().map(|t| t.kind)
    }

    /// Checks if the current token matches a specific kind.
    fn check(&self, kind: TokenKind) -> bool {
        self.current_kind() == Some(kind)
    }

    /// Consumes the current token if it matches the kind, returning its Span.
    /// Returns a ParseError if the token does not match.
    fn consume(&mut self, kind: TokenKind) -> Result<Span<'input>, ParseError> {
        if self.check(kind) {
            let span = self.current_token.as_ref().unwrap().span;
            self.advance();
            Ok(span)
        } else {
            Err(ParseError::UnexpectedToken {
                expected: kind,
                found: self.current_kind(),
                span: self.current_token.as_ref().map(|t| t.span),
            })
        }
    }

    // --- AST Construction Helper ---

    /// Allocates a new Node into the arena and returns a ThinNode handle.
    /// This replaces direct `Node` struct instantiation.
    fn alloc(&mut self, data: NodeData) -> ThinNode {
        self.arena.alloc(data)
    }

    // --- Public Entry Point ---

    /// Parses the entire input into an AST node.
    pub fn parse(&mut self) -> Result<ThinNode, ParseError> {
        self.parse_expr()
    }

    // --- Recursive Descent Logic ---

    /// expr ::= term ((PLUS | MINUS) term)*
    fn parse_expr(&mut self) -> Result<ThinNode, ParseError> {
        let mut left = self.parse_term()?;

        while let Some(kind) = self.current_kind() {
            match kind {
                TokenKind::Plus | TokenKind::Minus => {
                    let op_token = self.current_token.as_ref().unwrap();
                    let op_span = op_token.span;
                    let op = match kind {
                        TokenKind::Plus => BinaryOp::Add,
                        TokenKind::Minus => BinaryOp::Sub,
                        _ => unreachable!(),
                    };

                    self.advance(); // consume operator
                    
                    let right = self.parse_term()?;

                    // Allocate a new BinaryOp node in the arena
                    left = self.alloc(NodeData::BinaryOp {
                        lhs: left,
                        op,
                        rhs: right,
                        span: op_span, // Store the span of the operator for error reporting
                    });
                }
                _ => break,
            }
        }

        Ok(left)
    }

    /// term ::= factor ((MUL | DIV) factor)*
    fn parse_term(&mut self) -> Result<ThinNode, ParseError> {
        let mut left = self.parse_factor()?;

        while let Some(kind) = self.current_kind() {
            match kind {
                TokenKind::Star | TokenKind::Slash => {
                    let op_token = self.current_token.as_ref().unwrap();
                    let op_span = op_token.span;
                    let op = match kind {
                        TokenKind::Star => BinaryOp::Mul,
                        TokenKind::Slash => BinaryOp::Div,
                        _ => unreachable!(),
                    };

                    self.advance();
                    
                    let right = self.parse_factor()?;

                    left = self.alloc(NodeData::BinaryOp {
                        lhs: left,
                        op,
                        rhs: right,
                        span: op_span,
                    });
                }
                _ => break,
            }
        }

        Ok(left)
    }

    /// factor ::= NUMBER | LPAREN expr RPAREN
    fn parse_factor(&mut self) -> Result<ThinNode, ParseError> {
        match self.current_kind() {
            Some(TokenKind::Number) => {
                let token = self.current_token.as_ref().unwrap();
                let span = token.span;
                let text = span.as_str(); // Zero-copy access
                
                // Parse the string slice to a primitive type
                let value = text.parse::<i64>().map_err(|_| ParseError::InvalidNumber {
                    text: text.to_string(), // Allocates here only for error reporting
                    span,
                })?;

                self.advance();

                Ok(self.alloc(NodeData::Number { value, span }))
            }
            Some(TokenKind::LeftParen) => {
                self.consume(TokenKind::LeftParen)?;
                let expr = self.parse_expr()?;
                self.consume(TokenKind::RightParen)?;
                Ok(expr)
            }
            Some(kind) => Err(ParseError::UnexpectedToken {
                expected: TokenKind::Number,
                found: Some(kind),
                span: self.current_token.as_ref().map(|t| t.span),
            }),
            None => Err(ParseError::UnexpectedEndOfInput),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_arithmetic() {
        let input = "1 + 2 * 3";
        let mut parser = Parser::new(input);
        
        // Verify parsing succeeds
        let root = parser.parse().expect("Parse failed");
        
        // Verify we can access the arena
        let arena = parser.into_arena();
        
        // Verify the root exists in the arena
        let root_data = arena.get(root).expect("Root handle invalid");
        
        // Basic structure check (1 + (2 * 3))
        match root_data {
            NodeData::BinaryOp { op, .. } => assert_eq!(*op, BinaryOp::Add),
            _ => panic!("Root was not a BinaryOp"),
        }
    }
    
    #[test]
    fn test_arena_allocation() {
        let input = "42";
        let mut parser = Parser::new(input);
        let root = parser.parse().unwrap();
        let arena = parser.into_arena();
        
        // Ensure ThinNode handles point to correct data
        match arena.get(root) {
            Some(NodeData::Number { value, .. }) => assert_eq!(*value, 42),
            _ => panic!("Expected Number node"),
        }
    }
}
```
