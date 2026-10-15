use crate::ast::*;
use crate::token::{Lexer, Token, TokenKind};
use vibetex_core::Span;

pub struct LatexParser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
}

impl<'a> LatexParser<'a> {
    pub fn new(source: &'a str) -> Self {
        let tokens = Lexer::new(source).tokenize_all();
        Self {
            source,
            tokens,
            cursor: 0,
        }
    }

    #[inline]
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    #[inline]
    fn advance(&mut self) -> Option<Token> {
        if self.cursor < self.tokens.len() {
            let tok = self.tokens[self.cursor].clone();
            self.cursor += 1;
            Some(tok)
        } else {
            None
        }
    }

    pub fn parse_document(&mut self) -> DocumentAst {
        let full_span = Span::new(0, self.source.len());
        let mut ast = DocumentAst::new(full_span);

        // Check for preamble: look for \begin{document}
        let doc_begin_idx = self.find_begin_document();
        if let Some(begin_tok_idx) = doc_begin_idx {
            let preamble_end = self.tokens[begin_tok_idx].span.start;
            if preamble_end > 0 {
                let preamble_raw = self.source[0..preamble_end].to_string();
                ast.preamble = Some(AstNode::Preamble {
                    raw: preamble_raw,
                    span: Span::new(0, preamble_end),
                });
            }
            // Advance past \begin{document}
            self.cursor = begin_tok_idx;
            self.consume_begin_document();
        }

        // Parse body nodes
        while self.cursor < self.tokens.len() {
            if let Some(tok) = self.peek() {
                if let TokenKind::Command(ref name) = tok.kind {
                    if name == "end" && self.is_env_name("document", 1) {
                        // Reached \end{document}
                        self.advance(); // \end
                        self.skip_braces(); // {document}
                        break;
                    }
                }
            }

            let prev_cursor = self.cursor;
            if let Some(node) = self.parse_node() {
                ast.nodes.push(node);
            } else if self.cursor == prev_cursor {
                self.advance();
            }
        }

        ast
    }

    fn find_begin_document(&self) -> Option<usize> {
        for i in 0..self.tokens.len() {
            if let TokenKind::Command(ref name) = self.tokens[i].kind {
                if name == "begin" {
                    if let Some(tok_brace) = self.tokens.get(i + 1) {
                        if matches!(tok_brace.kind, TokenKind::OpenBrace) {
                            if let Some(tok_name) = self.tokens.get(i + 2) {
                                if let TokenKind::Word(ref env) = tok_name.kind {
                                    if env == "document" {
                                        return Some(i);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn is_env_name(&self, expected: &str, offset: usize) -> bool {
        let mut idx = self.cursor + offset;
        while idx < self.tokens.len() && matches!(self.tokens[idx].kind, TokenKind::Whitespace(_) | TokenKind::Newline) {
            idx += 1;
        }
        if let Some(tok) = self.tokens.get(idx) {
            if matches!(tok.kind, TokenKind::OpenBrace) {
                if let Some(name_tok) = self.tokens.get(idx + 1) {
                    if let TokenKind::Word(ref name) = name_tok.kind {
                        return name == expected;
                    }
                }
            }
        }
        false
    }

    fn consume_begin_document(&mut self) {
        if let Some(tok) = self.peek() {
            if matches!(tok.kind, TokenKind::Command(ref name) if name == "begin") {
                self.advance();
                self.skip_braces();
            }
        }
    }

    fn skip_braces(&mut self) {
        while self.cursor < self.tokens.len() && matches!(self.tokens[self.cursor].kind, TokenKind::Whitespace(_) | TokenKind::Newline) {
            self.cursor += 1;
        }
        if let Some(tok) = self.peek() {
            if matches!(tok.kind, TokenKind::OpenBrace) {
                self.advance();
                let mut depth = 1;
                while self.cursor < self.tokens.len() && depth > 0 {
                    let t = self.advance().unwrap();
                    match t.kind {
                        TokenKind::OpenBrace => depth += 1,
                        TokenKind::CloseBrace => depth -= 1,
                        _ => {}
                    }
                }
            }
        }
    }

    fn parse_node(&mut self) -> Option<AstNode> {
        let (tok_kind, tok_span) = {
            let tok = self.peek()?;
            (tok.kind.clone(), tok.span)
        };

        match tok_kind {
            TokenKind::Comment(text) => {
                self.advance();
                Some(AstNode::Comment { text, span: tok_span })
            }
            TokenKind::DisplayMath(content) => {
                self.advance();
                Some(AstNode::MathBlock {
                    kind: MathBlockKind::DisplayDollar,
                    content,
                    span: tok_span,
                })
            }
            TokenKind::Command(cmd) => {
                if let Some(level) = SectionLevel::from_command(&cmd) {
                    self.advance(); // consume \section
                    let (title, title_span, end_byte) = self.parse_braced_argument()?;
                    return Some(AstNode::Section {
                        level,
                        title,
                        title_span,
                        span: Span::new(tok_span.start, end_byte),
                    });
                }

                if cmd == "begin" {
                    return self.parse_environment(tok_span);
                }

                match cmd.as_str() {
                    "medskip" => {
                        self.advance();
                        return Some(AstNode::Spacing { kind: SpacingKind::MedSkip, argument: None, span: tok_span });
                    }
                    "smallskip" => {
                        self.advance();
                        return Some(AstNode::Spacing { kind: SpacingKind::SmallSkip, argument: None, span: tok_span });
                    }
                    "bigskip" => {
                        self.advance();
                        return Some(AstNode::Spacing { kind: SpacingKind::BigSkip, argument: None, span: tok_span });
                    }
                    "noindent" => {
                        self.advance();
                        return Some(AstNode::Spacing { kind: SpacingKind::NoIndent, argument: None, span: tok_span });
                    }
                    "vspace" | "hspace" => {
                        self.advance();
                        let kind = if cmd == "vspace" { SpacingKind::VSpace } else { SpacingKind::HSpace };
                        let (arg, _, end_byte) = self.parse_braced_argument().unwrap_or_default();
                        return Some(AstNode::Spacing { kind, argument: Some(arg), span: Span::new(tok_span.start, end_byte) });
                    }
                    _ => {}
                }

                self.parse_paragraph()
            }
            TokenKind::Whitespace(_) | TokenKind::Newline => {
                self.advance();
                None
            }
            _ => self.parse_paragraph(),
        }
    }

    fn parse_environment(&mut self, begin_span: Span) -> Option<AstNode> {
        self.advance(); // \begin
        let (env_name, _, mut current_end) = self.parse_braced_argument()?;

        let mut _options = None;
        if let Some(tok) = self.peek() {
            if matches!(tok.kind, TokenKind::OpenBracket) {
                let (opt_text, end_byte) = self.parse_bracket_argument();
                _options = Some(opt_text);
                current_end = end_byte;
            }
        }

        match env_name.as_str() {
            "itemize" => {
                let items = self.parse_list_items("itemize");
                let end_span = self.consume_env_end("itemize");
                let full_span = Span::new(begin_span.start, end_span.map(|s| s.end).unwrap_or(current_end));
                Some(AstNode::Itemize { items, span: full_span })
            }
            "enumerate" => {
                let items = self.parse_list_items("enumerate");
                let end_span = self.consume_env_end("enumerate");
                let full_span = Span::new(begin_span.start, end_span.map(|s| s.end).unwrap_or(current_end));
                Some(AstNode::Enumerate { items, span: full_span })
            }
            "equation" | "align" | "align*" | "gather" => {
                let math_start = current_end;
                let end_span = self.find_and_consume_env_end(&env_name);
                let math_end = end_span.map(|s| s.start).unwrap_or(self.source.len());
                let content = self.source[math_start..math_end].trim().to_string();
                let full_span = Span::new(begin_span.start, end_span.map(|s| s.end).unwrap_or(self.source.len()));
                let kind = if env_name.starts_with("align") {
                    MathBlockKind::Align
                } else {
                    MathBlockKind::Equation
                };
                Some(AstNode::MathBlock { kind, content, span: full_span })
            }
            _ => {
                let env_start = begin_span.start;
                let end_span = self.find_and_consume_env_end(&env_name);
                let full_end = end_span.map(|s| s.end).unwrap_or(self.source.len());
                let raw = self.source[env_start..full_end].to_string();
                Some(AstNode::ProtectedBlock {
                    raw,
                    description: format!("Ambiente LaTeX: {}", env_name),
                    span: Span::new(env_start, full_end),
                })
            }
        }
    }

    fn parse_list_items(&mut self, env_name: &str) -> Vec<ListItem> {
        let mut items = Vec::new();
        while self.cursor < self.tokens.len() {
            if let Some(tok) = self.peek() {
                if let TokenKind::Command(ref name) = tok.kind {
                    if name == "end" && self.is_env_name(env_name, 1) {
                        break;
                    }
                    if name == "item" {
                        let item_start = tok.span.start;
                        self.advance(); // \item
                        let mut text = String::new();
                        while self.cursor < self.tokens.len() {
                            if let Some(next_tok) = self.peek() {
                                if let TokenKind::Command(ref n) = next_tok.kind {
                                    if n == "item" || (n == "end" && self.is_env_name(env_name, 1)) {
                                        break;
                                    }
                                }
                            }
                            let t = self.advance().unwrap();
                            text.push_str(t.span.slice(self.source));
                        }
                        let item_end = self.tokens.get(self.cursor - 1).map(|t| t.span.end).unwrap_or(item_start);
                        items.push(ListItem {
                            content: text.trim().to_string(),
                            span: Span::new(item_start, item_end),
                        });
                        continue;
                    }
                }
            }
            self.advance();
        }
        items
    }

    fn find_and_consume_env_end(&mut self, env_name: &str) -> Option<Span> {
        while self.cursor < self.tokens.len() {
            if let Some(tok) = self.peek() {
                if let TokenKind::Command(ref name) = tok.kind {
                    if name == "end" && self.is_env_name(env_name, 1) {
                        return self.consume_env_end(env_name);
                    }
                }
            }
            self.advance();
        }
        None
    }

    fn consume_env_end(&mut self, _env_name: &str) -> Option<Span> {
        let start = self.peek()?.span.start;
        self.advance(); // \end
        self.skip_braces();
        let end = self.tokens.get(self.cursor - 1).map(|t| t.span.end).unwrap_or(start);
        Some(Span::new(start, end))
    }

    fn parse_paragraph(&mut self) -> Option<AstNode> {
        let mut inlines = Vec::new();
        let start_span = self.peek()?.span;
        let mut consecutive_newlines = 0;

        while self.cursor < self.tokens.len() {
            let (tok_kind, tok_span) = {
                let tok = self.peek()?;
                (tok.kind.clone(), tok.span)
            };

            if matches!(tok_kind, TokenKind::Newline) {
                consecutive_newlines += 1;
                if consecutive_newlines >= 2 {
                    self.advance();
                    break;
                }
                self.advance();
                continue;
            } else {
                consecutive_newlines = 0;
            }

            if let TokenKind::Command(ref name) = tok_kind {
                if SectionLevel::from_command(name).is_some() || name == "begin" || name == "end" {
                    break;
                }

                if let Some(kind) = FormatKind::from_command(name) {
                    let cmd_start = tok_span.start;
                    self.advance(); // consume \textbf
                    if let Some((inner_text, inner_span, end_byte)) = self.parse_braced_argument() {
                        inlines.push(InlineNode::Format {
                            kind,
                            inner: inner_text,
                            inner_span,
                            span: Span::new(cmd_start, end_byte),
                        });
                        continue;
                    }
                }

                if name == "cite" || name == "parencite" || name == "textcite" || name == "autocite" {
                    let cmd_start = tok_span.start;
                    let cmd = name.clone();
                    self.advance();
                    if let Some((key, _, end_byte)) = self.parse_braced_argument() {
                        inlines.push(InlineNode::Citation {
                            key,
                            command: cmd,
                            span: Span::new(cmd_start, end_byte),
                        });
                        continue;
                    }
                }

                if name == "ref" || name == "eqref" || name == "autoref" {
                    let cmd_start = tok_span.start;
                    let cmd = name.clone();
                    self.advance();
                    if let Some((label, _, end_byte)) = self.parse_braced_argument() {
                        inlines.push(InlineNode::Reference {
                            label,
                            command: cmd,
                            span: Span::new(cmd_start, end_byte),
                        });
                        continue;
                    }
                }

                if name == "label" {
                    let cmd_start = tok_span.start;
                    self.advance();
                    if let Some((key, _, end_byte)) = self.parse_braced_argument() {
                        inlines.push(InlineNode::Label {
                            key,
                            span: Span::new(cmd_start, end_byte),
                        });
                        continue;
                    }
                }

                let raw = tok_span.slice(self.source).to_string();
                self.advance();
                inlines.push(InlineNode::Protected { raw, span: tok_span });
                continue;
            }

            if let TokenKind::InlineMath(ref content) = tok_kind {
                let content = content.clone();
                self.advance();
                inlines.push(InlineNode::InlineMath { content, span: tok_span });
                continue;
            }

            let text = tok_span.slice(self.source).to_string();
            self.advance();
            inlines.push(InlineNode::Text { text, span: tok_span });
        }

        if inlines.is_empty() {
            return None;
        }

        let end_byte = inlines.last().map(|i| i.span().end).unwrap_or(start_span.end);
        Some(AstNode::Paragraph {
            inlines,
            span: Span::new(start_span.start, end_byte),
        })
    }

    fn parse_braced_argument(&mut self) -> Option<(String, Span, usize)> {
        while self.cursor < self.tokens.len() && matches!(self.peek()?.kind, TokenKind::Whitespace(_) | TokenKind::Newline) {
            self.advance();
        }

        let first = self.peek()?;
        if !matches!(first.kind, TokenKind::OpenBrace) {
            return None;
        }

        let open_span = first.span;
        self.advance(); // {

        let content_start = open_span.end;
        let mut depth = 1;

        while self.cursor < self.tokens.len() && depth > 0 {
            let tok = self.advance().unwrap();
            match tok.kind {
                TokenKind::OpenBrace => depth += 1,
                TokenKind::CloseBrace => {
                    depth -= 1;
                    if depth == 0 {
                        let close_end = tok.span.end;
                        let content_end = tok.span.start;
                        let inner_span = Span::new(content_start, content_end);
                        let inner_text = self.source[content_start..content_end].to_string();
                        return Some((inner_text, inner_span, close_end));
                    }
                }
                _ => {}
            }
        }

        None
    }

    fn parse_bracket_argument(&mut self) -> (String, usize) {
        let first = self.advance().unwrap(); // [
        let start = first.span.end;

        while self.cursor < self.tokens.len() {
            let tok = self.advance().unwrap();
            if matches!(tok.kind, TokenKind::CloseBracket) {
                let end = tok.span.start;
                let text = self.source[start..end].to_string();
                return (text, tok.span.end);
            }
        }
        (self.source[start..self.source.len()].to_string(), self.source.len())
    }
}

pub fn parse_latex(source: &str) -> DocumentAst {
    LatexParser::new(source).parse_document()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_section_and_bold() {
        let doc = r"\documentclass{article}
\begin{document}
\section{Introdução ao Estudo}
Este é um texto com \textbf{negrito} e $x^2 + y^2 = z^2$.
\end{document}";

        let ast = parse_latex(doc);
        assert!(ast.preamble.is_some());
        assert!(!ast.nodes.is_empty());

        let mut found_section = false;
        let mut found_bold = false;

        for node in &ast.nodes {
            match node {
                AstNode::Section { title, title_span, span, .. } => {
                    found_section = true;
                    assert_eq!(title, "Introdução ao Estudo");
                    assert_eq!(title_span.slice(doc), "Introdução ao Estudo");
                    assert!(span.slice(doc).starts_with(r"\section{"));
                }
                AstNode::Paragraph { inlines, .. } => {
                    for inline in inlines {
                        if let InlineNode::Format { kind, inner, inner_span, .. } = inline {
                            if *kind == FormatKind::Bold {
                                found_bold = true;
                                assert_eq!(inner, "negrito");
                                assert_eq!(inner_span.slice(doc), "negrito");
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        assert!(found_section, "Deve encontrar a seção");
        assert!(found_bold, "Deve encontrar o nó em negrito");
    }

    #[test]
    fn test_preservation_of_unknown_commands_and_comments() {
        let doc = r"\begin{document}
% Comentário importante
\meuComandoPersonalizado{123}
\begin{tikzpicture}
\draw (0,0) -- (1,1);
\end{tikzpicture}
Texto normal.
\end{document}";

        let ast = parse_latex(doc);
        let mut found_comment = false;
        let mut found_protected_env = false;

        for node in &ast.nodes {
            match node {
                AstNode::Comment { text, .. } => {
                    if text.contains("Comentário importante") {
                        found_comment = true;
                    }
                }
                AstNode::ProtectedBlock { raw, .. } => {
                    if raw.contains("tikzpicture") {
                        found_protected_env = true;
                    }
                }
                _ => {}
            }
        }

        assert!(found_comment, "Comentários devem ser preservados");
        assert!(found_protected_env, "Ambientes complexos/TikZ devem ser preservados como ProtectedBlock");
    }
}
