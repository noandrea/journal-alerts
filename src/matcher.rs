use super::config::MatchingRule;
use anyhow::Result;

pub struct Matcher {
    patterns: Vec<(regex::Regex, String)>,
}

impl Matcher {
    pub fn new(rules: &[MatchingRule]) -> Result<Self> {
        let mut patterns = Vec::new();
        for rule in rules {
            let re = regex::Regex::new(&rule.pattern)
                .inspect_err(|e| eprintln!("invalid pattern: {e}"))?;
            patterns.push((re, rule.prefix.clone()));
        }
        Ok(Matcher { patterns })
    }

    pub fn find_match(&self, line: &str) -> Option<String> {
        for (re, prefix) in &self.patterns {
            if re.is_match(line) {
                if prefix.is_empty() {
                    return Some(line.to_string());
                }
                return Some(format!("{prefix}{line}"));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matcher() {
        let rules = vec![
            MatchingRule {
                pattern: "error".to_string(),
                prefix: "ERROR: ".to_string(),
            },
            MatchingRule {
                pattern: "warn".to_string(),
                prefix: "WARNING: ".to_string(),
            },
            MatchingRule {
                pattern: r"(?i)quorum not reached".to_string(),
                prefix: "".to_string(),
            },
        ];

        let matcher = Matcher::new(&rules).unwrap();

        let tests = vec![
            (
                "This is an error message",
                Some("ERROR: This is an error message".to_string()),
            ),
            (
                "This is a warn message",
                Some("WARNING: This is a warn message".to_string()),
            ),
            (
                "Quorum not reached in the cluster",
                Some("Quorum not reached in the cluster".to_string()),
            ),
            ("All systems operational", None),
        ];

        for (input, expected) in tests {
            assert_eq!(matcher.find_match(input), expected);
        }
    }
}
