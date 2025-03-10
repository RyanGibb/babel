use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct AlpineVersion(pub String);

#[derive(Debug, PartialEq, Eq, Clone)]
enum Token {
    Num(String),
    Str(String),
    Suffix(SuffixType),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
enum SuffixType {
    Alpha,
    Beta,
    Pre,
    Rc,
    None,
    Patch,
    Rev,
    Git,
    Svn,
    Cvs,
    Hg,
}

fn tokenize(version: &str) -> Vec<Token> {
    if version.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut chars = version.chars().peekable();
    let mut current = String::new();
    let mut is_digit = None; // Not set until we see the first character

    while let Some(ch) = chars.next() {
        // Handle special suffix marker '_'
        if ch == '_' {
            // Push the current token if any
            if !current.is_empty() {
                if is_digit.unwrap_or(false) {
                    tokens.push(Token::Num(current.clone()));
                } else {
                    tokens.push(Token::Str(current.clone()));
                }
                current.clear();
            }

            // Collect suffix
            let mut suffix_str = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphabetic() {
                    suffix_str.push(c);
                    chars.next();
                } else {
                    break;
                }
            }

            // Valid suffix
            match suffix_str.as_str() {
                "alpha" => tokens.push(Token::Suffix(SuffixType::Alpha)),
                "beta" => tokens.push(Token::Suffix(SuffixType::Beta)),
                "pre" => tokens.push(Token::Suffix(SuffixType::Pre)),
                "rc" => tokens.push(Token::Suffix(SuffixType::Rc)),
                "p" => tokens.push(Token::Suffix(SuffixType::Patch)),
                "r" => tokens.push(Token::Suffix(SuffixType::Rev)),
                "git" => tokens.push(Token::Suffix(SuffixType::Git)),
                "svn" => tokens.push(Token::Suffix(SuffixType::Svn)),
                "cvs" => tokens.push(Token::Suffix(SuffixType::Cvs)),
                "hg" => tokens.push(Token::Suffix(SuffixType::Hg)),
                _ => {
                    // Unknown suffix, treat as string
                    tokens.push(Token::Str(format!("_{}", suffix_str)));
                }
            }

            is_digit = None;
            continue;
        }

        // Handle revision marker '-r'
        if ch == '-' && chars.peek() == Some(&'r') {
            // Push the current token if any
            if !current.is_empty() {
                if is_digit.unwrap_or(false) {
                    tokens.push(Token::Num(current.clone()));
                } else {
                    tokens.push(Token::Str(current.clone()));
                }
                current.clear();
            }

            chars.next(); // Skip 'r'
            tokens.push(Token::Str("-r".to_string()));
            is_digit = None;
            continue;
        }

        // Normal character processing
        let ch_is_digit = ch.is_ascii_digit();

        match is_digit {
            Some(current_is_digit) if current_is_digit == ch_is_digit => {
                // Continue current token
                current.push(ch);
            }
            Some(_) => {
                // Type changed: push current token and start a new one
                if !current.is_empty() {
                    if is_digit.unwrap() {
                        tokens.push(Token::Num(current.clone()));
                    } else {
                        tokens.push(Token::Str(current.clone()));
                    }
                    current.clear();
                }
                current.push(ch);
                is_digit = Some(ch_is_digit);
            }
            None => {
                // First character or after special token
                current.push(ch);
                is_digit = Some(ch_is_digit);
            }
        }
    }

    // Push final token if any
    if !current.is_empty() {
        if is_digit.unwrap_or(false) {
            tokens.push(Token::Num(current.clone()));
        } else {
            tokens.push(Token::Str(current.clone()));
        }
    }

    tokens
}

impl Ord for AlpineVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0.is_empty() && other.0.is_empty() {
            return Ordering::Equal;
        }
        if self.0.is_empty() || other.0.is_empty() {
            if self.0.is_empty() {
                return Ordering::Less;
            } else {
                return Ordering::Greater;
            }
        }

        let tokens_a = tokenize(&self.0);
        let tokens_b = tokenize(&other.0);

        let max_len = tokens_a.len().max(tokens_b.len());
        for i in 0..max_len {
            match (tokens_a.get(i), tokens_b.get(i)) {
                (Some(a), Some(b)) => {
                    match (a, b) {
                        (Token::Num(s1), Token::Num(s2)) => {
                            match (s1.chars().next(), s2.chars().next()) {
                                // either begins with a leading zero, and this isn't the initial digit, compare lexigraphically
                                (Some('0'), _) | (_, Some('0')) if i != 0 => match s1.cmp(s2) {
                                    Ordering::Equal => continue,
                                    ord => return ord,
                                },
                                // otherwise, compare numerically
                                _ => {
                                    let n1 = s1.parse::<u64>().unwrap_or(0);
                                    let n2 = s2.parse::<u64>().unwrap_or(0);
                                    match n1.cmp(&n2) {
                                        Ordering::Equal => continue,
                                        ord => return ord,
                                    }
                                }
                            }
                        }
                        (Token::Str(s1), Token::Str(s2)) => {
                            // For strings, compare lexicographically
                            match s1.cmp(s2) {
                                Ordering::Equal => continue,
                                ord => return ord,
                            }
                        }
                        (Token::Suffix(s1), Token::Suffix(s2)) => {
                            // For suffixes, compare by type
                            match s1.cmp(s2) {
                                Ordering::Equal => continue,
                                ord => return ord,
                            }
                        }
                        // Different token types
                        (Token::Suffix(s), _) if *s < SuffixType::None => {
                            // Pre-release suffixes sort before everything
                            return Ordering::Less;
                        }
                        (_, Token::Suffix(s)) if *s < SuffixType::None => {
                            // Everything sorts after pre-release suffixes
                            return Ordering::Greater;
                        }
                        (Token::Num(_), Token::Str(_)) => return Ordering::Greater,
                        (Token::Str(_), Token::Num(_)) => return Ordering::Less,
                        (Token::Num(_), Token::Suffix(_)) => return Ordering::Greater,
                        (Token::Suffix(_), Token::Num(_)) => return Ordering::Less,
                        (Token::Str(_), Token::Suffix(_)) => return Ordering::Greater,
                        (Token::Suffix(_), Token::Str(_)) => return Ordering::Less,
                    }
                }
                (None, Some(Token::Suffix(s))) if *s < SuffixType::None => {
                    // If the shorter version is followed by a pre-release suffix in the longer version,
                    // the shorter version is greater (e.g., "1.0" > "1.0_alpha")
                    return Ordering::Greater;
                }
                (Some(Token::Suffix(s)), None) if *s < SuffixType::None => {
                    // If the longer version has a pre-release suffix, it's less than the shorter version
                    return Ordering::Less;
                }
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (None, None) => unreachable!(),
            }
        }

        Ordering::Equal
    }
}

impl PartialOrd for AlpineVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for AlpineVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(AlpineVersion(s.to_string()))
    }
}

impl fmt::Display for AlpineVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::Path;

    #[test]
    fn test_tokenization() {
        let tokens = tokenize("1.2.3_alpha4");

        assert_eq!(tokens.len(), 7);
        assert_eq!(tokens[0], Token::Num("1".to_string()));
        assert_eq!(tokens[1], Token::Str(".".to_string()));
        assert_eq!(tokens[2], Token::Num("2".to_string()));
        assert_eq!(tokens[3], Token::Str(".".to_string()));
        assert_eq!(tokens[4], Token::Num("3".to_string()));
        assert_eq!(tokens[5], Token::Suffix(SuffixType::Alpha));
        assert_eq!(tokens[6], Token::Num("4".to_string()));
    }

    #[test]
    fn test_version_comparison() {
        let versions = [
            "1.0_alpha",
            "1.0_beta",
            "1.0_pre",
            "1.0_rc",
            "1.0",
            "1.0_p1",
            "1.0-r1",
            "1.0.1",
            "1.1",
        ];

        for (i, v1) in versions.iter().enumerate() {
            for (j, v2) in versions.iter().enumerate() {
                let a: AlpineVersion = v1.parse().unwrap();
                let b: AlpineVersion = v2.parse().unwrap();

                let expected = i.cmp(&j);
                let actual = a.cmp(&b);

                assert_eq!(
                    actual, expected,
                    "Comparing {} and {} resulted in {:?}, expected {:?}",
                    v1, v2, actual, expected
                );
            }
        }
    }

    #[test]
    fn test_specific_comparisons() {
        // Test cases based on Alpine version comparison rules
        assert!(AlpineVersion("1.0".to_string()) < AlpineVersion("1.1".to_string()));
        assert!(AlpineVersion("1.0_alpha".to_string()) < AlpineVersion("1.0".to_string()));
        assert!(AlpineVersion("1.0_beta".to_string()) < AlpineVersion("1.0".to_string()));
        assert!(AlpineVersion("1.0_pre".to_string()) < AlpineVersion("1.0".to_string()));
        assert!(AlpineVersion("1.0_rc".to_string()) < AlpineVersion("1.0".to_string()));
        assert!(AlpineVersion("1.0".to_string()) < AlpineVersion("1.0_p1".to_string()));
        assert!(AlpineVersion("1.0".to_string()) < AlpineVersion("1.0-r1".to_string()));
        assert!(AlpineVersion("1.0_alpha".to_string()) < AlpineVersion("1.0_beta".to_string()));
        assert!(AlpineVersion("1.0.0".to_string()) < AlpineVersion("1.0.1".to_string()));
        assert!(AlpineVersion("".to_string()) < AlpineVersion("1.0".to_string()));

        // Test equality
        assert_eq!(
            AlpineVersion("1.0".to_string()),
            AlpineVersion("1.0".to_string())
        );
        assert_eq!(AlpineVersion("".to_string()), AlpineVersion("".to_string()));
    }

    /// Test versions using data from Alpine's version.data file
    #[test]
    fn test_versions_from_data_file() {
        let file_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/version.data");
        let file = File::open(file_path).expect("Failed to open version.data file");
        let reader = BufReader::new(file);

        let mut errors = 0;

        for (line_num, line) in reader.lines().enumerate() {
            let line = line.expect("Failed to read line");

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Check if test passed
            if !test_one_version_line(&line) {
                errors += 1;
                eprintln!("Failed test at line {}: {}", line_num + 1, line);
            }
        }

        assert_eq!(errors, 0, "{} errors found in version.data tests", errors);
    }

    /// Test a single line from version.data
    ///
    /// This function handles the parsing of a version test line:
    /// - "ver1 op ver2" -> check if comparison is true
    /// - "ver1 !op ver2" -> check if comparison is false
    fn test_one_version_line(line: &str) -> bool {
        // Split by comments
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            return true;
        }

        // Split the line into components
        let parts: Vec<&str> = line.split_whitespace().collect();

        match parts.len() {
            3 => {
                // Comparison test: ver1 op ver2
                let ver1 = AlpineVersion(parts[0].to_string());
                let mut op = parts[1];
                let ver2 = AlpineVersion(parts[2].to_string());

                let invert = op.starts_with('!');
                if invert {
                    op = &op[1..]; // Remove '!' prefix
                }

                let result = match op {
                    "=" | "==" => ver1 == ver2,
                    "!=" | "<>" => ver1 != ver2,
                    "<" => ver1 < ver2,
                    ">" => ver1 > ver2,
                    "<=" => ver1 <= ver2,
                    ">=" => ver1 >= ver2,
                    _ => panic!("Unknown operator: {}", op),
                };

                invert != result
            }
            _ => {
                eprintln!("Invalid test line format: {}", line);
                false
            }
        }
    }
}
