mod add_account;
mod add_input;
mod add_output;
mod add_witness;
mod common;
mod finalize;
mod info;
mod mk_witness;
mod new;
mod seal;
mod staging;

use self::staging::StagingKind;
use cardano::util::hex;
use chain_core::property::Serialize as _;
use chain_impl_mockchain as chain;
use jcli_app::utils::error::CustomErrorFiller;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum Transaction {
    /// create a new staging transaction. The transaction is initially
    /// empty.
    New(new::New),

    /// add UTxO input to the transaction
    AddInput(add_input::AddInput),
    /// add Account input to the transaction
    AddAccount(add_account::AddAccount),
    /// add output to the transaction
    AddOutput(add_output::AddOutput),
    /// add output to the finalized transaction
    AddWitness(add_witness::AddWitness),
    /// Lock a transaction and start adding witnesses
    Finalize(finalize::Finalize),
    /// Finalize the transaction
    Seal(seal::Seal),
    /// get the Transaction ID from the given transaction
    /// (if the transaction is edited, the returned value will change)
    Id(common::CommonTransaction),
    /// display the info regarding a given transaction
    Info(info::Info),
    /// create witnesses
    MakeWitness(mk_witness::MkWitness),
    /// get the message format out of a sealed transaction
    ToMessage(common::CommonTransaction),
}

type StaticStr = &'static str;

custom_error! { pub Error
    StagingFileOpenFailed { source: std::io::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not open staging transaction file '{}'", path.display()) }},
    StagingFileReadFailed { source: bincode::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not read staging transaction file '{}'", path.display()) }},
    StagingFileWriteFailed { source: bincode::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not write staging transaction file '{}'", path.display()) }},
    SecretFileReadFailed { source: std::io::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not read secret file '{}'", path.display()) }},
    SecretFileMalformed { source: chain_crypto::bech32::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not decode secret file '{}'", path.display()) }},
    WitnessFileReadFailed { source: std::io::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not read witness file '{}'", path.display()) }},
    WitnessFileWriteFailed { source: std::io::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not write witness file '{}'", path.display()) }},
    WitnessFileBech32Malformed { source: bech32::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not parse Bech32 in witness file '{}'", path.display()) }},
    WitnessFileBech32HrpInvalid { actual: String, expected: StaticStr, path: PathBuf }
        = @{{ format_args!("Invalid Bech32 prefix in witness file, expected '{}', found '{}' in '{}'",
            expected, actual, path.display()) }},
    WitnessFileBech32EncodingFailed { source: bech32::Error } = "Failed to encode witness as bech32",
    WitnessFileDeserializationFailed { source: chain_core::mempack::ReadError, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not parse data in witness file '{}'", path.display()) }},
    WitnessFileSerializationFailed { source: std::io::Error, filler: CustomErrorFiller }
        = "Could not serialize witness data",
    InfoFileWriteFailed { source: std::io::Error, path: PathBuf }
        = @{{ let _ = source; format_args!("Could not write info file '{}'", path.display()) }},

    TxKindToAddInputInvalid { kind: StagingKind } = "Adding input to {kind} transaction is not valid",
    TxKindToAddOutputInvalid { kind: StagingKind } = "Adding output to {kind} transaction is not valid",
    TxKindToAddWitnessInvalid { kind: StagingKind } = "Adding witness to {kind} transaction is not valid",
    TxKindToSealInvalid { kind: StagingKind } = "Sealing {kind} transaction is not valid",
    TxKindToFinalizeInvalid { kind: StagingKind } = "Finalizing {kind} transaction is not valid",
    TxKindToGetMessageInvalid { kind: StagingKind } = "Cannot get message from transaction in {kind} state",

    TooManyWitnessesToAddWitness { actual: usize, max: usize }
        = "Too many witnesses in transaction to add another: {actual}, maximum is {max}",
    WitnessCountToSealInvalid { actual: usize, expected: usize }
        = "Invalid number of witnesses in transaction to seal: {actual}, should be {expected}",
    AccountAddressSingle = "Invalid input account, this is a UTxO address.",
    AccountAddressGroup = "Invalid input account, this is a UTxO address with delegation.",
    AddingWitnessToFinalizedTxFailed { source: chain::txbuilder::BuildError, filler: CustomErrorFiller }
        = "Could not add witness to finalized transaction",
    GeneratedTxBuildingFailed { source: chain::txbuilder::BuildError, filler: CustomErrorFiller }
        = "Generated transaction building failed",
    TxFinalizationFailed { source: chain::txbuilder::Error }
        = "Transaction finalization failed",
    GeneratedTxTypeUnexpected = "Unexpected generated transaction type",
    MessageSerializationFailed { source: std::io::Error, filler: CustomErrorFiller }
        = "Serialization of message to bytes failed",
    InfoOutputFormatInvalid { source: strfmt::FmtError, format: String } = "Invalid info output format '{format}'",
    InfoCalculationFailed { source: chain::value::ValueError } = "Calculation of info failed",
    MakeWitnessLegacyUtxoUnsupported = "Making legacy UTxO witness unsupported",
    MakeWitnessAccountCounterMissing = "Making account witness requires passing spending counter",
}

impl Transaction {
    pub fn exec(self) -> Result<(), Error> {
        match self {
            Transaction::New(new) => new.exec(),
            Transaction::AddInput(add_input) => add_input.exec(),
            Transaction::AddAccount(add_account) => add_account.exec(),
            Transaction::AddOutput(add_output) => add_output.exec(),
            Transaction::AddWitness(add_witness) => add_witness.exec(),
            Transaction::Finalize(finalize) => finalize.exec(),
            Transaction::Seal(seal) => seal.exec(),
            Transaction::Id(common) => display_id(common),
            Transaction::Info(info) => info.exec(),
            Transaction::MakeWitness(mk_witness) => mk_witness.exec(),
            Transaction::ToMessage(common) => display_message(common),
        }
    }
}

fn display_id(common: common::CommonTransaction) -> Result<(), Error> {
    let id = common.load()?.transaction().hash();
    println!("{}", id);
    Ok(())
}

fn display_message(common: common::CommonTransaction) -> Result<(), Error> {
    let message = common.load()?.message()?;
    let bytes: Vec<u8> =
        message
            .serialize_as_vec()
            .map_err(|source| Error::MessageSerializationFailed {
                source,
                filler: CustomErrorFiller,
            })?;
    println!("{}", hex::encode(&bytes));
    Ok(())
}
