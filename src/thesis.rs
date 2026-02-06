use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use trove::ObjectId;

use super::content::Content;
use super::mention::Mention;
use super::tag::Tag;
use super::text::Text;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Thesis {
    pub content: Content,

    #[serde(default)]
    pub tags: Vec<Tag>,
}

impl Thesis {
    pub fn id(&self) -> Result<ObjectId> {
        self.content.id()
    }

    pub fn validated(&self) -> Result<&Self> {
        self.content.validated()?;
        for tag in self.tags.iter() {
            tag.validated()?;
        }
        Ok(self)
    }

    pub fn mentions(&self) -> Result<Vec<Mention>> {
        match self.content {
            Content::Text(Text(ref text)) => {
                static MENTION_IN_TEXT_REGEX: std::sync::OnceLock<Regex> =
                    std::sync::OnceLock::new();
                let mention_regex = MENTION_IN_TEXT_REGEX.get_or_init(|| {
                    Regex::new(r"@([A-Za-z0-9-_]{22})[ ,$]")
                        .with_context(
                            || "Can not compile regular expression to search text for mentions",
                        )
                        .unwrap()
                });
                let self_id = self.id()?;
                let mut result = vec![];
                for capture in mention_regex.captures_iter(text) {
                    result.push(Mention {
                        mentioned: serde_json::from_str(&format!("\"{}\"", &capture[1]))?,
                        inside: self_id.clone(),
                    });
                }
                result.sort_by_key(|mention| mention.mentioned.clone());
                result.dedup_by_key(|mention| mention.mentioned.clone());
                Ok(result)
            }
            Content::Relation(_) => Ok(vec![]),
        }
    }
}
