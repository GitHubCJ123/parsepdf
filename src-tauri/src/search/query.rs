use thiserror::Error;

const MAX_QUERY_CHARS: usize = 256;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QueryError {
    #[error("Search query is empty after sanitization")]
    Empty,
}

/// Sanitize a user query for safe use in FTS5 MATCH.
///
/// Phase 3 uses a deliberately small grammar: ASCII words and quoted phrases.
/// Every emitted term is FTS5-quoted and the final expression is still bound as a
/// SQL parameter by the caller.
pub fn build_match_expr(user_input: &str) -> Result<(String, Vec<String>), QueryError> {
    let trimmed = user_input.trim();
    if trimmed.is_empty() {
        return Err(QueryError::Empty);
    }

    let mut warnings = Vec::new();
    let input = if trimmed.chars().count() > MAX_QUERY_CHARS {
        warnings.push("Query truncated to 256 chars".to_string());
        trimmed.chars().take(MAX_QUERY_CHARS).collect::<String>()
    } else {
        trimmed.to_string()
    };

    let simple_input = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '"' {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>();

    let tokens = parse_simple_tokens(&simple_input);
    if tokens.is_empty() {
        return Err(QueryError::Empty);
    }

    let expr = tokens
        .into_iter()
        .map(|token| format!("\"{token}\""))
        .collect::<Vec<_>>()
        .join(" ");
    Ok((expr, warnings))
}

fn parse_simple_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                push_normalized(&mut tokens, &mut current);
                in_quote = !in_quote;
            }
            ch if ch.is_whitespace() && !in_quote => {
                push_normalized(&mut tokens, &mut current);
            }
            ch => current.push(ch),
        }
    }

    push_normalized(&mut tokens, &mut current);
    tokens
}

fn push_normalized(tokens: &mut Vec<String>, current: &mut String) {
    let normalized = current.split_whitespace().collect::<Vec<_>>().join(" ");
    if !normalized.is_empty() {
        tokens.push(normalized);
    }
    current.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_error() {
        assert_eq!(build_match_expr("   ").unwrap_err(), QueryError::Empty);
    }

    #[test]
    fn strips_fts_syntax_characters() {
        let (expr, warnings) = build_match_expr("invoice* status:paid").unwrap();
        assert!(warnings.is_empty());
        assert_eq!(expr, "\"invoice\" \"status\" \"paid\"");
        assert!(!expr.contains('*'));
        assert!(!expr.contains(':'));
    }

    #[test]
    fn preserves_quoted_phrases() {
        let (expr, _) = build_match_expr("\"acme corp\" invoice").unwrap();
        assert_eq!(expr, "\"acme corp\" \"invoice\"");
    }

    #[test]
    fn truncates_very_long_input() {
        let long = "a".repeat(300);
        let (expr, warnings) = build_match_expr(&long).unwrap();
        assert_eq!(warnings, vec!["Query truncated to 256 chars"]);
        assert_eq!(expr.len(), 258);
    }

    #[test]
    fn keeps_single_character_tokens() {
        let (expr, _) = build_match_expr("a b c").unwrap();
        assert_eq!(expr, "\"a\" \"b\" \"c\"");
    }

    #[test]
    fn injection_like_input_is_only_terms() {
        let (expr, _) = build_match_expr("'); DROP TABLE pages;--").unwrap();
        assert_eq!(expr, "\"DROP\" \"TABLE\" \"pages\"");
    }
}
