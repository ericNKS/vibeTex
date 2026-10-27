pub mod checker;
pub mod dictionary;
pub mod levenshtein;
pub mod tokenizer;

pub use checker::{LanguageMode, PersonalDictionary, SpellChecker, SpellingIssue};
pub use dictionary::Dictionary;
pub use levenshtein::damerau_levenshtein;
pub use tokenizer::{LatexToken, LatexTokenizer};
