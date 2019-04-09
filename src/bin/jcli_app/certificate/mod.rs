use bech32::{Bech32, FromBase32};
use cardano::util::hex;
use chain_core::property::Serialize;
use chain_crypto::{bech32, Ed25519Extended, PublicKey, SecretKey};
use chain_impl_mockchain::{
    certificate::{self, CertificateContent},
    leadership::genesis::GenesisPraosLeader,
    stake::{StakeKeyId, StakePoolInfo},
};
use std::{fs, path::PathBuf};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum Certificate {
    /// Build transaction and write it to stdout as hex-encoded message
    New(NewArgs),
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum NewArgs {
    StakePoolRegistration(StakePoolRegistrationArgs),
}

#[derive(StructOpt)]
pub struct StakeKeyRegistrationArgs {
    #[structopt(name = "PUBLIC_KEY")]
    pub key: String,
    #[structopt(name = "SIGNING_KEY")]
    pub private_key: PathBuf,
}

#[derive(StructOpt)]
pub struct StakeKeyDeregistrationArgs {
    #[structopt(name = "PUBLIC_KEY")]
    pub key: String,
    #[structopt(name = "SIGNING_KEY")]
    pub private_key: PathBuf,
}

#[derive(StructOpt)]
pub struct StakeDelegationArgs {
    #[structopt(name = "PUBLIC_KEY")]
    pub key: String,
    #[structopt(name = "POOL_ID")]
    pub pool_id: String,
    #[structopt(name = "SIGNING_KEY")]
    pub private_key: PathBuf,
}

#[derive(Debug, StructOpt)]
pub struct StakePoolRegistrationArgs {
    #[structopt(long = "serial", name = "SERIAL")]
    pub serial: u128,
    #[structopt(long = "owner", name = "PUBLIC_KEY")]
    pub owners: Vec<String>,
    #[structopt(long = "kes-key", name = "KES_KEY")]
    pub kes_key: PathBuf,
    #[structopt(long = "vrf-key", name = "VRF_KEY")]
    pub vrf_key: PathBuf,
    #[structopt(long = "signing-key", name = "PRIVATE_KEY_FILE")]
    pub priv_keys: Vec<PathBuf>,
}

impl NewArgs {
    pub fn exec(self) {
        match self {
            NewArgs::StakePoolRegistration(args) => args.exec(),
        }
    }
}

impl StakePoolRegistrationArgs {
    pub fn exec(self) {
        let owners = self
            .owners
            .iter()
            .map(|key| {
                let stake_key_id =
                    <PublicKey<Ed25519Extended> as bech32::Bech32>::try_from_bech32_str(key)
                        .unwrap();
                StakeKeyId::from(stake_key_id)
            })
            .collect();

        let input_str =
            fs::read_to_string(&self.kes_key).expect("Cannot read input key from the given input");
        let bech32: Bech32 = input_str
            .trim_end()
            .parse()
            .expect("Expect a valid Bech32 string");
        let pub_key_bytes = Vec::<u8>::from_base32(bech32.data()).unwrap();
        let kes_public_key = PublicKey::from_binary(&pub_key_bytes).unwrap();

        let input_str =
            fs::read_to_string(&self.vrf_key).expect("Cannot read input key from the given input");
        let bech32: Bech32 = input_str
            .trim_end()
            .parse()
            .expect("Expect a valid Bech32 string");
        let pub_key_bytes = Vec::<u8>::from_base32(bech32.data()).unwrap();
        let vrf_public_key = PublicKey::from_binary(&pub_key_bytes).unwrap();

        let initial_key = GenesisPraosLeader {
            kes_public_key,
            vrf_public_key,
        };

        let content = StakePoolInfo {
            serial: self.serial,
            owners,
            initial_key,
        };

        let signatures = self
            .priv_keys
            .iter()
            .map(|key_path| {
                let key_str = fs::read_to_string(key_path).unwrap();
                let private_key =
                    <SecretKey<Ed25519Extended> as bech32::Bech32>::try_from_bech32_str(
                        &key_str.trim(),
                    )
                    .unwrap();
                content.make_certificate(&private_key)
            })
            .collect();

        let cert = certificate::Certificate {
            content: CertificateContent::StakePoolRegistration(content),
            signatures,
        };

        let data = cert.serialize_as_vec().unwrap();
        println!("{}", hex::encode(&data));
    }
}

impl Certificate {
    pub fn exec(self) {
        match self {
            Certificate::New(args) => args.exec(),
        }
    }
}
