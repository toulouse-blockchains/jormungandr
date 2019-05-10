use chain_addr::Address;
use chain_impl_mockchain::{
    self as chain,
    fee::FeeAlgorithm,
    message::Message,
    transaction::{NoExtra, Transaction},
    value::Value,
};
use jcli_app::transaction::{common, Error};
use jcli_app::utils::error::CustomErrorFiller;
use jcli_app::utils::io;
use jormungandr_utils::serde;
use serde::{Deserialize, Serialize};
use std::path::Path;

const INPUT_PTR_SIZE: usize = 32;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum StagingKind {
    Balancing,
    Finalizing,
    Sealed,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Input {
    index_or_account: u8,
    #[serde(with = "serde::value")]
    value: Value,
    input_ptr: [u8; INPUT_PTR_SIZE],
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Output {
    #[serde(with = "serde::address")]
    address: Address,
    #[serde(with = "serde::value")]
    value: Value,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Witness {
    #[serde(with = "serde::witness")]
    witness: chain::transaction::Witness,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Staging {
    kind: StagingKind,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
    witnesses: Vec<Witness>,
}

impl std::fmt::Display for StagingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            StagingKind::Balancing => write!(f, "balancing"),
            StagingKind::Finalizing => write!(f, "finalizing"),
            StagingKind::Sealed => write!(f, "sealed"),
        }
    }
}

impl Staging {
    pub fn new() -> Self {
        Staging {
            kind: StagingKind::Balancing,
            inputs: Vec::new(),
            outputs: Vec::new(),
            witnesses: Vec::new(),
        }
    }

    pub fn load<P: AsRef<Path>>(path: &Option<P>) -> Result<Self, Error> {
        let file = io::open_file_read(path).map_err(|source| Error::StagingFileOpenFailed {
            source,
            path: common::path_to_path_buf(path),
        })?;
        bincode::deserialize_from(file).map_err(|source| Error::StagingFileReadFailed {
            source,
            path: common::path_to_path_buf(path),
        })
    }

    pub fn store<P: AsRef<Path>>(&self, path: &Option<P>) -> Result<(), Error> {
        let file = io::open_file_write(path).map_err(|source| Error::StagingFileOpenFailed {
            source,
            path: common::path_to_path_buf(path),
        })?;
        bincode::serialize_into(file, self).map_err(|source| Error::StagingFileWriteFailed {
            source,
            path: common::path_to_path_buf(path),
        })
    }

    pub fn add_input(&mut self, input: chain::transaction::Input) -> Result<(), Error> {
        if self.kind != StagingKind::Balancing {
            return Err(Error::TxKindToAddInputInvalid { kind: self.kind });
        }

        Ok(self.inputs.push(Input {
            index_or_account: input.index_or_account,
            value: input.value,
            input_ptr: input.input_ptr,
        }))
    }

    pub fn add_output(&mut self, output: chain::transaction::Output<Address>) -> Result<(), Error> {
        if self.kind != StagingKind::Balancing {
            return Err(Error::TxKindToAddOutputInvalid { kind: self.kind });
        }

        Ok(self.outputs.push(Output {
            address: output.address,
            value: output.value,
        }))
    }

    pub fn add_witness(&mut self, witness: chain::transaction::Witness) -> Result<(), Error> {
        if self.kind != StagingKind::Finalizing {
            return Err(Error::TxKindToAddWitnessInvalid { kind: self.kind });
        }

        if self.inputs.len() <= self.witnesses.len() {
            return Err(Error::TooManyWitnessesToAddWitness {
                actual: self.witnesses.len(),
                max: self.inputs.len(),
            });
        }

        Ok(self.witnesses.push(Witness { witness }))
    }

    pub fn witness_count(&self) -> usize {
        self.witnesses.len()
    }

    pub fn staging_kind_name(&self) -> String {
        self.kind.to_string()
    }

    pub fn finalize<FA>(
        &mut self,
        fee_algorithm: FA,
        output_policy: chain::txbuilder::OutputPolicy,
    ) -> Result<chain::transaction::Balance, Error>
    where
        FA: FeeAlgorithm<Transaction<Address, NoExtra>>,
    {
        if self.kind != StagingKind::Balancing {
            return Err(Error::TxKindToFinalizeInvalid { kind: self.kind });
        }
        let builder = self.builder();

        let (balance, tx) = builder.finalize(fee_algorithm, output_policy)?;

        self.inputs = tx
            .inputs
            .into_iter()
            .map(|input| Input {
                index_or_account: input.index_or_account,
                value: input.value,
                input_ptr: input.input_ptr,
            })
            .collect();
        self.outputs = tx
            .outputs
            .into_iter()
            .map(|output| Output {
                address: output.address,
                value: output.value,
            })
            .collect();

        self.kind = StagingKind::Finalizing;

        Ok(balance)
    }

    pub fn seal(&mut self) -> Result<(), Error> {
        if self.kind != StagingKind::Finalizing {
            return Err(Error::TxKindToSealInvalid { kind: self.kind });
        }

        if self.inputs.len() != self.witnesses.len() {
            return Err(Error::WitnessCountToSealInvalid {
                actual: self.witnesses.len(),
                expected: self.inputs.len(),
            });
        }

        Ok(self.kind = StagingKind::Sealed)
    }

    pub fn message(&self) -> Result<Message, Error> {
        if self.kind != StagingKind::Sealed {
            Err(Error::TxKindToGetMessageInvalid { kind: self.kind })?
        }

        let transaction = self.finalizer()?;

        let result = transaction
            .build()
            .map_err(|source| Error::GeneratedTxBuildingFailed {
                source,
                filler: CustomErrorFiller,
            })?;

        match result {
            chain::txbuilder::GeneratedTransaction::Type1(auth) => Ok(Message::Transaction(auth)),
            _ => Err(Error::GeneratedTxTypeUnexpected),
        }
    }

    pub fn transaction(
        &self,
    ) -> chain::transaction::Transaction<Address, chain::transaction::NoExtra> {
        chain::transaction::Transaction {
            inputs: self.inputs(),
            outputs: self.outputs(),
            extra: chain::transaction::NoExtra,
        }
    }

    pub fn builder(
        &self,
    ) -> chain::txbuilder::TransactionBuilder<Address, chain::transaction::NoExtra> {
        chain::txbuilder::TransactionBuilder::from(self.transaction())
    }

    pub fn finalizer(&self) -> Result<chain::txbuilder::TransactionFinalizer, Error> {
        let transaction = self.transaction();
        let mut finalizer = chain::txbuilder::TransactionFinalizer::new_trans(transaction);

        for (index, witness) in self.witnesses.iter().enumerate() {
            finalizer
                .set_witness(index, witness.witness.clone())
                .map_err(|source| Error::AddingWitnessToFinalizedTxFailed {
                    source,
                    filler: CustomErrorFiller,
                })?;
        }

        Ok(finalizer)
    }

    pub fn inputs(&self) -> Vec<chain::transaction::Input> {
        self.inputs
            .iter()
            .map(|input| chain::transaction::Input {
                index_or_account: input.index_or_account,
                value: input.value,
                input_ptr: input.input_ptr,
            })
            .collect()
    }

    pub fn outputs(&self) -> Vec<chain::transaction::Output<Address>> {
        self.outputs
            .iter()
            .map(|output| chain::transaction::Output {
                address: output.address.clone(),
                value: output.value,
            })
            .collect()
    }
}
