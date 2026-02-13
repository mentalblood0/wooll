use anyhow::Result;
use serde::{Deserialize, Serialize};
use trove::ObjectId;

use crate::alias::Alias;
use crate::content::Content;
use crate::relation::Relation;
use crate::tag::Tag;
use crate::text::Text;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Thesis {
    pub alias: Option<Alias>,
    pub content: Content,

    #[serde(default)]
    pub tags: Vec<Tag>,
}

impl Thesis {
    pub fn id(&self) -> Result<ObjectId> {
        self.content.id()
    }

    pub fn validated(&self) -> Result<&Self> {
        if let Some(ref alias) = self.alias {
            alias.validated()?;
        }
        self.content.validated()?;
        for tag in self.tags.iter() {
            tag.validated()?;
        }
        Ok(self)
    }

    pub fn references(&self) -> Vec<ObjectId> {
        match self.content {
            Content::Text(Text {
                raw_text_parts: _,
                ref references,
                start_with_reference: _,
            }) => references.clone(),
            Content::Relation(Relation {
                ref from,
                ref to,
                kind: _,
            }) => vec![from.clone(), to.clone()],
        }
    }
}
