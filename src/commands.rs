use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};

use anyhow::{Context, Error, Result, anyhow};
use fallible_iterator::FallibleIterator;
use regex::Regex;
use serde::{Deserialize, Serialize};
use trove::ObjectId;

use crate::relation::RelationKind;
use crate::tag::Tag;
use crate::text::Text;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Alias(String);

impl Alias {
    pub fn validated(&self) -> Result<&Self> {
        static ALIAS_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let sentence_regex = ALIAS_REGEX.get_or_init(|| {
            Regex::new(r#"^\S+$"#)
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThesisReference {
    Alias(Alias),
    ObjectId(ObjectId),
}

impl ThesisReference {
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

    pub fn validated(&self) -> Result<&Self> {
        match self {
            ThesisReference::Alias(alias) => {
                alias.validated()?;
            }
            ThesisReference::ObjectId(_) => {}
        }
        Ok(self)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AddTextThesis {
    pub alias: Option<Alias>,
    pub text: Text,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AddRelationThesis {
    pub alias: Option<Alias>,
    pub from: ThesisReference,
    pub to: ThesisReference,
    pub kind: RelationKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RemoveThesis {
    pub thesis_id: ObjectId,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AddTag {
    pub thesis_reference: ThesisReference,
    pub tag: Tag,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RemoveTag {
    pub thesis_id: ObjectId,
    pub tag: Tag,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    AddTextThesis(AddTextThesis),
    AddRelationThesis(AddRelationThesis),
    AddTag(AddTag),
    RemoveThesis(RemoveThesis),
    RemoveTag(RemoveTag),
}

impl Command {
    pub fn validated(&self) -> Result<&Self> {
        match self {
            Command::AddTextThesis(add_text_thesis) => {
                if let Some(ref alias) = add_text_thesis.alias {
                    alias.validated()?;
                }
                add_text_thesis.text.validated()?;
            }
            Command::AddRelationThesis(add_relation_thesis) => {
                if let Some(ref alias) = add_relation_thesis.alias {
                    alias.validated()?;
                }
                add_relation_thesis.kind.validated()?;
            }
            Command::RemoveThesis(_) => {}
            Command::AddTag(add_tag) => {
                add_tag.thesis_reference.validated()?;
                add_tag.tag.validated()?;
            }
            Command::RemoveTag(remove_tag) => {
                remove_tag.tag.validated()?;
            }
        }
        Ok(self)
    }
}

static COMMANDS_SPLIT_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
static COMMAND_FIRST_LINE_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

struct CommandsIterator<'a> {
    paragraphs_iterator: Box<dyn FallibleIterator<Item = (usize, &'a str), Error = Error> + 'a>,
    supported_relations_kinds: &'a BTreeSet<RelationKind>,
    aliases: BTreeSet<Alias>,
}

impl<'a> CommandsIterator<'a> {
    pub fn new(input: &'a str, supported_relations_kinds: &'a BTreeSet<RelationKind>) -> Self {
        let commands_split_regex = COMMANDS_SPLIT_REGEX.get_or_init(|| {
            Regex::new(r#"(\r?\n|\r){2,}"#)
                .with_context(|| "Can not compile regular expression for commands splitting")
                .unwrap()
        });
        Self {
            paragraphs_iterator: Box::new(fallible_iterator::convert(
                commands_split_regex
                    .split(input)
                    .map(|paragraph| paragraph.trim())
                    .filter(|paragraph| !paragraph.is_empty())
                    .enumerate()
                    .map(|index_and_paragraph| Ok(index_and_paragraph)),
            )),
            supported_relations_kinds,
            aliases: BTreeSet::new(),
        }
    }
}

impl<'a> FallibleIterator for CommandsIterator<'a> {
    type Item = Command;
    type Error = Error;

    fn next(&mut self) -> Result<Option<Self::Item>> {
        if let Some((paragraph_index, paragraph)) = self.paragraphs_iterator.next()? {
            let lines = paragraph.split('\n').collect::<Vec<_>>();
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
                        if let Some(ref alias) = alias_option {
                            self.aliases.insert(alias.clone());
                        }
                        Command::AddTextThesis(AddTextThesis {
                            alias: alias_option,
                            text: Text(lines[1].to_string()),
                        })
                    }
                    ('+', 4) => {
                        if let Some(ref alias) = alias_option {
                            self.aliases.insert(alias.clone());
                        }
                        Command::AddRelationThesis(AddRelationThesis {
                            alias: alias_option,
                            from: ThesisReference::new(lines[1])?,
                            kind: RelationKind(lines[2].to_string()),
                            to: ThesisReference::new(lines[3])?,
                        })
                    }
                    ('-', 2) => Command::RemoveThesis(RemoveThesis {
                        thesis_id: serde_json::from_str(&format!("\"{}\"", lines[1]))?,
                    }),
                    ('#', 3) => Command::AddTag(AddTag {
                        thesis_reference: ThesisReference::new(lines[1])?,
                        tag: Tag(lines[2].to_string()),
                    }),
                    ('^', 3) => Command::RemoveTag(RemoveTag {
                        thesis_id: serde_json::from_str(&format!("\"{}\"", lines[1]))?,
                        tag: Tag(lines[2].to_string()),
                    }),
                    _ => {
                        return Err(anyhow!(
                            "Unsupported operation character and lines count combination ({:?}, {}) in first line {:?} of {}-nth paragraph {:?}, supported combinations are ('+', 2) for adding text thesis, ('+', 4) for adding relation thesis, ('-', 2) for removing thesis, ('#', 3) for adding tag, ('^', 3) for removing tag",
                            operation_char,
                            lines.len(),
                            lines[0],
                            paragraph_index + 1,
                            paragraph
                        ));
                    }
                }.validated()?.to_owned()))
            } else {
                Err(anyhow!(
                    "Can not parse first line {:?} in {}-nth paragraph {:?}",
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
