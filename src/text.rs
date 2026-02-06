use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq)]
pub struct Text(pub String);

impl Text {
    pub fn validated(&self) -> Result<&Self> {
        static TEXT_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let sentence_regex = TEXT_REGEX.get_or_init(|| {
            Regex::new(r#"^(?:[\p{Script=Cyrillic}\s,-]+|@[A-Za-z0-9-_]{22})+(?:\s+(?:[\p{Script=Cyrillic}\s,-]+|@[A-Za-z0-9-_]{22})+)*$|^(?:[\p{Script=Latin}\s,-]+|@[A-Za-z0-9-_]{22})+(?:\s+(?:[\p{Script=Latin}\s,-]+|@[A-Za-z0-9-_]{22})+)*$"#)
                .with_context(|| "Can not compile regular expression for text validation")
                .unwrap()
        });
        if sentence_regex.is_match(&self.0) {
            Ok(self)
        } else {
            Err(anyhow!(
                "Text must be one English or Russian sentence: letters, whitespaces, ',' and '-', so {:?} does not seem to be text",
                self.0
            ))
        }
    }
}
