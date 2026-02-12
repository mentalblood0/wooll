use anyhow::{Result, anyhow};
use fallible_iterator::FallibleIterator;
use trove::{IndexRecordType, Object, ObjectId, path_segments};

use crate::alias::Alias;
use crate::content::Content;
use crate::define_read_methods;
use crate::read_transaction::ReadTransactionMethods;
use crate::relation::Relation;
use crate::sweater::SweaterConfig;
use crate::tag::Tag;
use crate::thesis::Thesis;

pub struct WriteTransaction<'a, 'b, 'c, 'd> {
    pub chest_transaction: &'a mut trove::WriteTransaction<'b, 'c, 'd>,
    pub sweater_config: SweaterConfig,
}

impl ReadTransactionMethods for WriteTransaction<'_, '_, '_, '_> {
    define_read_methods!();
}

impl WriteTransaction<'_, '_, '_, '_> {
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

    pub fn untag_thesis(&mut self, thesis_id: &ObjectId, tag: &Tag) -> Result<()> {
        if let Some(tag_index_in_array) = self.chest_transaction.get_element_index(
            thesis_id,
            &path_segments!("tags"),
            &serde_json::to_value(tag)?.try_into()?,
        )? {
            self.chest_transaction
                .remove(thesis_id, &path_segments!("tags", tag_index_in_array))?;
        }
        Ok(())
    }

    pub fn remove_thesis(&mut self, thesis_id: &ObjectId) -> Result<()> {
        if self.chest_transaction.contains_object_with_id(thesis_id)? {
            self.chest_transaction.remove(thesis_id, &vec![])?;
            let thesis_id_json_value = serde_json::to_value(thesis_id)?;
            let relations_ids = self
                .chest_transaction
                .select(
                    &vec![(
                        IndexRecordType::Direct,
                        path_segments!("content", "Relation", "from"),
                        thesis_id_json_value.clone(),
                    )],
                    &vec![],
                    None,
                )?
                .chain(self.chest_transaction.select(
                    &vec![(
                        IndexRecordType::Direct,
                        path_segments!("content", "Relation", "to"),
                        thesis_id_json_value,
                    )],
                    &vec![],
                    None,
                )?)
                .collect::<Vec<_>>()?;
            for relation_id in relations_ids {
                self.chest_transaction.remove(&relation_id, &vec![])?;
            }
            let where_mentioned = self.where_referenced(thesis_id)?;
            for id_of_thesis_where_mentioned in where_mentioned {
                self.remove_thesis(&id_of_thesis_where_mentioned)?;
            }
        }
        Ok(())
    }
}
