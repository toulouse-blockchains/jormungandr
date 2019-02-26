use actix_web::{App, Json, Responder, State};
use blockcfg::mock::Mockchain;
use blockchain::BlockchainR;

pub fn crate_handler(
    blockchain: BlockchainR<Mockchain>,
) -> impl Fn() -> App<BlockchainR<Mockchain>> + Send + Sync + Clone + 'static {
    move || {
        App::with_state(blockchain.clone()).resource("v0/utxo", |r| r.get().with(handle_request))
    }
}

fn handle_request(blockchain: State<BlockchainR<Mockchain>>) -> impl Responder {
    let utxos = &blockchain
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .state
        .ledger
        .unspent_outputs;
    Json(json!({}))
}
