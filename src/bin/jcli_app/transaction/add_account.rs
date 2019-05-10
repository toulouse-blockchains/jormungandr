use chain_addr::{Address, Kind};
use chain_impl_mockchain::{
    transaction::{Input, InputEnum},
    value::Value,
};
use structopt::StructOpt;

use jcli_app::transaction::{common, Error};
use jormungandr_utils::structopt;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct AddAccount {
    #[structopt(flatten)]
    pub common: common::CommonTransaction,

    /// the account to debit the funds from
    #[structopt(name = "ACCOUNT", parse(try_from_str = "structopt::try_parse_address"))]
    pub account: Address,

    /// the value
    #[structopt(name = "VALUE", parse(try_from_str = "structopt::try_parse_value"))]
    pub value: Value,
}

impl AddAccount {
    pub fn exec(self) -> Result<(), Error> {
        let mut transaction = self.common.load()?;

        let account_identifier = match self.account.kind() {
            Kind::Account(key) => key.clone().into(),
            Kind::Single(_) => return Err(Error::AccountAddressSingle),
            Kind::Group(_, _) => return Err(Error::AccountAddressGroup),
        };

        transaction.add_input(Input::from_enum(InputEnum::AccountInput(
            account_identifier,
            self.value,
        )))?;

        self.common.store(&transaction)
    }
}
