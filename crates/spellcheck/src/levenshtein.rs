/// Calculates the Damerau-Levenshtein distance between two strings,
/// allowing insertions, deletions, substitutions, and transpositions of adjacent characters.
pub fn damerau_levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let len_a = a_chars.len();
    let len_b = b_chars.len();

    if len_a == 0 {
        return len_b;
    }
    if len_b == 0 {
        return len_a;
    }

    // dp[i][j] stores distance between a[0..i] and b[0..j]
    let mut dp = vec![vec![0usize; len_b + 1]; len_a + 1];

    for i in 0..=len_a {
        dp[i][0] = i;
    }
    for j in 0..=len_b {
        dp[0][j] = j;
    }

    for i in 1..=len_a {
        for j in 1..=len_b {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };

            let mut d = (dp[i - 1][j] + 1) // deletion
                .min(dp[i][j - 1] + 1) // insertion
                .min(dp[i - 1][j - 1] + cost); // substitution

            // Transposition of adjacent characters
            if i > 1 && j > 1 && a_chars[i - 1] == b_chars[j - 2] && a_chars[i - 2] == b_chars[j - 1] {
                d = d.min(dp[i - 2][j - 2] + 1);
            }

            dp[i][j] = d;
        }
    }

    dp[len_a][len_b]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_cases() {
        assert_eq!(damerau_levenshtein("teh", "the"), 1); // transposition
        assert_eq!(damerau_levenshtein("livro", "livro"), 0);
        assert_eq!(damerau_levenshtein("livro", "livros"), 1); // insertion
        assert_eq!(damerau_levenshtein("conclusao", "conclusão"), 1); // substitution
    }
}
