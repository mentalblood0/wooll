pub mod alias;
pub mod aliases_resolver;
pub mod commands;
pub mod content;
pub mod read_transaction;
pub mod relation;
pub mod sweater;
pub mod tag;
pub mod text;
pub mod thesis;
pub mod write_transaction;

use trove::PathSegment;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fallible_iterator::FallibleIterator;
    use nanorand::{Rng, WyRand};
    use pretty_assertions::assert_eq;
    use trove::ObjectId;

    use crate::aliases_resolver::AliasesResolver;
    use crate::commands::CommandsIterator;
    use crate::content::Content;
    use crate::read_transaction::ReadTransactionMethods;
    use crate::relation::Relation;
    use crate::sweater::Sweater;
    use crate::tag::Tag;
    use crate::text::Text;
    use crate::thesis::Thesis;
    use crate::write_transaction::WriteTransaction;

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

    fn random_text(
        rng: &mut WyRand,
        previously_added_theses: &BTreeMap<ObjectId, Thesis>,
        aliases_resolver: &mut AliasesResolver,
    ) -> Text {
        const ENGLISH_LETTERS: [&str; 26] = [
            "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q",
            "r", "s", "t", "u", "v", "w", "x", "y", "z",
        ];
        const RUSSIAN_LETTERS: [&str; 33] = [
            "а", "б", "в", "г", "д", "е", "ё", "ж", "з", "и", "й", "к", "л", "м", "н", "о", "п",
            "р", "с", "т", "у", "ф", "х", "ц", "ч", "ш", "щ", "ъ", "ы", "ь", "э", "ю", "я",
        ];
        const PUNCTUATION: &[&str] = &[",-'\""];
        let language = rng.generate_range(1..=2);
        let mut references_count = 0;
        let words: Vec<String> = (0..rng.generate_range(3..=10))
            .map(|_| {
                if previously_added_theses.is_empty() || rng.generate_range(0..=3) > 0 {
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
                    references_count += 1;
                    format!(
                        "[{}]",
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
        let mut result_string = String::new();
        for (i, word) in words.iter().enumerate() {
            result_string.push_str(word);
            if i < words.len() - 1 {
                if rng.generate_range(0..3) == 0 {
                    result_string.push_str(PUNCTUATION[rng.generate_range(0..PUNCTUATION.len())]);
                } else {
                    result_string.push(' ');
                }
            }
        }
        let result = Text::new(&result_string, aliases_resolver).unwrap();
        assert_eq!(result.composed(), result_string);
        result
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
        aliases_resolver: &mut AliasesResolver,
        previously_added_theses: &BTreeMap<ObjectId, Thesis>,
        transaction: &WriteTransaction,
    ) -> Thesis {
        Thesis {
            alias: None,
            content: {
                let action_id = if previously_added_theses.is_empty() {
                    1
                } else {
                    rng.generate_range(1..=2)
                };
                match action_id {
                    1 => Content::Text(random_text(rng, previously_added_theses, aliases_resolver)),
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
                        rng.generate_range(1..=3)
                    };
                    let mut aliases_resolver = AliasesResolver {
                        read_able_transaction: transaction,
                        known_aliases: BTreeMap::new(),
                    };
                    match action_id {
                        1 => {
                            let thesis = {
                                let mut result = random_thesis(
                                    &mut rng,
                                    &mut aliases_resolver,
                                    &previously_added_theses,
                                    &transaction,
                                );
                                while previously_added_theses.contains_key(&result.id()?) {
                                    result = random_thesis(
                                        &mut rng,
                                        &mut aliases_resolver,
                                        &previously_added_theses,
                                        &transaction,
                                    );
                                }
                                result
                            };
                            thesis.validated()?;
                            transaction.insert_thesis(thesis.clone())?;
                            let thesis_id = thesis.id()?;
                            assert_eq!(transaction.get_thesis(&thesis_id)?.unwrap(), thesis);
                            for referenced_thesis_id in thesis.references() {
                                let where_referenced =
                                    transaction.where_referenced(&referenced_thesis_id)?;
                                assert!(where_referenced.contains(&thesis_id));
                            }
                            previously_added_theses.insert(thesis_id, thesis);
                        }
                        2 => {
                            let tag_to_add = random_tag(&mut rng);
                            let thesis_to_tag_id = previously_added_theses
                                .keys()
                                .nth(rng.generate_range(0..previously_added_theses.len()))
                                .unwrap()
                                .clone();
                            transaction.tag_thesis(&thesis_to_tag_id, tag_to_add.clone())?;
                            assert!(
                                transaction
                                    .get_thesis(&thesis_to_tag_id)?
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
                        3 => {
                            if let Some((thesis_to_untag_id, thesis_to_untag)) =
                                previously_added_theses
                                    .iter()
                                    .find(|(_, thesis)| !thesis.tags.is_empty())
                                    .map(|(id, thesis)| (id.clone(), thesis.clone()))
                            {
                                let tag_to_remove_index =
                                    rng.generate_range(0..thesis_to_untag.tags.len());
                                let tag_to_remove =
                                    thesis_to_untag.tags[tag_to_remove_index].clone();
                                transaction.untag_thesis(&thesis_to_untag_id, &tag_to_remove)?;
                                assert!(
                                    !transaction
                                        .get_thesis(&thesis_to_untag_id)?
                                        .unwrap()
                                        .tags
                                        .contains(&tag_to_remove)
                                );
                                previously_added_theses
                                    .get_mut(&thesis_to_untag_id)
                                    .unwrap()
                                    .tags
                                    .remove(tag_to_remove_index);
                            }
                        }
                        _ => {}
                    }
                }
                for (thesis_id, thesis) in previously_added_theses.iter() {
                    assert_eq!(transaction.get_thesis(thesis_id)?.unwrap(), *thesis);
                }
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn test_example() {
        let mut sweater = new_default_sweater("test_example");
        sweater
            .lock_all_and_write(|transaction| {
                let commands = CommandsIterator::new(
                    &std::fs::read_to_string("src/example.txt")?,
                    &transaction.sweater_config.supported_relations_kinds,
                    &mut AliasesResolver {
                        read_able_transaction: transaction,
                        known_aliases: BTreeMap::new(),
                    },
                )
                .collect::<Vec<_>>()?;
                for command in commands {
                    transaction.execute_command(&command)?;
                }
                Ok(())
            })
            .unwrap();
    }
}
