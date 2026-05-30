use algraf_core::Span;
use lsp_types::{Position, Range};

pub fn span_to_range(source: &str, span: Span) -> Range {
    Range {
        start: offset_to_position(source, span.start),
        end: offset_to_position(source, span.end),
    }
}

pub fn offset_to_position(source: &str, offset: usize) -> Position {
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

pub fn position_to_offset(source: &str, position: Position) -> usize {
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

pub fn range_to_offsets(source: &str, range: Range) -> Option<(usize, usize)> {
    let start = position_to_offset(source, range.start);
    let end = position_to_offset(source, range.end);
    (start <= end && end <= source.len()).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_offset_round_trips_through_position() {
        let source = "Chart(data: \"p.csv\") {\n  Space(x) {}\n}";
        for offset in 0..=source.len() {
            if !source.is_char_boundary(offset) {
                continue;
            }
            let pos = offset_to_position(source, offset);
            assert_eq!(position_to_offset(source, pos), offset, "offset {offset}");
        }
    }

    #[test]
    fn non_ascii_uses_utf16_columns_for_byte_spans() {
        // `é` is two UTF-8 bytes but one UTF-16 unit; `𝄞` is four UTF-8 bytes
        // and two UTF-16 units. LSP positions are UTF-16, spans are byte
        // offsets (spec §11.2, §21.x), so the conversion must account for both.
        let source = "let café = 𝄞\nlet y = 1";
        // Byte offset of `=` after `café ` (c=1,a=1,f=1,é=2 bytes => "café" is 5 bytes).
        let eq = source.find('=').unwrap();
        let pos = offset_to_position(source, eq);
        assert_eq!(pos.line, 0);
        // "let café " is l e t space c a f é space = 9 UTF-16 units before `=`.
        assert_eq!(pos.character, 9);
        assert_eq!(position_to_offset(source, pos), eq);

        // The astral clef sits at the end of line 0; converting its end offset
        // round-trips and the next line starts cleanly.
        let clef_start = source.find('𝄞').unwrap();
        let clef_end = clef_start + '𝄞'.len_utf8();
        let end_pos = offset_to_position(source, clef_end);
        assert_eq!(position_to_offset(source, end_pos), clef_end);

        // A byte span over `café` maps to UTF-16 columns 4..8 (é is one unit).
        let start = source.find("café").unwrap();
        let range = span_to_range(source, Span::new(start, start + "café".len()));
        assert_eq!(range.start.character, 4);
        assert_eq!(range.end.character, 8);
    }

    #[test]
    fn position_past_end_clamps_to_source_length() {
        let source = "abc";
        let pos = Position::new(5, 0);
        assert_eq!(position_to_offset(source, pos), source.len());
    }
}
