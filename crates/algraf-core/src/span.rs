//! Source spans (spec §11.2).
//!
//! Spans are byte offsets into the source document (spec §6.1, §6.12). Ranges
//! are half-open: `start` is inclusive, `end` is exclusive. Zero-length spans
//! are permitted for inserted recovery nodes.

use serde::{Deserialize, Serialize};

/// A byte offset into a source document.
pub type ByteOffset = usize;

/// A half-open byte range `[start, end)` into a source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: ByteOffset,
    pub end: ByteOffset,
}

impl Span {
    /// Create a span from a start (inclusive) and end (exclusive) offset.
    pub fn new(start: ByteOffset, end: ByteOffset) -> Self {
        debug_assert!(start <= end, "span start must not exceed end");
        Span { start, end }
    }

    /// Create a zero-length span at `offset`, used for inserted recovery nodes.
    pub fn empty(offset: ByteOffset) -> Self {
        Span {
            start: offset,
            end: offset,
        }
    }

    /// The number of bytes covered by this span.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the span covers zero bytes.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Whether `offset` falls within the half-open range.
    pub fn contains(&self, offset: ByteOffset) -> bool {
        self.start <= offset && offset < self.end
    }

    /// The smallest span covering both `self` and `other`.
    pub fn cover(&self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl From<std::ops::Range<usize>> for Span {
    fn from(range: std::ops::Range<usize>) -> Self {
        Span::new(range.start, range.end)
    }
}

impl From<Span> for std::ops::Range<usize> {
    fn from(span: Span) -> Self {
        span.start..span.end
    }
}
