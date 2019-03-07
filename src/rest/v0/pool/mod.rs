use actix_web::{App, Json, Responder, State};
use blockcfg::mock::Mockchain;
use blockchain::BlockchainR;
use chain_impl_mockchain::stake::StakePoolInfo;
use std::collections::{HashMap, HashSet};

pub fn create_handler(
    blockchain: BlockchainR<Mockchain>,
) -> impl Fn(&str) -> App<BlockchainR<Mockchain>> + Send + Sync + Clone + 'static {
    move |prefix: &str| {
        let app_prefix = format!("{}/v0/pool", prefix);
        App::with_state(blockchain.clone())
            .prefix(app_prefix)
            .resource("", |r| r.get().with(handle_request))
    }
}

#[derive(Serialize)]
pub struct Pool {
    pub members: HashSet<String>,
}

impl<'a> From<&StakePoolInfo> for Pool {
    fn from(pool_info: &StakePoolInfo) -> Self {
        Self {
            members: pool_info
                .members
                .iter()
                .map(|id| format!("{:?}", id))
                .collect(),
        }
    }
}

fn handle_request(blockchain: State<BlockchainR<Mockchain>>) -> impl Responder {
    let blockchain = blockchain.read().unwrap_or_else(|e| e.into_inner());
    let leaders = &blockchain.state.leaders;
    let pools = leaders
        .get_delegation_state()
        .get_stake_pools()
        .iter()
        .map(|(pool_id, pool_info)| (format!("{:?}", pool_id), Pool::from(pool_info)))
        .collect::<HashMap<_, _>>();
    Json(pools)
}
