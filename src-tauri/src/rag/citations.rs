use std::collections::HashSet;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::retrieval::RetrievedChunk;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CitationRef {
    pub index: usize,
    pub chunk_id: i64,
    pub page_id: i64,
    pub document_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroundedAnswer {
    pub content: String,
    pub citations: Vec<CitationRef>,
}

pub fn ground_citations(answer: &str, retrieved: &[RetrievedChunk]) -> GroundedAnswer {
    let (content, indices) = sanitize_citation_markers(answer, retrieved.len());
    let citations = indices
        .into_iter()
        .filter_map(|index| {
            retrieved.get(index - 1).map(|chunk| CitationRef {
                index,
                chunk_id: chunk.chunk_id,
                page_id: chunk.page_id,
                document_id: chunk.document_id,
            })
        })
        .collect();
    GroundedAnswer { content, citations }
}

pub fn sanitize_citation_markers(answer: &str, max_index: usize) -> (String, Vec<usize>) {
    let regex = Regex::new(r"\[(\d{1,4})\]").expect("citation regex is valid");
    let mut seen = HashSet::new();
    let mut indices = Vec::new();
    let content = regex
        .replace_all(answer, |captures: &regex::Captures<'_>| {
            let raw = captures
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            match raw.parse::<usize>() {
                Ok(index) if index > 0 && index <= max_index => {
                    if seen.insert(index) {
                        indices.push(index);
                    }
                    format!("[{index}]")
                }
                _ => {
                    warn!(
                        citation = raw,
                        max_index, "rejected hallucinated citation marker"
                    );
                    "[?]".to_string()
                }
            }
        })
        .to_string();
    (content, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_hallucinated_citation_indices() {
        let (content, indices) = sanitize_citation_markers("Use [1] but not [99].", 3);
        assert_eq!(content, "Use [1] but not [?].");
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn deduplicates_valid_citations_in_first_seen_order() {
        let (content, indices) = sanitize_citation_markers("A [2] B [1] C [2]", 2);
        assert_eq!(content, "A [2] B [1] C [2]");
        assert_eq!(indices, vec![2, 1]);
    }
}
