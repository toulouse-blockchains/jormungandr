use reqwest;
use std::io::Read;

pub fn dump_utxos(host: &str) {
    let mut host = host.to_owned();
    if host.ends_with('/') {
        host.pop();
    }

    let mut resp = reqwest::get(&(host + "/api/v0/utxo")).unwrap();
    assert!(resp.status().is_success());

    let mut content = String::new();
    resp.read_to_string(&mut content).unwrap();

    let data: serde_json::Value = serde_json::from_reader(content.as_bytes()).unwrap();

    println!("GOT: {:#?}", data);
}
