use chain_core::property::Serialize;
use chain_crypto as crypto;
use chain_impl_mockchain::certificate;
use reqwest;

pub fn create_stake_pool(host: &str) {
    let mut host = host.to_owned();
    if host.ends_with('/') {
        host.pop();
    }

    let xprv = crypto::SecretKey::<crypto::Ed25519Extended>::generate(rand::thread_rng());

    let msg = (certificate::StakePoolRegistration {
        pool_id: (&xprv).into(),
    })
    .make_certificate(&xprv);

    let client = reqwest::Client::new().unwrap();

    let resp = client
        .post(&(host + "/api/v0/message"))
        .unwrap()
        .body(msg.serialize_as_vec().unwrap())
        .send()
        .unwrap();
    assert!(resp.status().is_success());

    // FIXME: write to file (or alternatively, add an argument to read
    // a stake pool private key from a file).
    //println!("{:?}", xprv);
    println!("{:?}", xprv.to_public());
}
