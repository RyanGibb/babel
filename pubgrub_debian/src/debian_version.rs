use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DebianVersion(pub String);

impl DebianVersion {
    /// Splits the version string into (epoch, upstream, debian_revision).
    /// If the epoch is absent, it defaults to 0.
    /// If the debian_revision is absent, it defaults to "0".
    fn split(&self) -> (u64, String, String) {
        // Trim whitespace.
        let s = self.0.trim();
        // Check for an epoch: look for ':'.
        let (epoch, rest) = if let Some(pos) = s.find(':') {
            let epoch_str = &s[..pos];
            let epoch = epoch_str.parse::<u64>().unwrap_or(0);
            (epoch, &s[pos + 1..])
        } else {
            (0, s)
        };
        // For debian_revision, split at the *last* hyphen.
        let (upstream, debian) = if let Some(pos) = rest.rfind('-') {
            let upstream = &rest[..pos];
            let debian = &rest[pos + 1..];
            (upstream, debian)
        } else {
            (rest, "0")
        };
        (epoch, upstream.to_string(), debian.to_string())
    }

    /// Tokenizes a version component (either upstream or debian) into alternating
    /// non-digit and digit tokens.
    fn tokenize_str(s: &str) -> Vec<Token> {
        tokenize(s)
    }
}

/// A token is either a numeric token or a non-numeric string token.
#[derive(Debug, PartialEq, Eq)]
enum Token {
    Num(u64),
    Str(String),
}

/// Tokenize a version string into alternating non-digit and digit tokens.
/// Always starts with a non-digit token (insert an empty token if needed).
fn tokenize(version: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut is_digit = None; // Not set until we see the first character

    let mut chars = version.chars().peekable();

    // According to Debian, if the first character is a digit, insert an empty string token.
    if let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            tokens.push(Token::Str(String::new()));
        }
    }

    while let Some(ch) = chars.next() {
        let ch_is_digit = ch.is_ascii_digit();
        match is_digit {
            Some(current_is_digit) if current_is_digit == ch_is_digit => {
                current.push(ch);
            }
            Some(_) => {
                // Type changed: push the current token and start a new one.
                if is_digit.unwrap() {
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

/// Compare two non-digit tokens (strings) according to Debian rules:
/// - Compare character by character.
/// - Letters sort before non-letters.
/// - The tilde `~` sorts before anything (even an empty string).
fn compare_str_token(a: &str, b: &str) -> Ordering {
    let mut it1 = a.chars();
    let mut it2 = b.chars();
    loop {
        match (it1.next(), it2.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(c2)) => {
                // When one string is finished, an empty string is considered to be after any token that begins with '~'
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
                // If either character is '~', handle specially.
                if c1 == '~' || c2 == '~' {
                    if c1 == '~' && c2 == '~' {
                        continue;
                    }
                    return if c1 == '~' {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    };
                }
                // Letters always sort before non-letters.
                let is_letter1 = c1.is_alphabetic();
                let is_letter2 = c2.is_alphabetic();
                if is_letter1 != is_letter2 {
                    return if is_letter1 {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    };
                }
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
        (Token::Num(_), Token::Str(_)) => Ordering::Greater,
        (Token::Str(_), Token::Num(_)) => Ordering::Less,
    }
}

impl Ord for DebianVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let (epoch1, upstream1, debian1) = self.split();
        let (epoch2, upstream2, debian2) = other.split();

        // First compare epochs numerically.
        match epoch1.cmp(&epoch2) {
            Ordering::Equal => {}
            non_eq => return non_eq,
        }

        // Compare upstream_version using tokenization.
        let tokens1 = DebianVersion::tokenize_str(&upstream1);
        let tokens2 = DebianVersion::tokenize_str(&upstream2);
        let max = tokens1.len().max(tokens2.len());
        for i in 0..max {
            let token1 = tokens1.get(i);
            let token2 = tokens2.get(i);
            let ord = match (token1, token2) {
                (Some(t1), Some(t2)) => compare_tokens(t1, t2),
                (None, Some(t2)) => {
                    if let Token::Str(s2) = t2 {
                        if s2.starts_with('~') {
                            Ordering::Greater
                        } else {
                            Ordering::Less
                        }
                    } else {
                        Ordering::Less
                    }
                }
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
                }
                (None, None) => Ordering::Equal,
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }

        // Compare debian_revision similarly.
        let tokens1 = DebianVersion::tokenize_str(&debian1);
        let tokens2 = DebianVersion::tokenize_str(&debian2);
        let max = tokens1.len().max(tokens2.len());
        for i in 0..max {
            let token1 = tokens1.get(i);
            let token2 = tokens2.get(i);
            let ord = match (token1, token2) {
                (Some(t1), Some(t2)) => compare_tokens(t1, t2),
                (None, Some(t2)) => {
                    if let Token::Str(s2) = t2 {
                        if s2.starts_with('~') {
                            Ordering::Greater
                        } else {
                            Ordering::Less
                        }
                    } else {
                        Ordering::Less
                    }
                }
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
                }
                (None, None) => Ordering::Equal,
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for DebianVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for DebianVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DebianVersion(s.to_string()))
    }
}

impl fmt::Display for DebianVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_starts_with_digit() {
        let tokens = DebianVersion::tokenize_str("1.2");
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
        // Example ordering from the documentation:
        // "~~", "~", "~beta2", "~beta10", "0.1", "1.0~beta", "1.0", "1.0-test",
        // "1.0.1", "1.0.10", "dev", "trunk"
        let mut versions = vec![
            DebianVersion("1.0-test".to_string()),
            DebianVersion("1.0.10".to_string()),
            DebianVersion("1.0~beta".to_string()),
            DebianVersion("1.0".to_string()),
            DebianVersion("~beta2".to_string()),
            DebianVersion("trunk".to_string()),
            DebianVersion("0.1".to_string()),
            DebianVersion("dev".to_string()),
            DebianVersion("~~".to_string()),
            DebianVersion("1.0.1".to_string()),
            DebianVersion("~".to_string()),
            DebianVersion("~beta10".to_string()),
        ];

        versions.sort();

        let expected_order = vec![
            "~~", "~", "~beta2", "~beta10", "0.1", "1.0~beta", "1.0", "1.0-test", "1.0.1",
            "1.0.10", "dev", "trunk",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

        let sorted_versions = versions.iter().map(|v| v.to_string()).collect::<Vec<_>>();

        assert_eq!(sorted_versions, expected_order);
    }

    #[test]
    fn test_comparison_specific() {
        let v1 = DebianVersion("1.0~beta".to_string());
        let v2 = DebianVersion("1.0".to_string());
        // We expect "1.0~beta" to be less than "1.0"
        assert!(v1 < v2, "Expected '1.0~beta' to be less than '1.0'");
    }
}
