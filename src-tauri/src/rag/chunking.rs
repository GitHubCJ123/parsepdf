#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageText {
    pub document_id: i64,
    pub page_id: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub document_id: i64,
    pub page_id: i64,
    pub char_start: usize,
    pub char_end: usize,
    pub token_count: usize,
    pub text: String,
}

const MIN_PAGE_CHARS: usize = 20;

pub fn chunk_pages(pages: &[PageText], target_tokens: usize, overlap_tokens: usize) -> Vec<Chunk> {
    let target_chars = target_tokens.max(1) * 4;
    let overlap_chars = overlap_tokens.min(target_tokens.saturating_sub(1)) * 4;
    let mut chunks = Vec::new();

    for page in pages {
        if page.text.trim().chars().count() < MIN_PAGE_CHARS {
            continue;
        }
        let text = page.text.as_str();
        let mut start = first_non_whitespace_at_or_after(text, 0);
        while start < text.len() {
            let max_end = byte_index_after_chars(text, start, target_chars).unwrap_or(text.len());
            let split_end = choose_split(text, start, max_end);
            let (trim_start, trim_end) = trim_byte_range(text, start, split_end);
            if trim_end > trim_start {
                let chunk_text = text[trim_start..trim_end].to_string();
                chunks.push(Chunk {
                    document_id: page.document_id,
                    page_id: page.page_id,
                    char_start: text[..trim_start].chars().count(),
                    char_end: text[..trim_end].chars().count(),
                    token_count: estimate_tokens(&chunk_text),
                    text: chunk_text,
                });
            }

            if split_end >= text.len() {
                break;
            }

            let overlap_start =
                byte_index_before_chars(text, split_end, overlap_chars).unwrap_or(split_end);
            let next_start = first_non_whitespace_at_or_after(text, overlap_start);
            start = if next_start <= start {
                first_non_whitespace_at_or_after(text, split_end)
            } else {
                next_start
            };
        }
    }

    chunks
}

pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.trim().chars().count();
    if chars == 0 {
        0
    } else {
        chars.div_ceil(4)
    }
}

fn choose_split(text: &str, start: usize, max_end: usize) -> usize {
    if max_end >= text.len() {
        return text.len();
    }
    let min_end = byte_index_after_chars(text, start, 256).unwrap_or(start);
    let window = &text[start..max_end];
    for delimiter in ["\n\n", "\n", " "] {
        if let Some(relative) = window.rfind(delimiter) {
            let candidate = start + relative;
            if candidate > min_end {
                return candidate;
            }
        }
    }
    previous_char_boundary(text, max_end)
}

fn trim_byte_range(text: &str, mut start: usize, mut end: usize) -> (usize, usize) {
    while start < end {
        let Some(ch) = text[start..end].chars().next() else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        start += ch.len_utf8();
    }
    while end > start {
        let Some((offset, ch)) = text[start..end].char_indices().last() else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        end = start + offset;
    }
    (start, end)
}

fn first_non_whitespace_at_or_after(text: &str, start: usize) -> usize {
    let mut index = next_char_boundary(text, start);
    while index < text.len() {
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn byte_index_after_chars(text: &str, start: usize, chars: usize) -> Option<usize> {
    if chars == 0 {
        return Some(start);
    }
    text[start..]
        .char_indices()
        .nth(chars)
        .map(|(offset, _)| start + offset)
}

fn byte_index_before_chars(text: &str, end: usize, chars: usize) -> Option<usize> {
    if chars == 0 {
        return Some(end);
    }
    let prefix = &text[..end];
    let total = prefix.chars().count();
    if chars >= total {
        return Some(0);
    }
    prefix
        .char_indices()
        .nth(total - chars)
        .map(|(offset, _)| offset)
}

fn previous_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn next_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(text: &str) -> PageText {
        PageText {
            document_id: 10,
            page_id: 20,
            text: text.to_string(),
        }
    }

    #[test]
    fn skips_near_empty_pages() {
        assert!(chunk_pages(&[page("short")], 512, 64).is_empty());
    }

    #[test]
    fn respects_page_boundaries() {
        let pages = vec![
            page(&"alpha ".repeat(700)),
            PageText {
                document_id: 10,
                page_id: 21,
                text: "beta ".repeat(700),
            },
        ];
        let chunks = chunk_pages(&pages, 64, 8);
        assert!(chunks.iter().any(|chunk| chunk.page_id == 20));
        assert!(chunks.iter().any(|chunk| chunk.page_id == 21));
        assert!(chunks
            .iter()
            .all(|chunk| chunk.text.contains("alpha") || chunk.text.contains("beta")));
    }

    #[test]
    fn overlaps_consecutive_chunks() {
        let text = (0..260)
            .map(|index| format!("word{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = chunk_pages(&[page(&text)], 64, 8);
        assert!(chunks.len() > 1);
        let first_words = chunks[0].text.split_whitespace().collect::<Vec<_>>();
        let second_words = chunks[1].text.split_whitespace().collect::<Vec<_>>();
        assert!(first_words.iter().any(|word| second_words.contains(word)));
    }

    #[test]
    fn prefers_paragraph_split() {
        let text = format!(
            "{}\n\n{}",
            "A paragraph sentence. ".repeat(80),
            "Second paragraph sentence. ".repeat(80)
        );
        let chunks = chunk_pages(&[page(&text)], 64, 8);
        assert!(chunks.len() >= 2);
        assert!(!chunks[0].text.ends_with(' '));
    }

    #[test]
    fn token_count_empty_input_is_zero() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   "), 0);
    }
}
