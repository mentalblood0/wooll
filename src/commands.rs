use std::collections::BTreeSet;

use anyhow::{Context, Error, Result, anyhow};
use fallible_iterator::FallibleIterator;
use regex::Regex;
use serde::{Deserialize, Serialize};
use trove::ObjectId;

use crate::alias::Alias;
use crate::aliases_resolver::AliasesResolver;
use crate::content::Content;
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
            Ok(Self::ObjectId(serde_json::from_value(
                serde_json::Value::String(input.to_string()),
            )?))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    AddThesis(Thesis),
    RemoveThesis(ObjectId),
    AddTags(ObjectId, Vec<Tag>),
    RemoveTags(ObjectId, Vec<Tag>),
}

impl Command {
    pub fn validated(&self) -> Result<&Self> {
        match self {
            Command::AddThesis(thesis) => {
                thesis.validated()?;
            }
            Command::RemoveThesis(_) => {}
            Command::AddTags(_, tags) => {
                for tag in tags.iter() {
                    tag.validated()?;
                }
            }
            Command::RemoveTags(_, tags) => {
                for tag in tags.iter() {
                    tag.validated()?;
                }
            }
        }
        Ok(self)
    }
}

pub struct CommandsIterator<'a> {
    supported_relations_kinds: &'a BTreeSet<RelationKind>,
    paragraphs_iterator: Box<dyn FallibleIterator<Item = (usize, &'a str), Error = Error> + 'a>,
    aliases_resolver: &'a mut AliasesResolver<'a>,
}

impl<'a> CommandsIterator<'a> {
    pub fn new(
        input: &'a str,
        supported_relations_kinds: &'a BTreeSet<RelationKind>,
        aliases_resolver: &'a mut AliasesResolver<'a>,
    ) -> Self {
        static COMMANDS_SPLIT_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let commands_split_regex = COMMANDS_SPLIT_REGEX.get_or_init(|| {
            Regex::new(r#"(\r?\n|\r){2,}"#)
                .with_context(|| "Can not compile regular expression for commands splitting")
                .unwrap()
        });
        Self {
            supported_relations_kinds,
            aliases_resolver: aliases_resolver,
            paragraphs_iterator: Box::new(fallible_iterator::convert(
                commands_split_regex
                    .split(input)
                    .map(|paragraph| paragraph.trim())
                    .filter(|paragraph| !paragraph.is_empty())
                    .enumerate()
                    .map(|index_and_paragraph| Ok(index_and_paragraph)),
            )),
        }
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
                Regex::new(r#"^ *(\+|-|#|\^)(:? +([^ ]+))? *$"#)
                    .with_context(|| "Can not compile regular expression for commands splitting")
                    .unwrap()
            });
            if let Some(captures) = command_first_line_regex.captures(lines[0]) {
                let operation_char = captures[1].chars().next().unwrap();
                let alias_option = captures
                    .get(3)
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
                            let thesis = Thesis {
                                alias: alias_option.clone(),
                                content: Content::Text(Text::new(lines[1], self.aliases_resolver)?),
                                tags: vec![]
                            };
                        if let Some(ref alias) = alias_option {
                            self.aliases_resolver.remember(alias.clone(), thesis.id()?);
                        }
                        Command::AddThesis(thesis)
                    }
                    ('+', 4) => {
                        let    thesis = Thesis {
                                alias: alias_option.clone(),
                                content: Content::Relation(Relation { from: self.aliases_resolver.get_thesis_id_by_reference(&Reference::new(lines[1])?).with_context(|| format!("Can not parse relation for AddThesis command on {}-th paragraph {:?}", paragraph_index + 1, paragraph))?, kind: RelationKind(lines[2].to_string()), to: self.aliases_resolver.get_thesis_id_by_reference(&Reference::new(lines[3])?)? }),
                                tags: vec![],
                        };
                        if let Some(ref alias) = alias_option {
                            self.aliases_resolver.remember(alias.clone(), thesis.id()?);
                        }
                        Command::AddThesis(thesis)
                    }
                    ('-', 2) => Command::RemoveThesis(
                        self.aliases_resolver.get_thesis_id_by_reference(&Reference::new(lines[1])?)?,
                    ),
                    ('#', 3..) => Command::AddTags(
                        self.aliases_resolver.get_thesis_id_by_reference(&Reference::new(lines[1])?)?,
                         lines[2..].iter().map(|tag_string|  Tag(tag_string.to_string())).collect(),
                    ),
                    ('^', 3..) => Command::RemoveTags(
                         self.aliases_resolver.get_thesis_id_by_reference(&Reference::new(lines[1])?)?,
                        lines[2..].iter().map(|tag_string|  Tag(tag_string.to_string())).collect(),
                    ),
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
