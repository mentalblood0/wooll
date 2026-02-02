use anyhow::{Context, Result, anyhow};
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use trove::{Chest, ChestConfig, Object, ObjectId};

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode)]
pub struct Mention {
    pub mentioned: ObjectId,
    pub inside: ObjectId,
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

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq)]
pub struct Text(String);

static TEXT_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

impl Text {
    pub fn validate(&self) -> Result<()> {
        let sentence_regex = TEXT_REGEX.get_or_init(|| {
            Regex::new(r#"^(?=.*[a-zA-Zа-яА-ЯёЁ])[a-zA-Zа-яА-ЯёЁ\s,;:'"\-–—()]+$"#)
                .with_context(|| "Can not compile regular expression for text validation")
                .unwrap()
        });
        if sentence_regex
            .is_match(&self.0)
            .with_context(|| "Regex matching failed")?
        {
            Ok(())
        } else {
            Err(anyhow!(
                "Text must be one English or Russian sentence with no punctuation at the end"
            ))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq)]
pub struct RelationKind(String);

static RELATION_KIND_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

impl RelationKind {
    pub fn validate(&self) -> Result<()> {
        let sentence_regex = RELATION_KIND_REGEX.get_or_init(|| {
            Regex::new(r"^(?=.*[a-zA-Z])[a-zA-Z\s]+$")
                .with_context(|| "Can not compile regular expression for relation kind validation")
                .unwrap()
        });
        if sentence_regex
            .is_match(&self.0)
            .with_context(|| "Regex matching failed")?
        {
            Ok(())
        } else {
            Err(anyhow!(
                "Relation kind must be an English words sequence without punctuation"
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
    pub fn validate(&self) -> Result<()> {
        self.kind.validate()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq)]
pub enum Content {
    Text(Text),
    Relation(Relation),
}

impl Content {
    pub fn id(&self) -> Result<ObjectId> {
        Ok(ObjectId {
            value: xxhash_rust::xxh3::xxh3_128(&bincode::encode_to_vec(self, bincode::config::standard()).with_context(|| format!("Can not binary encode Content {self:?} in order to compute it's ObjectId as it's binary representation hash"))?).to_be_bytes(),
        })
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Content::Text(text) => text.validate(),
            Content::Relation(relation) => relation.validate(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Tag(String);

static TAG_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

impl Tag {
    pub fn validate(&self) -> Result<()> {
        let tag_regex = TAG_REGEX.get_or_init(|| {
            Regex::new(r"^\w+$")
                .with_context(|| "Can not compile regular expression for tag validation")
                .unwrap()
        });
        if tag_regex
            .is_match(&self.0)
            .with_context(|| "Regex matching failed")?
        {
            Ok(())
        } else {
            Err(anyhow!("Tag must be a word symbols sequence"))
        }
    }
}

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

    pub fn validate(&self) -> Result<()> {
        self.content.validate()?;
        for tag in self.tags.iter() {
            tag.validate()?;
        }
        Ok(())
    }
}

pub struct Sweater {
    pub chest: Chest,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SweaterConfig {
    pub chest: ChestConfig,
}

pub struct WriteTransaction<'a, 'b, 'c, 'd> {
    pub chest_transaction: &'a mut trove::WriteTransaction<'b, 'c, 'd>,
}

pub struct ReadTransaction<'a> {
    pub chest_transaction: &'a trove::ReadTransaction<'a>,
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

    pub fn lock_all_and_write<'a, F>(&'a mut self, mut f: F) -> Result<&'a mut Self>
    where
        F: FnMut(&mut WriteTransaction<'_, '_, '_, '_>) -> Result<()>,
    {
        self.chest
            .lock_all_and_write(|chest_write_transaction| {
                f(&mut WriteTransaction {
                    chest_transaction: chest_write_transaction,
                })
            })
            .with_context(|| "Can not lock chest and initiate write transaction")?;

        Ok(self)
    }

    pub fn lock_all_writes_and_read<F>(&self, mut f: F) -> Result<&Self>
    where
        F: FnMut(ReadTransaction) -> Result<()>,
    {
        self.chest
            .lock_all_writes_and_read(|chest_read_transaction| {
                f(ReadTransaction {
                    chest_transaction: &chest_read_transaction,
                })
            })
            .with_context(
                || "Can not lock all write operations on chest and initiate read transaction",
            )?;
        Ok(self)
    }
}

macro_rules! define_read_methods {
    () => {
        pub fn get_thesis(&self, thesis_id: &ObjectId) -> Result<Option<Thesis>> {
            if let Some(thesis_json_value) = self.chest_transaction.get(thesis_id, &vec![])? {
                Ok(serde_json::from_value(thesis_json_value)?)
            } else {
                Ok(None)
            }
        }
    };
}

impl WriteTransaction<'_, '_, '_, '_> {
    define_read_methods!();

    pub fn insert_thesis(&mut self, thesis: Thesis) -> Result<()> {
        let thesis_id = thesis.id()?;
        if self.chest_transaction.contains_object_with_id(&thesis_id)? {
            Err(anyhow!(
                "Can not insert thesis {thesis:?} with id {thesis_id:?} as chest already contains object with such id"
            ))
        } else {
            self.chest_transaction.insert_with_id(Object {
                id: thesis_id,
                value: serde_json::to_value(thesis)?,
            })?;
            Ok(())
        }
    }
}

impl ReadTransaction<'_> {
    define_read_methods!();
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use nanorand::{Rng, WyRand};

    use super::*;
    use pretty_assertions::assert_eq;

    fn new_default_sweater(test_name_for_isolation: &str) -> Sweater {
        Sweater::new(
            serde_saphyr::from_str(
                &std::fs::read_to_string("src/test_sweater_config.yml")
                    .unwrap()
                    .replace("TEST_NAME", test_name_for_isolation),
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn random_text(rng: &mut WyRand) -> Text {
        const LETTERS: &str = "abcdefghijklmnopqrstuvwxyzабвгдеёжзийклмнопрстуфхцчшщъыьэюя";
        const PUNCTUATION: &[&str] = &[", ", "; ", ": ", " - ", " (", ") "];
        let word_count = rng.generate_range(3..=10);
        let letters: Vec<char> = LETTERS.chars().collect();
        let words: Vec<String> = (0..word_count)
            .map(|_| {
                let len = rng.generate_range(2..=8);
                (0..len)
                    .map(|_| letters[rng.generate_range(0..letters.len())])
                    .collect()
            })
            .collect();
        let mut result = String::new();
        for (i, word) in words.iter().enumerate() {
            result.push_str(word);
            if i < words.len() - 1 {
                if rng.generate_range(0..3) == 0 {
                    result.push_str(PUNCTUATION[rng.generate_range(0..PUNCTUATION.len())]);
                } else {
                    result.push(' ');
                }
            }
        }
        Text(result)
    }

    fn random_relation_kind(rng: &mut WyRand) -> RelationKind {
        const LETTERS: &str = "abcdefghijklmnopqrstuvwxyz";
        let word_count = rng.generate_range(1..=5);
        let letters: Vec<char> = LETTERS.chars().collect();
        let words: Vec<String> = (0..word_count)
            .map(|_| {
                let len = rng.generate_range(2..=8);
                (0..len)
                    .map(|_| letters[rng.generate_range(0..letters.len())])
                    .collect()
            })
            .collect();
        RelationKind(words.join(" "))
    }

    #[test]
    fn test_generative() {
        let mut sweater = new_default_sweater("test_generative");
        let mut rng = WyRand::new_seed(0);

        random_text(&mut rng).validate().unwrap();

        sweater
            .lock_all_and_write(|transaction| {
                let mut previously_added_theses: BTreeMap<ObjectId, Thesis> = BTreeMap::new();
                for _ in 0..100 {
                    let action_id = if previously_added_theses.is_empty() {
                        1
                    } else {
                        rng.generate_range(1..=3)
                    };
                    match action_id {
                        1 => {
                            let thesis =
                                Thesis {
                                    content: match rng.generate_range(1..2) {
                                        1 => Content::Text(random_text(&mut rng)),
                                        2 => Content::Relation(Relation {
                                            from: previously_added_theses
                                                .keys()
                                                .nth(rng.generate_range(
                                                    0..previously_added_theses.len(),
                                                ))
                                                .unwrap()
                                                .clone(),
                                            to: previously_added_theses
                                                .keys()
                                                .nth(rng.generate_range(
                                                    0..previously_added_theses.len(),
                                                ))
                                                .unwrap()
                                                .clone(),
                                            kind: random_relation_kind(&mut rng),
                                        }),
                                        _ => {
                                            panic!()
                                        }
                                    },
                                    tags: vec![],
                                };
                            thesis.validate().unwrap();
                            transaction.insert_thesis(thesis.clone()).unwrap();
                            previously_added_theses.insert(thesis.id().unwrap(), thesis);
                        }
                        _ => {}
                    }
                    for (thesis_id, thesis) in previously_added_theses.iter() {
                        assert_eq!(transaction.get_thesis(thesis_id).unwrap().unwrap(), *thesis);
                    }
                }
                Ok(())
            })
            .unwrap();
    }
}
