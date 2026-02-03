use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};
use fallible_iterator::FallibleIterator;
use regex::Regex;
use serde::{Deserialize, Serialize};
use trove::{Chest, ChestConfig, IndexRecordType, Object, ObjectId, PathSegment, path_segments};

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
            Regex::new(r#"(:?^[\p{Script=Cyrillic}\s,-]+)|(:?[\p{Script=Latin}\s,-]+)$"#)
                .with_context(|| "Can not compile regular expression for text validation")
                .unwrap()
        });
        if sentence_regex.is_match(&self.0) {
            Ok(())
        } else {
            Err(anyhow!(
                "Text must be one English or Russian sentence: letters, whitespaces, ',' and '-', so {:?} does not seem to be text",
                self.0
            ))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelationKind(String);

static RELATION_KIND_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

impl RelationKind {
    pub fn validate(&self) -> Result<()> {
        let sentence_regex = RELATION_KIND_REGEX.get_or_init(|| {
            Regex::new(r"^[\w\s]+$")
                .with_context(|| "Can not compile regular expression for relation kind validation")
                .unwrap()
        });
        if sentence_regex.is_match(&self.0) {
            Ok(())
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
    pub fn validate(&self) -> Result<()> {
        self.kind.validate()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq)]
pub enum Content {
    Text(Text),
    Relation(Relation),
}

static MENTION_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

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
        if tag_regex.is_match(&self.0) {
            Ok(())
        } else {
            Err(anyhow!(
                "Tag must be a word symbols sequence, so {:?} does not seem to be tag",
                self.0
            ))
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

    pub fn mentions(&self) -> Result<Vec<Mention>> {
        match self.content {
            Content::Text(Text(ref text)) => {
                let mention_regex = MENTION_REGEX.get_or_init(|| {
            Regex::new(
                r"@((?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=|[A-Za-z0-9+/]{4}))[ ,$]",
            )
            .with_context(|| "Can not compile regular expression to search text for mentions")
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SweaterConfig {
    pub chest: ChestConfig,
    pub supported_relations_kinds: BTreeSet<RelationKind>,
}

pub struct Sweater {
    pub chest: Chest,
    pub config: SweaterConfig,
}

pub struct WriteTransaction<'a, 'b, 'c, 'd> {
    pub chest_transaction: &'a mut trove::WriteTransaction<'b, 'c, 'd>,
    pub sweater_config: SweaterConfig,
}

pub struct ReadTransaction<'a> {
    pub chest_transaction: &'a trove::ReadTransaction<'a>,
    pub sweater_config: SweaterConfig,
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
            config: config,
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
                    sweater_config: self.config.clone(),
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
                    sweater_config: self.config.clone(),
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
                Ok(Some(serde_json::from_value(thesis_json_value).unwrap()))
            } else {
                Ok(None)
            }
        }
    };
}

impl WriteTransaction<'_, '_, '_, '_> {
    define_read_methods!();

    pub fn where_mentioned(&self, thesis_id: &ObjectId) -> Result<Vec<ObjectId>> {
        self.chest_transaction
            .select(
                &vec![(
                    IndexRecordType::Direct,
                    path_segments!("mentioned"),
                    serde_json::to_value(thesis_id)?.try_into()?,
                )],
                &vec![],
                None,
            )?
            .map(|mention_id| {
                Ok(serde_json::from_value(
                    self.chest_transaction
                        .get(&mention_id, &path_segments!("inside"))?
                        .with_context(|| {
                            format!(
                                "Can not get field 'inside' of mention with {:?}",
                                mention_id
                            )
                        })?,
                )?)
            })
            .collect()
    }

    pub fn insert_thesis(&mut self, thesis: Thesis) -> Result<()> {
        let thesis_id = thesis.id()?;
        if self.chest_transaction.contains_object_with_id(&thesis_id)? {
            Err(anyhow!(
                "Can not insert thesis {thesis:?} with id {thesis_id:?} as chest already contains object with such id"
            ))
        } else {
            if let Content::Relation(Relation {
                from: ref from_id,
                to: ref to_id,
                kind: ref relation_kind,
            }) = thesis.content
            {
                if !self
                    .sweater_config
                    .supported_relations_kinds
                    .contains(&relation_kind)
                {
                    return Err(anyhow!(
                        "Can not insert relation {thesis:?} of kind {relation_kind:?} in sweater with supported relations kinds {:?} as it's kind is not supported",
                        self.sweater_config.supported_relations_kinds
                    ));
                }
                for related_id in [from_id, to_id] {
                    if self
                        .chest_transaction
                        .get(&related_id, &path_segments!("content"))?
                        .is_none()
                    {
                        return Err(anyhow!(
                            "Can not insert relation {thesis:?} in sweater without inserted thesis with {related_id:?}"
                        ));
                    }
                }
            }
            self.chest_transaction.insert_with_id(Object {
                id: thesis_id,
                value: serde_json::to_value(thesis.clone())?,
            })?;
            for mention in thesis.mentions()? {
                self.chest_transaction.insert_with_id(Object {
                    id: mention.id()?,
                    value: serde_json::to_value(mention)?,
                })?;
            }
            Ok(())
        }
    }

    pub fn tag_thesis(&mut self, thesis_id: &ObjectId, tag: Tag) -> Result<()> {
        if !self.chest_transaction.contains_element(
            thesis_id,
            &path_segments!("tags"),
            &serde_json::to_value(tag.clone())?.try_into()?,
        )? {
            self.chest_transaction.push(
                thesis_id,
                &path_segments!("tags"),
                serde_json::to_value(tag)?,
            )?;
        }
        Ok(())
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

    fn random_text(rng: &mut WyRand, previously_added_theses: &BTreeMap<ObjectId, Thesis>) -> Text {
        const ENGLISH_LETTERS: [&str; 26] = [
            "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q",
            "r", "s", "t", "u", "v", "w", "x", "y", "z",
        ];
        const RUSSIAN_LETTERS: [&str; 33] = [
            "а", "б", "в", "г", "д", "е", "ё", "ж", "з", "и", "й", "к", "л", "м", "н", "о", "п",
            "р", "с", "т", "у", "ф", "х", "ц", "ч", "ш", "щ", "ъ", "ы", "ь", "э", "ю", "я",
        ];
        const PUNCTUATION: &[&str] = &[", "];
        let language = rng.generate_range(1..=2);
        let words: Vec<String> = (0..rng.generate_range(3..=10))
            .map(|_| {
                if previously_added_theses.is_empty() || rng.generate_range(0..=1) == 0 {
                    (0..rng.generate_range(2..=8))
                        .map(|_| {
                            if language == 1 {
                                ENGLISH_LETTERS[rng.generate_range(0..ENGLISH_LETTERS.len())]
                            } else {
                                RUSSIAN_LETTERS[rng.generate_range(0..RUSSIAN_LETTERS.len())]
                            }
                        })
                        .collect()
                } else {
                    format!(
                        "@{}",
                        serde_json::to_value(
                            previously_added_theses
                                .keys()
                                .nth(rng.generate_range(0..previously_added_theses.len()))
                                .unwrap()
                                .clone(),
                        )
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string()
                    )
                }
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

    fn random_tag(rng: &mut WyRand) -> Tag {
        const LETTERS: [&str; 26] = [
            "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q",
            "r", "s", "t", "u", "v", "w", "x", "y", "z",
        ];
        Tag((0..rng.generate_range(2..=8))
            .map(|_| LETTERS[rng.generate_range(0..LETTERS.len())])
            .collect())
    }

    fn random_thesis(
        rng: &mut WyRand,
        previously_added_theses: &BTreeMap<ObjectId, Thesis>,
        transaction: &WriteTransaction,
    ) -> Thesis {
        Thesis {
            content: {
                let action_id = if previously_added_theses.is_empty() {
                    1
                } else {
                    rng.generate_range(1..=2)
                };
                match action_id {
                    1 => Content::Text(random_text(rng, previously_added_theses)),
                    2 => Content::Relation(Relation {
                        from: previously_added_theses
                            .keys()
                            .nth(rng.generate_range(0..previously_added_theses.len()))
                            .unwrap()
                            .clone(),
                        to: previously_added_theses
                            .keys()
                            .nth(rng.generate_range(0..previously_added_theses.len()))
                            .unwrap()
                            .clone(),
                        kind: transaction
                            .sweater_config
                            .supported_relations_kinds
                            .iter()
                            .nth(rng.generate_range(
                                0..transaction.sweater_config.supported_relations_kinds.len(),
                            ))
                            .unwrap()
                            .clone(),
                    }),
                    _ => {
                        panic!()
                    }
                }
            },
            tags: vec![],
        }
    }

    #[test]
    fn test_generative() {
        let mut sweater = new_default_sweater("test_generative");
        let mut rng = WyRand::new_seed(0);

        sweater
            .lock_all_and_write(|transaction| {
                let mut previously_added_theses: BTreeMap<ObjectId, Thesis> = BTreeMap::new();
                for _ in 0..1000 {
                    let action_id = if previously_added_theses.is_empty() {
                        1
                    } else {
                        rng.generate_range(1..=2)
                    };
                    match action_id {
                        1 => {
                            let thesis = {
                                let mut result =
                                    random_thesis(&mut rng, &previously_added_theses, &transaction);
                                while previously_added_theses.contains_key(&result.id().unwrap()) {
                                    result = random_thesis(
                                        &mut rng,
                                        &previously_added_theses,
                                        &transaction,
                                    );
                                }
                                result
                            };
                            thesis.validate().unwrap();
                            transaction.insert_thesis(thesis.clone()).unwrap();
                            let thesis_id = thesis.id().unwrap();
                            assert_eq!(
                                transaction.get_thesis(&thesis_id).unwrap().unwrap(),
                                thesis
                            );
                            assert_eq!(
                                transaction.where_mentioned(&thesis_id)?,
                                thesis
                                    .mentions()?
                                    .into_iter()
                                    .map(|mention| mention.mentioned)
                                    .collect::<Vec<_>>()
                            );
                            previously_added_theses.insert(thesis_id, thesis);
                        }
                        2 => {
                            let tag_to_add = random_tag(&mut rng);
                            let thesis_to_tag_id = previously_added_theses
                                .keys()
                                .nth(rng.generate_range(0..previously_added_theses.len()))
                                .unwrap()
                                .clone();
                            transaction
                                .tag_thesis(&thesis_to_tag_id, tag_to_add.clone())
                                .unwrap();
                            assert!(
                                transaction
                                    .get_thesis(&thesis_to_tag_id)
                                    .unwrap()
                                    .unwrap()
                                    .tags
                                    .contains(&tag_to_add)
                            );
                            previously_added_theses
                                .get_mut(&thesis_to_tag_id)
                                .unwrap()
                                .tags
                                .push(tag_to_add);
                        }
                        _ => {}
                    }
                }
                for (thesis_id, thesis) in previously_added_theses.iter() {
                    assert_eq!(transaction.get_thesis(thesis_id).unwrap().unwrap(), *thesis);
                }
                Ok(())
            })
            .unwrap();
    }
}
