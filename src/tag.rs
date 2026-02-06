use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Tag(pub String);

impl Tag {
    pub fn validated(&self) -> Result<&Self> {
        static TAG_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let tag_regex = TAG_REGEX.get_or_init(|| {
            Regex::new(r"^\w+$")
                .with_context(|| "Can not compile regular expression for tag validation")
                .unwrap()
        });
        if tag_regex.is_match(&self.0) {
            Ok(self)
        } else {
            Err(anyhow!(
                "Tag must be a word symbols sequence, so {:?} does not seem to be tag",
                self.0
            ))
        }
    }
}
