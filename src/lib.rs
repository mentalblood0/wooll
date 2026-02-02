mod id_serializer;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use trove::{Chest, ChestConfig, ObjectId};

pub struct Sweater {
    pub chest: Chest,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SweaterConfig {
    pub chest: ChestConfig,
}

impl Sweater {
    pub fn new(config: SweaterConfig) -> Result<Self> {
        Ok(Self {
            chest: Chest::new(config.chest.clone()).with_context(|| {
                format!(
                    "Can not create sweater with chest config {:?}",
                    config.chest
                )
            })?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode)]
pub struct Mention {
    mentioned: ObjectId,
    inside: ObjectId,
}

impl Mention {
    pub fn id(&self) -> Result<ObjectId> {
        Ok(ObjectId {
            value: xxhash_rust::xxh3::xxh3_128(
                &bincode::encode_to_vec(self, bincode::config::standard())
                    .with_context(|| format!("Can not binary encode Mention {self:?} in order to compute it's ObjectId as it's binary representation hash"))?,
            )
            .to_be_bytes(),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode)]
pub struct Text(String);

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode)]
pub struct RelationKind(String);

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode)]
pub struct Relation {
    from: ObjectId,
    to: ObjectId,
    kind: RelationKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode)]
pub enum Content {
    Text(Text),
    Relation(Relation),
}

impl Content {
    pub fn id(&self) -> Result<ObjectId> {
        Ok(ObjectId {
            value: xxhash_rust::xxh3::xxh3_128(
                &bincode::encode_to_vec(self, bincode::config::standard())
                    .with_context(|| format!("Can not binary encode Content {self:?} in order to compute it's ObjectId as it's binary representation hash"))?,
            )
            .to_be_bytes(),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tag(String);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Thesis {
    pub content: Content,
    pub tags: Vec<Tag>,
}

impl Thesis {
    pub fn id(&self) -> Result<ObjectId> {
        self.content.id()
    }
}
