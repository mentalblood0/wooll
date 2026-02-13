use anyhow::{Result, anyhow};
use std::collections::BTreeMap;
use trove::ObjectId;

use crate::{alias::Alias, commands::Reference, read_transaction::ReadTransactionMethods};

pub struct AliasesResolver<'a> {
    pub read_able_transaction: &'a dyn ReadTransactionMethods,
    pub known_aliases: BTreeMap<Alias, ObjectId>,
}

impl<'a> AliasesResolver<'a> {
    pub fn get_thesis_id_by_reference(&mut self, reference: &Reference) -> Result<ObjectId> {
        Ok(match reference {
            Reference::ObjectId(thesis_id) => {
                if self.read_able_transaction.get_thesis(thesis_id)?.is_none() {
                    return Err(anyhow!("Can not find thesis with id {thesis_id:?}"));
                }
                thesis_id.clone()
            }
            Reference::Alias(alias) => {
                if let Some(result) = self.known_aliases.get(alias) {
                    result.clone()
                } else {
                    let result = self
                        .read_able_transaction
                        .get_thesis_id_by_alias(alias)?
                        .ok_or_else(|| anyhow!("Can not find thesis id by alias {alias:?}"))?;
                    self.known_aliases.insert(alias.clone(), result.clone());
                    result
                }
            }
        })
    }

    pub fn remember(&mut self, alias: Alias, object_id: ObjectId) -> &Self {
        self.known_aliases.insert(alias, object_id);
        self
    }
}
