//! REST API of the node

mod server;

pub mod v0;

pub use self::server::{Error, Server};

use crate::blockcfg::mock::Mockchain;
use crate::blockchain::BlockchainR;
use crate::settings::start::{Error as ConfigError, Rest};
use crate::settings::Error as SettingsError;

pub struct Context {
    pub stats_counter: v0::node::stats::StatsCounter,
    pub blockchain: BlockchainR<Mockchain>,
    pub transaction_task: v0::transaction::Task,
}

pub fn start_rest_server(config: &Rest, context: Context) -> Result<Server, SettingsError> {
    let prefix = config
        .prefix
        .as_ref()
        .map(|prefix| prefix.as_str())
        .unwrap_or("");
    Server::builder(config.pkcs12.clone(), config.listen.clone(), prefix)
        .add_handler(v0::block::create_handler(context.blockchain.clone()))
        .add_handler(v0::node::stats::create_handler(context.stats_counter))
        .add_handler(v0::tip::create_handler(context.blockchain.clone()))
        .add_handler(v0::transaction::create_handler(context.transaction_task))
        .add_handler(v0::utxo::create_handler(context.blockchain))
        .build()
        .map_err(|e| SettingsError::Start(ConfigError::InvalidRest(e)))
}
