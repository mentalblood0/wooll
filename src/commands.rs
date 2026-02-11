use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Error, Result, anyhow};
use fallible_iterator::FallibleIterator;
use regex::Regex;
use serde::{Deserialize, Serialize};
use trove::ObjectId;

use crate::alias::Alias;
use crate::content::Content;
use crate::read_transaction::ReadTransactionMethods;
use crate::relation::{Relation, RelationKind};
use crate::tag::Tag;
use crate::text::Text;
use crate::thesis::Thesis;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reference {
    Alias(Alias),
    ObjectId(ObjectId),
}

impl Reference {
    pub fn new(input: &str) -> Result<Self> {
        if let Ok(alias) = Alias(input.to_string()).validated() {
            Ok(Self::Alias(alias.to_owned()))
        } else {
            Ok(Self::ObjectId(serde_json::from_str(&format!(
                "\"{}\"",
                input
            ))?))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AddThesis {
    pub thesis: Thesis,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RemoveThesis {
    pub thesis_id: ObjectId,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AddTag {
    pub thesis_id: ObjectId,
    pub tag: Tag,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RemoveTag {
    pub thesis_id: ObjectId,
    pub tag: Tag,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    AddThesis(AddThesis),
    AddTag(AddTag),
    RemoveThesis(RemoveThesis),
    RemoveTag(RemoveTag),
}

impl Command {
    pub fn validated(&self) -> Result<&Self> {
        match self {
            Command::AddThesis(add_text_thesis) => {
                add_text_thesis.thesis.validated()?;
            }
            Command::RemoveThesis(_) => {}
            Command::AddTag(add_tag) => {
                add_tag.tag.validated()?;
            }
            Command::RemoveTag(remove_tag) => {
                remove_tag.tag.validated()?;
            }
        }
        Ok(self)
    }
}

pub struct CommandsIterator<'a> {
    supported_relations_kinds: &'a BTreeSet<RelationKind>,
    transaction_for_aliases_resolving: Box<dyn ReadTransactionMethods>,
    paragraphs_iterator: Box<dyn FallibleIterator<Item = (usize, &'a str), Error = Error> + 'a>,
    aliases: BTreeMap<Alias, ObjectId>,
}

impl<'a> CommandsIterator<'a> {
    pub fn new(
        input: &'a str,
        supported_relations_kinds: &'a BTreeSet<RelationKind>,
        transaction_for_aliases_resolving: Box<dyn ReadTransactionMethods>,
    ) -> Self {
        static COMMANDS_SPLIT_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let commands_split_regex = COMMANDS_SPLIT_REGEX.get_or_init(|| {
            Regex::new(r#"(\r?\n|\r){2,}"#)
                .with_context(|| "Can not compile regular expression for commands splitting")
                .unwrap()
        });
        Self {
            supported_relations_kinds,
            transaction_for_aliases_resolving,
            paragraphs_iterator: Box::new(fallible_iterator::convert(
                commands_split_regex
                    .split(input)
                    .map(|paragraph| paragraph.trim())
                    .filter(|paragraph| !paragraph.is_empty())
                    .enumerate()
                    .map(|index_and_paragraph| Ok(index_and_paragraph)),
            )),
            aliases: BTreeMap::new(),
        }
    }

    fn get_thesis_id_by_reference(&self, reference: &Reference) -> Result<ObjectId> {
        Ok(match reference {
            Reference::ObjectId(thesis_id) => {
                if self
                    .transaction_for_aliases_resolving
                    .get_thesis(thesis_id)?
                    .is_none()
                {
                    return Err(anyhow!("Can not find thesis with id {thesis_id:?}"));
                }
                thesis_id.clone()
            }
            Reference::Alias(alias) => {
                if let Some(result) = self.aliases.get(alias) {
                    result.clone()
                } else {
                    self.transaction_for_aliases_resolving
                        .get_thesis_id_by_alias(alias)?
                        .ok_or_else(|| anyhow!("Can not find thesis id by alias {alias:?}"))?
                }
            }
        })
    }
}

impl<'a> FallibleIterator for CommandsIterator<'a> {
    type Item = Command;
    type Error = Error;

    fn next(&mut self) -> Result<Option<Self::Item>> {
        if let Some((paragraph_index, paragraph)) = self.paragraphs_iterator.next()? {
            let lines = paragraph.split('\n').collect::<Vec<_>>();
            static COMMAND_FIRST_LINE_REGEX: std::sync::OnceLock<Regex> =
                std::sync::OnceLock::new();
            let command_first_line_regex = COMMAND_FIRST_LINE_REGEX.get_or_init(|| {
                Regex::new(r#"^ *(\+|-|#|\^) +([^ ]+) *$"#)
                    .with_context(|| "Can not compile regular expression for commands splitting")
                    .unwrap()
            });
            if let Some(captures) = command_first_line_regex.captures(lines[0]) {
                let operation_char = captures[1].chars().next().unwrap();
                let alias_option = captures
                    .get(1)
                    .map(|alias_match| Alias(alias_match.as_str().to_string()));
                if let Some(ref alias) = alias_option {
                    alias.validated().with_context(|| {
                        format!(
                            "Can not parse first line {:?} in {}-nth paragraph {:?}",
                            lines[0],
                            paragraph_index + 1,
                            paragraph
                        )
                    })?;
                }
                Ok(Some(match (operation_char, lines.len()) {
                    ('+', 2) => {
                        let add_text_thesis = AddThesis {
                            thesis: Thesis {
                                alias: alias_option.clone(),
                                content: Content::Text(Text::new(lines[1], &*self.transaction_for_aliases_resolving)?),
                                tags: vec![]
                            }
                        };
                        if let Some(ref alias) = alias_option {
                            self.aliases.insert(alias.clone(), add_text_thesis.thesis.id()?);
                        }
                        Command::AddThesis(add_text_thesis)
                    }
                    ('+', 4) => {
                        let add_relation_thesis = AddThesis {
                            thesis: Thesis {
                                alias: alias_option.clone(),
                                content: Content::Relation(Relation { from: self.get_thesis_id_by_reference(&Reference::new(lines[1])?)?, kind: RelationKind(lines[2].to_string()), to: self.get_thesis_id_by_reference(&Reference::new(lines[3])?)? }),
                                tags: vec![],
                            }
                        };
                        if let Some(ref alias) = alias_option {
                            self.aliases.insert(
                                alias.clone(),
                                add_relation_thesis.thesis.id()?
                            );
                        }
                        Command::AddThesis(add_relation_thesis)
                    }
                    ('-', 2) => Command::RemoveThesis(RemoveThesis {
                        thesis_id: serde_json::from_str(&format!("\"{}\"", lines[1]))?,
                    }),
                    ('#', 3) => Command::AddTag(AddTag {
                        thesis_id: self.get_thesis_id_by_reference(&Reference::new(lines[1])?)?,
                        tag: Tag(lines[2].to_string()),
                    }),
                    ('^', 3) => Command::RemoveTag(RemoveTag {
                        thesis_id: serde_json::from_str(&format!("\"{}\"", lines[1]))?,
                        tag: Tag(lines[2].to_string()),
                    }),
                    _ => {
                        return Err(anyhow!(
                            "Unsupported operation character and lines count combination ({:?}, {}) in first line {:?} of {}-th paragraph {:?}, supported combinations are ('+', 2) for adding text thesis, ('+', 4) for adding relation thesis, ('-', 2) for removing thesis, ('#', 3) for adding tag, ('^', 3) for removing tag",
                            operation_char,
                            lines.len(),
                            lines[0],
                            paragraph_index + 1,
                            paragraph
                        ));
                    }
                }.validated().with_context(|| format!("Invalid command parsed from {}-th paragraph {:?}", paragraph_index + 1, paragraph))?.to_owned()))
            } else {
                Err(anyhow!(
                    "Can not parse first line {:?} in {}-th paragraph {:?}",
                    lines[0],
                    paragraph_index + 1,
                    paragraph
                ))
            }
        } else {
            Ok(None)
        }
    }
}
