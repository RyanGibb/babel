use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Eq, PartialEq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct OpamVersion(pub String);

impl OpamVersion {
    /// Split the version string into tokens.
    /// The algorithm splits into alternating non-digit and digit tokens,
    /// always starting with a non-digit token (inserting an empty token if needed).
    fn tokenize(&self) -> Vec<Token> {
        tokenize(&self.0)
    }
}

/// A token is either a numeric token or a non-numeric string token.
#[derive(Debug, PartialEq, Eq)]
enum Token {
    Num(u64),
    Str(String),
}

/// Tokenize a version string into alternating non-digit and digit tokens.
fn tokenize(version: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut is_digit = None; // Not set until we see the first character

    let mut chars = version.chars().peekable();

    // Debian/Opam rules say the token list always starts with a non-digit token.
    if let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            tokens.push(Token::Str(String::new()));
        }
    }

    while let Some(ch) = chars.next() {
        let ch_is_digit = ch.is_ascii_digit();
        match is_digit {
            Some(current_is_digit) if current_is_digit == ch_is_digit => {
                // Same type of token, so add the character.
                current.push(ch);
            }
            Some(_) => {
                // Type changed: push the current token and start a new one.
                if is_digit.unwrap() {
                    // Numeric token – parse it (ignoring leading zeros)
                    let num = current.parse::<u64>().unwrap_or(0);
                    tokens.push(Token::Num(num));
                } else {
                    tokens.push(Token::Str(current.clone()));
                }
                current.clear();
                current.push(ch);
                is_digit = Some(ch_is_digit);
            }
            None => {
                // First character encountered.
                current.push(ch);
                is_digit = Some(ch_is_digit);
            }
        }
    }
    // Push the final token.
    if let Some(current_is_digit) = is_digit {
        if current_is_digit {
            let num = current.parse::<u64>().unwrap_or(0);
            tokens.push(Token::Num(num));
        } else {
            tokens.push(Token::Str(current));
        }
    }
    tokens
}

/// Compare two non-numeric tokens according to Debian’s rules:
/// - Compare character by character.
/// - Letters are always considered lower than non-letters.
/// - The tilde character (`~`) sorts even before the end of a token.
fn compare_str_token(a: &str, b: &str) -> Ordering {
    let mut it1 = a.chars();
    let mut it2 = b.chars();

    loop {
        match (it1.next(), it2.next()) {
            (None, None) => return Ordering::Equal,
            // When one string ends, we normally return Less or Greater.
            // However, in Debian version ordering an empty string is considered to sort
            // AFTER any string that starts with '~'. (This is what makes "1.0~beta" sort
            // before "1.0".)
            (None, Some(c2)) => {
                if c2 == '~' {
                    return Ordering::Greater;
                } else {
                    return Ordering::Less;
                }
            }
            (Some(c1), None) => {
                if c1 == '~' {
                    return Ordering::Less;
                } else {
                    return Ordering::Greater;
                }
            }
            (Some(c1), Some(c2)) => {
                if c1 == c2 {
                    continue;
                }
                // Special handling for '~'
                if c1 == '~' || c2 == '~' {
                    if c1 == '~' && c2 == '~' {
                        continue;
                    } else if c1 == '~' {
                        return Ordering::Less;
                    } else {
                        return Ordering::Greater;
                    }
                }
                // Letters are considered lower than non-letters.
                let is_letter1 = c1.is_alphabetic();
                let is_letter2 = c2.is_alphabetic();
                if is_letter1 != is_letter2 {
                    return if is_letter1 { Ordering::Less } else { Ordering::Greater };
                }
                // Fallback: compare by ASCII value.
                return c1.cmp(&c2);
            }
        }
    }
}

/// Compare two tokens.
fn compare_tokens(a: &Token, b: &Token) -> Ordering {
    match (a, b) {
        (Token::Num(n1), Token::Num(n2)) => n1.cmp(n2),
        (Token::Str(s1), Token::Str(s2)) => compare_str_token(s1, s2),
        // In practice, token types should alternate.
        (Token::Num(_), Token::Str(_)) => Ordering::Greater,
        (Token::Str(_), Token::Num(_)) => Ordering::Less,
    }
}

impl Ord for OpamVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let tokens1 = self.tokenize();
        let tokens2 = other.tokenize();
        let max = tokens1.len().max(tokens2.len());

        for i in 0..max {
            let token1 = tokens1.get(i);
            let token2 = tokens2.get(i);
            let ord = match (token1, token2) {
                (Some(t1), Some(t2)) => compare_tokens(t1, t2),
                // When self runs out of tokens, treat it as an empty string.
                (None, Some(t2)) => {
                    // Compare an empty string with t2.
                    if let Token::Str(s2) = t2 {
                        // An empty string is considered greater than any string that starts with '~'
                        if s2.starts_with('~') {
                            Ordering::Greater
                        } else {
                            Ordering::Less
                        }
                    } else {
                        Ordering::Less
                    }
                },
                // When other runs out of tokens.
                (Some(t1), None) => {
                    if let Token::Str(s1) = t1 {
                        if s1.starts_with('~') {
                            Ordering::Less
                        } else {
                            Ordering::Greater
                        }
                    } else {
                        Ordering::Greater
                    }
                },
                (None, None) => Ordering::Equal,
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for OpamVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for OpamVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(OpamVersion(s.to_string()))
    }
}

impl fmt::Display for OpamVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_starts_with_digit() {
        let version = OpamVersion("1.2".to_string());
        let tokens = version.tokenize();
        // Expected tokens: [Token::Str(""), Token::Num(1), Token::Str("."), Token::Num(2)]
        assert_eq!(tokens.len(), 4);

        match &tokens[0] {
            Token::Str(s) => assert!(s.is_empty(), "Expected first token to be empty"),
            _ => panic!("Expected first token to be a string"),
        }
        match tokens[1] {
            Token::Num(n) => assert_eq!(n, 1),
            _ => panic!("Expected second token to be a number"),
        }
        match &tokens[2] {
            Token::Str(s) => assert_eq!(s, "."),
            _ => panic!("Expected third token to be a string"),
        }
        match tokens[3] {
            Token::Num(n) => assert_eq!(n, 2),
            _ => panic!("Expected fourth token to be a number"),
        }
    }

    #[test]
    fn test_ordering() {
        // Example ordering from the Opam documentation:
        // "~~", "~", "~beta2", "~beta10", "0.1", "1.0~beta", "1.0", "1.0-test",
        // "1.0.1", "1.0.10", "dev", "trunk"
        let mut versions = vec![
            OpamVersion("1.0-test".to_string()),
            OpamVersion("1.0.10".to_string()),
            OpamVersion("1.0~beta".to_string()),
            OpamVersion("1.0".to_string()),
            OpamVersion("~beta2".to_string()),
            OpamVersion("trunk".to_string()),
            OpamVersion("0.1".to_string()),
            OpamVersion("dev".to_string()),
            OpamVersion("~~".to_string()),
            OpamVersion("1.0.1".to_string()),
            OpamVersion("~".to_string()),
            OpamVersion("~beta10".to_string()),
        ];

        versions.sort();

        let expected_order = vec![
            "~~",
            "~",
            "~beta2",
            "~beta10",
            "0.1",
            "1.0~beta",
            "1.0",
            "1.0-test",
            "1.0.1",
            "1.0.10",
            "dev",
            "trunk",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

        let sorted_versions = versions
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>();

        assert_eq!(sorted_versions, expected_order);
    }

    #[test]
    fn test_comparison_specific() {
        let v1 = OpamVersion("1.0~beta".to_string());
        let v2 = OpamVersion("1.0".to_string());
        // We expect "1.0~beta" to be less than "1.0"
        assert!(v1 < v2, "Expected '1.0~beta' to be less than '1.0'");
    }
}
