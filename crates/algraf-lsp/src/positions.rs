use algraf_core::Span;
use tower_lsp::lsp_types::{Position, Range};

pub(crate) fn span_to_range(source: &str, span: Span) -> Range {
    Range {
        start: offset_to_position(source, span.start),
        end: offset_to_position(source, span.end),
    }
}

pub(crate) fn offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0;
    let mut line_start = 0;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = i + ch.len_utf8();
        }
    }
    let character = source[line_start..offset]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>();
    Position::new(line as u32, character as u32)
}

pub(crate) fn position_to_offset(source: &str, position: Position) -> usize {
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, ch) in source.char_indices() {
        if line == position.line {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = i + ch.len_utf8();
        }
    }
    if line != position.line {
        return source.len();
    }

    let mut utf16 = 0u32;
    for (rel, ch) in source[line_start..].char_indices() {
        if ch == '\n' {
            return line_start + rel;
        }
        if utf16 >= position.character {
            return line_start + rel;
        }
        let next = utf16 + ch.len_utf16() as u32;
        if next > position.character {
            return line_start + rel;
        }
        utf16 = next;
    }
    source.len()
}

pub(crate) fn range_to_offsets(source: &str, range: Range) -> Option<(usize, usize)> {
    let start = position_to_offset(source, range.start);
    let end = position_to_offset(source, range.end);
    (start <= end && end <= source.len()).then_some((start, end))
}
