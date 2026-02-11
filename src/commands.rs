use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};

use anyhow::{Context, Error, Result, anyhow};
use fallible_iterator::FallibleIterator;
use regex::Regex;
use serde::{Deserialize, Serialize};
use trove::ObjectId;

use crate::content::Content;
use crate::relation::{Relation, RelationKind};
use crate::tag::Tag;
use crate::text::Text;
use crate::alias::Alias;


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AddTextThesis {
    pub alias: Option<Alias>,
    pub text: Text,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AddRelationThesis {
    pub alias: Option<Alias>,
    pub from: Reference,
    pub to: Reference,
    pub kind: RelationKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RemoveThesis {
    pub thesis_id: ObjectId,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AddTag {
    pub reference: Reference,
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
                add_tag.reference.validated()?;
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
    aliases: BTreeMap<Alias, ObjectId>,
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
            aliases: BTreeMap::new(),
        }
    }

    fn get_thesis_id(&self, thesis_reference: &ThesisReference) -> Option<ObjectId> {
        match thesis_reference {
            ThesisReference::Alias(alias) => self.aliases.get(&alias).cloned(),
            ThesisReference::ObjectId(object_id) => Some(object_id.clone()),
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
                        let add_text_thesis = AddTextThesis {
                            alias: alias_option.clone(),
                            text: Text(lines[1].to_string()),
                        };
                        if let Some(ref alias) = alias_option {
                            self.aliases.insert(alias.clone(), Content::Text(add_text_thesis.text.clone()).id()?);
                        }
                        Command::AddTextThesis(add_text_thesis)
                    }
                    ('+', 4) => {
                        let add_relation_thesis = AddRelationThesis {
                            alias: alias_option.clone(),
                            from: ThesisReference::new(lines[1])?,
                            kind: RelationKind(lines[2].to_string()),
                            to: ThesisReference::new(lines[3])?,
                        };
                        if let Some(ref alias) = alias_option {
                            self.aliases.insert(
                                alias.clone(), 
                                Content::Relation(Relation {
                                    from: self.get_thesis_id(&add_relation_thesis.from).ok_or_else(|| anyhow!("Can not parse {}-th paragraph {:?}: no known thesis referenced by {:?}", paragraph_index + 1, paragraph, add_relation_thesis.from))?,
                                    to: self.get_thesis_id(&add_relation_thesis.to).ok_or_else(|| anyhow!("Can not parse {}-th paragraph {:?}: no known thesis referenced by {:?}", paragraph_index + 1, paragraph, add_relation_thesis.from))?,
                                    kind: add_relation_thesis.kind.clone() }
                                ).id()?
                            );
                        }
                        Command::AddRelationThesis(add_relation_thesis)
                    }
                    ('-', 2) => Command::RemoveThesis(RemoveThesis {
                        thesis_id: serde_json::from_str(&format!("\"{}\"", lines[1]))?,
                    }),
                    ('#', 3) => Command::AddTag(AddTag {
                        reference: ThesisReference::new(lines[1])?,
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
