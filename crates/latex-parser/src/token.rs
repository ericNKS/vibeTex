use vibetex_core::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Command(String),
    OpenBrace,      // {
    CloseBrace,     // }
    OpenBracket,    // [
    CloseBracket,   // ]
    InlineMath(String),  // $...$ or \(...\)
    DisplayMath(String), // $$...$$ or \[...\]
    Comment(String),     // %...
    Whitespace(String),
    Newline,
    Word(String),
    Special(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

pub struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            cursor: 0,
        }
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        if self.cursor < self.bytes.len() {
            Some(self.bytes[self.cursor])
        } else {
            None
        }
    }

    #[inline]
    fn peek_next(&self) -> Option<u8> {
        if self.cursor + 1 < self.bytes.len() {
            Some(self.bytes[self.cursor + 1])
        } else {
            None
        }
    }

    #[inline]
    fn advance(&mut self) -> Option<u8> {
        if self.cursor < self.bytes.len() {
            let b = self.bytes[self.cursor];
            self.cursor += 1;
            Some(b)
        } else {
            None
        }
    }

    pub fn tokenize_all(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while let Some(token) = self.next_token() {
            tokens.push(token);
        }
        tokens
    }

    pub fn next_token(&mut self) -> Option<Token> {
        if self.cursor >= self.bytes.len() {
            return None;
        }

        let start = self.cursor;
        let c = self.peek()?;

        // Comments: % until end of line
        if c == b'%' {
            self.advance();
            let comment_start = self.cursor;
            while let Some(b) = self.peek() {
                if b == b'\n' {
                    break;
                }
                self.advance();
            }
            let comment_text = &self.source[comment_start..self.cursor];
            // Include trailing newline in comment if present for lossless spacing
            if let Some(b'\n') = self.peek() {
                self.advance();
            }
            let span = Span::new(start, self.cursor);
            return Some(Token::new(TokenKind::Comment(comment_text.to_string()), span));
        }

        // Display math: $$...$$ or \[...\]
        if c == b'$' && self.peek_next() == Some(b'$') {
            self.advance(); // $
            self.advance(); // $
            let math_start = self.cursor;
            while self.cursor + 1 < self.bytes.len() {
                if self.bytes[self.cursor] == b'$' && self.bytes[self.cursor + 1] == b'$' {
                    let content = &self.source[math_start..self.cursor];
                    self.advance(); // $
                    self.advance(); // $
                    let span = Span::new(start, self.cursor);
                    return Some(Token::new(TokenKind::DisplayMath(content.to_string()), span));
                }
                self.advance();
            }
            // Fallback unclosed
            let content = &self.source[math_start..self.bytes.len()];
            self.cursor = self.bytes.len();
            let span = Span::new(start, self.cursor);
            return Some(Token::new(TokenKind::DisplayMath(content.to_string()), span));
        }

        // Inline math: $...$
        if c == b'$' {
            self.advance(); // $
            let math_start = self.cursor;
            while let Some(b) = self.peek() {
                if b == b'$' {
                    let content = &self.source[math_start..self.cursor];
                    self.advance(); // closing $
                    let span = Span::new(start, self.cursor);
                    return Some(Token::new(TokenKind::InlineMath(content.to_string()), span));
                }
                if b == b'\\' {
                    // Skip escaped chars
                    self.advance();
                    self.advance();
                    continue;
                }
                self.advance();
            }
            // Fallback unclosed
            let content = &self.source[math_start..self.bytes.len()];
            self.cursor = self.bytes.len();
            let span = Span::new(start, self.cursor);
            return Some(Token::new(TokenKind::InlineMath(content.to_string()), span));
        }

        // Display math \[...\] or Inline \(...\)
        if c == b'\\' && (self.peek_next() == Some(b'[') || self.peek_next() == Some(b'(')) {
            let is_display = self.peek_next() == Some(b'[');
            let close_char = if is_display { b']' } else { b')' };
            self.advance(); // \
            self.advance(); // [ or (
            let math_start = self.cursor;
            while self.cursor + 1 < self.bytes.len() {
                if self.bytes[self.cursor] == b'\\' && self.bytes[self.cursor + 1] == close_char {
                    let content = &self.source[math_start..self.cursor];
                    self.advance(); // \
                    self.advance(); // ] or )
                    let span = Span::new(start, self.cursor);
                    let kind = if is_display {
                        TokenKind::DisplayMath(content.to_string())
                    } else {
                        TokenKind::InlineMath(content.to_string())
                    };
                    return Some(Token::new(kind, span));
                }
                self.advance();
            }
        }

        // Commands: \name
        if c == b'\\' {
            self.advance();
            let cmd_start = self.cursor;
            if let Some(first) = self.peek() {
                if first.is_ascii_alphabetic() {
                    while let Some(b) = self.peek() {
                        if b.is_ascii_alphabetic() || b == b'@' || b == b'*' {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                } else {
                    // Single non-alphabetic char like \\, \$, \%, \&, \_
                    self.advance();
                }
            }
            let cmd_name = &self.source[cmd_start..self.cursor];
            let span = Span::new(start, self.cursor);
            return Some(Token::new(TokenKind::Command(cmd_name.to_string()), span));
        }

        // Delimiters
        match c {
            b'{' => {
                self.advance();
                return Some(Token::new(TokenKind::OpenBrace, Span::new(start, self.cursor)));
            }
            b'}' => {
                self.advance();
                return Some(Token::new(TokenKind::CloseBrace, Span::new(start, self.cursor)));
            }
            b'[' => {
                self.advance();
                return Some(Token::new(TokenKind::OpenBracket, Span::new(start, self.cursor)));
            }
            b']' => {
                self.advance();
                return Some(Token::new(TokenKind::CloseBracket, Span::new(start, self.cursor)));
            }
            b'\n' => {
                self.advance();
                return Some(Token::new(TokenKind::Newline, Span::new(start, self.cursor)));
            }
            _ => {}
        }

        // Whitespace (excluding newline)
        if c == b' ' || c == b'\t' || c == b'\r' {
            while let Some(b) = self.peek() {
                if b == b' ' || b == b'\t' || b == b'\r' {
                    self.advance();
                } else {
                    break;
                }
            }
            let ws = &self.source[start..self.cursor];
            return Some(Token::new(TokenKind::Whitespace(ws.to_string()), Span::new(start, self.cursor)));
        }

        // Words / text
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace()
                || b == b'\\'
                || b == b'{'
                || b == b'}'
                || b == b'['
                || b == b']'
                || b == b'%'
                || b == b'$'
            {
                break;
            }
            self.advance();
        }

        let word = &self.source[start..self.cursor];
        if !word.is_empty() {
            Some(Token::new(TokenKind::Word(word.to_string()), Span::new(start, self.cursor)))
        } else {
            // Special single char fallback
            let b = self.advance()?;
            Some(Token::new(TokenKind::Special(b as char), Span::new(start, self.cursor)))
        }
    }
}
