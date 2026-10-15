/// Analyzes whether the LaTeX source code contains unclosed commands, unclosed braces,
/// or is in the middle of a macro being edited at the cursor position.
/// When true, auto-compilation should be deferred until the user completes the command.
pub fn is_latex_command_incomplete(source: &str, cursor_offset: Option<usize>) -> bool {
    let text_at_cursor = if let Some(offset) = cursor_offset {
        if offset <= source.len() {
            &source[..offset]
        } else {
            source
        }
    } else {
        source
    };

    // 1. Check if the text ending at cursor is an unfinished macro name, e.g. "\sec", "\textb", "\ci"
    let bytes = text_at_cursor.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_alphabetic() {
        i -= 1;
    }
    if i > 0 && bytes[i - 1] == b'\\' {
        // Count preceding backslashes to ensure it's not escaped (e.g. \\)
        let mut slashes = 0;
        let mut k = i;
        while k > 0 && bytes[k - 1] == b'\\' {
            slashes += 1;
            k -= 1;
        }
        if slashes % 2 == 1 {
            // Unfinished command name!
            return true;
        }
    }

    // 2. Count unescaped open vs closed braces and inline math dollars across the full document
    let mut open_braces = 0i32;
    let mut in_comment = false;
    let mut escaped = false;
    let mut math_dollars = 0usize;

    for &b in source.as_bytes() {
        if b == b'\n' {
            in_comment = false;
            escaped = false;
            continue;
        }
        if in_comment {
            continue;
        }
        if escaped {
            escaped = false;
            continue;
        }
        if b == b'\\' {
            escaped = true;
            continue;
        }
        if b == b'%' {
            in_comment = true;
            continue;
        }
        if b == b'{' {
            open_braces += 1;
        } else if b == b'}' {
            open_braces -= 1;
        } else if b == b'$' {
            math_dollars += 1;
        }
    }

    // Unclosed brace: e.g. \section{... or \cite{...
    if open_braces > 0 {
        return true;
    }

    // Unclosed inline math $ ... $
    if math_dollars % 2 != 0 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_latex() {
        let code = r"\documentclass{article}\begin{document}\section{Intro} Texto.\end{document}";
        assert!(!is_latex_command_incomplete(code, None));
    }

    #[test]
    fn test_unclosed_brace() {
        let code = r"\documentclass{article}\begin{document}\section{Intro";
        assert!(is_latex_command_incomplete(code, None));
    }

    #[test]
    fn test_unclosed_cite() {
        let code = r"Como visto em \cite{einstein";
        assert!(is_latex_command_incomplete(code, None));
    }

    #[test]
    fn test_incomplete_macro_at_cursor() {
        let code = r"\documentclass{article} \sec";
        // Cursor at the very end of "\sec"
        assert!(is_latex_command_incomplete(code, Some(code.len())));
    }

    #[test]
    fn test_complete_macro_followed_by_space() {
        let code = r"\documentclass{article} \maketitle ";
        assert!(!is_latex_command_incomplete(code, Some(code.len())));
    }

    #[test]
    fn test_unclosed_inline_math() {
        let code = r"Fórmula $x + y e mais texto";
        assert!(is_latex_command_incomplete(code, None));
    }

    #[test]
    fn test_closed_inline_math() {
        let code = r"Fórmula $x + y$ e mais texto";
        assert!(!is_latex_command_incomplete(code, None));
    }

    #[test]
    fn test_escaped_braces_and_comments() {
        let code = r"% { comentário aberto
        Texto \{ literal \} normal";
        assert!(!is_latex_command_incomplete(code, None));
    }
}
