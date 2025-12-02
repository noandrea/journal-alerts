use anyhow::Result;

/// A struct that holds compiled regex patterns and can find matches in log lines.
pub struct Matcher {
    // A vector of tuples containing the index of the pattern and the compiled regex.
    patterns: Vec<(usize, regex::Regex)>,
}

impl Matcher {
    pub fn new(patterns: &[String]) -> Result<Self> {
        // Compile the regex patterns and store them with their indices.
        let patterns = patterns
            .iter()
            .enumerate()
            .map(|(i, rule)| {
                let re = regex::Regex::new(rule)
                    .map_err(|e| anyhow::anyhow!("Invalid regex pattern '{}': {}", rule, e))?;
                Ok((i, re))
            })
            .collect::<Result<Vec<(usize, regex::Regex)>>>()?;
        Ok(Matcher { patterns })
    }

    /// Finds the first matching pattern for the given log line.
    pub fn find_match(&self, line: &str) -> Option<(usize, String)> {
        // Check each pattern to see if it matches the given line.
        for (i, re) in &self.patterns {
            if re.is_match(line) {
                return Some((*i, line.into()));
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
        let rules = ["error", "warn", "(?i)quorum not reached"];

        let matcher =
            Matcher::new(&rules.iter().map(|s| s.to_string()).collect::<Vec<String>>()).unwrap();

        let tests = vec![
            (
                "This is an error message",
                Some((0, "This is an error message".to_string())),
            ),
            (
                "This is a warn message",
                Some((1, "This is a warn message".to_string())),
            ),
            (
                "Quorum not reached in the cluster",
                Some((2, "Quorum not reached in the cluster".to_string())),
            ),
            ("All systems operational", None),
        ];

        for (input, expected) in tests {
            assert_eq!(matcher.find_match(input), expected);
        }
    }
}
