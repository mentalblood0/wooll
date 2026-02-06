use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};
use trove::ObjectId;

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelationKind(pub String);

impl RelationKind {
    pub fn validated(&self) -> Result<&Self> {
        static RELATION_KIND_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let sentence_regex = RELATION_KIND_REGEX.get_or_init(|| {
            Regex::new(r"^[\w\s]+$")
                .with_context(|| "Can not compile regular expression for relation kind validation")
                .unwrap()
        });
        if sentence_regex.is_match(&self.0) {
            Ok(self)
        } else {
            Err(anyhow!(
                "Relation kind must be an English words sequence without punctuation, so {:?} does not seem to be relation kind",
                self.0
            ))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq)]
pub struct Relation {
    pub from: ObjectId,
    pub to: ObjectId,
    pub kind: RelationKind,
}

impl Relation {
    pub fn validated(&self) -> Result<&Self> {
        self.kind.validated()?;
        Ok(self)
    }
}
