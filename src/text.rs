use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde::{Deserialize, Serialize};
use trove::ObjectId;

use crate::alias::Alias;
use crate::aliases_resolver::AliasesResolver;
use crate::commands::Reference;

#[derive(Serialize, Deserialize, Debug, Clone, bincode::Encode, PartialEq, Eq)]
pub struct RawText(pub String);

impl RawText {
    pub fn validated(&self) -> Result<&Self> {
        static RAW_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let sentence_regex = RAW_REGEX.get_or_init(|| {
            Regex::new(r#"^[0-9\p{Script=Cyrillic}\p{Script=Latin}\s,\-\:\."']+$"#)
                .with_context(|| "Can not compile regular expression for text validation")
                .unwrap()
        });
        if sentence_regex.is_match(&self.0) {
            Ok(self)
        } else {
            Err(anyhow!(
                "Text part around mentions must be one English or Russian sentence part: letters, whitespaces, punctuation ,-:.'\" and references thesis id or alias put inside square brackets [], so {:?} does not seem to be text",
                self.0
            ))
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, bincode::Encode, Eq)]
pub struct Text {
    #[serde(default)]
    pub raw_text_parts: Vec<RawText>,
    #[serde(default)]
    pub references: Vec<ObjectId>,
    pub start_with_reference: bool,
}

impl Text {
    pub fn new(input: &str, aliases_resolver: &mut AliasesResolver) -> Result<Self> {
        static REFERENCE_IN_TEXT_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let reference_in_text_regex = REFERENCE_IN_TEXT_REGEX.get_or_init(|| {
            Regex::new(r#"\[(:?([A-Za-z0-9-_]{22})|([^\[\]]+))\]"#)
                .with_context(|| "Can not compile regular expression to split text on raw text parts and references")
                .unwrap()
        });

        let mut result = Self {
            raw_text_parts: Vec::new(),
            references: Vec::new(),
            start_with_reference: false,
        };
        let mut last_match_end = 0;
        for reference_match in reference_in_text_regex.captures_iter(input) {
            let full_reference_match = reference_match.get(0).unwrap();
            if full_reference_match.start() == 0 {
                result.start_with_reference = true;
            }
            let text_before = &input[last_match_end..full_reference_match.start()];
            if !text_before.is_empty() {
                result.raw_text_parts.push(RawText(text_before.to_string()));
            }
            if let Some(thesis_id_string) = reference_match
                .get(2)
                .map(|thesis_id_string_match| thesis_id_string_match.as_str())
            {
                result.references.push(
                    serde_json::from_value(serde_json::Value::String(thesis_id_string.to_string()))
                        .unwrap(),
                );
            } else if let Some(alias_string) = reference_match
                .get(3)
                .map(|alias_string_match| alias_string_match.as_str())
            {
                result.references.push(
                    aliases_resolver
                        .get_thesis_id_by_reference(&Reference::Alias(Alias(alias_string.to_string()))).with_context(|| anyhow!("Can not parse text {:?} with alias {:?} because do not know such alias", input, alias_string))?,
                );
            }
            last_match_end = full_reference_match.end();
        }
        if last_match_end < input.len() {
            let remaining = &input[last_match_end..];
            if !remaining.is_empty() {
                result.raw_text_parts.push(RawText(remaining.to_string()));
            }
        }

        Ok(result)
    }

    pub fn composed(&self) -> String {
        let mut result_list = Vec::new();
        if self.start_with_reference {
            for (reference_index, reference) in self.references.iter().enumerate() {
                result_list.push(format!(
                    "[{}]",
                    serde_json::to_value(reference).unwrap().as_str().unwrap()
                ));
                if reference_index < self.raw_text_parts.len() {
                    result_list.push(self.raw_text_parts[reference_index].0.clone());
                }
            }
        } else {
            for (part_index, part) in self.raw_text_parts.iter().enumerate() {
                result_list.push(part.0.clone());
                if part_index < self.references.len() {
                    result_list.push(format!(
                        "[{}]",
                        serde_json::to_value(&self.references[part_index])
                            .unwrap()
                            .as_str()
                            .unwrap()
                    ));
                }
            }
        }
        result_list.concat()
    }

    pub fn validated(&self) -> Result<&Self> {
        for part in self.raw_text_parts.iter() {
            part.validated()?;
        }
        Ok(self)
    }
}
