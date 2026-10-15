use serde::{Deserialize, Serialize};

/// Represents an exact byte interval [start, end) in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    #[inline]
    pub const fn new(start: usize, end: usize) -> Self {
        assert!(start <= end, "Span start cannot be greater than end");
        Self { start, end }
    }

    #[inline]
    pub const fn empty(pos: usize) -> Self {
        Self { start: pos, end: pos }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    #[inline]
    pub fn contains(&self, byte_offset: usize) -> bool {
        byte_offset >= self.start && byte_offset < self.end
    }

    #[inline]
    pub fn contains_inclusive(&self, byte_offset: usize) -> bool {
        byte_offset >= self.start && byte_offset <= self.end
    }

    #[inline]
    pub fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    #[inline]
    pub fn shift(&self, offset: isize) -> Self {
        let new_start = (self.start as isize + offset).max(0) as usize;
        let new_end = (self.end as isize + offset).max(new_start as isize) as usize;
        Self {
            start: new_start,
            end: new_end,
        }
    }

    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        if self.start > text.len() || self.end > text.len() {
            return "";
        }
        &text[self.start..self.end]
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}..{})", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_methods() {
        let span = Span::new(5, 10);
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());
        assert!(span.contains(5));
        assert!(span.contains(9));
        assert!(!span.contains(10));
        assert!(span.contains_inclusive(10));
        assert_eq!(span.slice("Hello, World!"), ", Wor");
    }
}
