use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Alias(pub String);

impl Alias {
    pub fn validated(&self) -> Result<&Self> {
        static ALIAS_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let sentence_regex = ALIAS_REGEX.get_or_init(|| {
            Regex::new(r#"^[^\[\]]+$"#)
                .with_context(|| "Can not compile regular expression for thesis alias validation")
                .unwrap()
        });
        if sentence_regex.is_match(&self.0) {
            Ok(self)
        } else {
            Err(anyhow!(
                "Alias must be sequence of one or more non-whitespace characters, so {:?} does not seem to be text",
                self.0
            ))
        }
    }
}
