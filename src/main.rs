#![cfg_attr(feature = "with-bench", feature(test))]
extern crate actix_net;
extern crate actix_web;
extern crate bech32;
extern crate bincode;
extern crate bytes;
extern crate cardano;
extern crate cardano_storage;
extern crate cbor_event;
extern crate chain_addr;
extern crate chain_core;
extern crate chain_crypto;
extern crate chain_impl_mockchain;
extern crate chain_storage;
extern crate chain_storage_sqlite;
extern crate clap;
extern crate cryptoxide;
extern crate exe_common;
extern crate futures;
extern crate generic_array;
extern crate http;
extern crate sha2;
#[macro_use]
extern crate lazy_static;
extern crate native_tls;
extern crate network_core;
extern crate network_grpc;
extern crate poldercast;
extern crate protocol_tokio as protocol;
extern crate rand_chacha;
extern crate tower_service;

extern crate tokio;
extern crate tokio_bus;

#[cfg(test)]
extern crate quickcheck;
extern crate rand;
extern crate regex;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate serde_yaml;
#[macro_use(o)]
extern crate slog;
extern crate slog_async;
extern crate slog_json;
extern crate slog_term;
extern crate structopt;
#[cfg(test)]
#[cfg(feature = "with-bench")]
extern crate test;

use std::io::{self, BufRead};
use std::sync::{mpsc::Receiver, Arc, Mutex, RwLock};

use chain_impl_mockchain::block::{message::MessageId, Message};
use futures::Future;

use bech32::{u5, Bech32, FromBase32, ToBase32};
use blockcfg::{
    genesis_data::ConfigGenesisData, genesis_data::GenesisData, mock::Mockchain as Cardano,
};
use blockchain::{Blockchain, BlockchainR};
use chain_crypto::{
    AsymmetricKey, Curve25519_2HashDH, Ed25519, Ed25519Bip32, Ed25519Extended, FakeMMM,
};
use intercom::BlockMsg;
use leadership::leadership_task;
use rand::rngs::EntropyRng;
use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use rest::v0::node::stats::StatsCounter;
use settings::{Command, GenPrivKeyType};
use transaction::{transaction_task, TPool};
use utils::task::{TaskBroadcastBox, Tasks};

#[macro_use]
pub mod log_wrapper;

pub mod blockcfg;
pub mod blockchain;
pub mod client;
pub mod clock;
// pub mod consensus;
pub mod intercom;
pub mod leadership;
pub mod network;
pub mod rest;
pub mod secure;
pub mod settings;
pub mod state;
pub mod transaction;
pub mod utils;

// TODO: consider an appropriate size for the broadcast buffer.
// For the block task, there should hardly be a need to buffer more
// than one block as the network task should be able to broadcast the
// block notifications in time.
const BLOCK_BUS_CAPACITY: usize = 2;

pub type TODO = u32;

fn block_task(
    blockchain: BlockchainR<Cardano>,
    _clock: clock::Clock, // FIXME: use it or lose it
    r: Receiver<BlockMsg<Cardano>>,
    stats_counter: StatsCounter,
) {
    let mut network_broadcast = TaskBroadcastBox::new(BLOCK_BUS_CAPACITY);
    loop {
        let bquery = r.recv().unwrap();
        blockchain::process(&blockchain, bquery, &mut network_broadcast, &stats_counter);
    }
}

fn startup_info(
    gd: &GenesisData,
    blockchain: &Blockchain<Cardano>,
    _settings: &settings::start::Settings,
) {
    println!(
        "k={} tip={}",
        gd.epoch_stability_depth,
        blockchain.get_tip()
    );
}

// Expand the type with more variants
// when it becomes necessary to represent different error cases.
type Error = settings::Error;

fn start(settings: settings::start::Settings) -> Result<(), Error> {
    settings.log_settings.apply();

    let genesis_data = settings.read_genesis_data().unwrap();

    let clock = {
        let initial_epoch = clock::ClockEpochConfiguration {
            slot_duration: genesis_data.slot_duration,
            slots_per_epoch: genesis_data.epoch_stability_depth * 10,
        };
        clock::Clock::new(genesis_data.start_time, initial_epoch)
    };

    let leader_secret = if let Some(secret_path) = &settings.leadership {
        Some(secure::NodeSecret::load_from_file(secret_path.as_path()))
    } else {
        None
    };

    //let mut state = State::new();

    let blockchain_data = Blockchain::new(genesis_data.clone(), &settings.storage);

    startup_info(&genesis_data, &blockchain_data, &settings);

    let blockchain = Arc::new(RwLock::new(blockchain_data));

    let mut tasks = Tasks::new();

    // # Bootstrap phase
    //
    // done at every startup: we need to bootstrap from whatever local state (including nothing)
    // to the latest network state (or close to latest). until this happen, we don't participate in the network
    // (no block creation) and our network connection(s) is only use to download data.
    //
    // Various aspects to do, similar to hermes:
    // * download all the existing blocks
    // * verify all the downloaded blocks
    // * network / peer discoveries (?)
    // * gclock sync ?

    // Read block state
    // init storage
    // create blockchain storage

    network::bootstrap(&settings.network, blockchain.clone());

    // # Active phase
    //
    // now that we have caught up (or almost caught up) we download blocks from neighbor nodes,
    // listen to announcements and actively listen to synchronous queries
    //
    // There's two simultaenous roles to this:
    // * Leader: decided after global or local evaluation. Need to create and propagate a block
    // * Non-Leader: always. receive (pushed-) blocks from other peers, investigate the correct blockchain updates
    //
    // Also receive synchronous connection queries:
    // * new nodes subscribing to updates (blocks, transactions)
    // * client GetBlocks/Headers ...

    let tpool_data: TPool<MessageId, Message> = TPool::new();
    let tpool = Arc::new(RwLock::new(tpool_data));

    // Validation of consensus settings should make sure that we always have
    // non-empty selection data.

    let stats_counter = StatsCounter::default();

    let transaction_task = {
        let tpool = tpool.clone();
        let blockchain = blockchain.clone();
        let stats_counter = stats_counter.clone();
        tasks.task_create_with_inputs("transaction", move |r| {
            transaction_task(blockchain, tpool, r, stats_counter)
        })
    };

    let block_task = {
        let blockchain = blockchain.clone();
        let clock = clock.clone();
        let stats_counter = stats_counter.clone();
        tasks.task_create_with_inputs("block", move |r| {
            block_task(blockchain, clock, r, stats_counter)
        })
    };

    let client_task = {
        let blockchain = blockchain.clone();
        tasks.task_create_with_inputs("client-query", move |r| client::client_task(blockchain, r))
    };

    // ** TODO **
    // setup_network
    //  connection-events:
    //    poll:
    //      recv_transaction:
    //         check_transaction_valid
    //         add transaction to pool
    //      recv_block:
    //         check block valid
    //         try to extend blockchain with block
    //         update utxo state
    //         flush transaction pool if any txid made it
    //      get block(s):
    //         try to answer
    //
    {
        let client_msgbox = client_task.clone();
        let transaction_msgbox = transaction_task.clone();
        let block_msgbox = block_task.clone();
        let config = settings.network.clone();
        let channels = network::Channels {
            client_box: client_msgbox,
            transaction_box: transaction_msgbox,
            block_box: block_msgbox,
        };
        tasks.task_create("network", move || {
            network::run(config, channels);
        });
    };

    if let Some(secret) = leader_secret
    // == settings::start::Leadership::Yes
    //    && leadership::selection::can_lead(&selection) == leadership::IsLeading::Yes
    {
        let tpool = tpool.clone();
        let clock = clock.clone();
        let block_task = block_task.clone();
        let blockchain = blockchain.clone();
        let leader_id =
            chain_impl_mockchain::leadership::LeaderId::Bft(secret.public().block_publickey.into());
        let pk = chain_impl_mockchain::leadership::Leader::BftLeader(secret.block_privatekey);
        tasks.task_create("leadership", move || {
            leadership_task(leader_id, pk, tpool, blockchain, clock, block_task)
        });
    };

    let rest_server = match settings.rest {
        Some(ref rest) => {
            let context = rest::Context {
                stats_counter,
                blockchain,
                transaction_task: Arc::new(Mutex::new(transaction_task)),
            };
            Some(rest::start_rest_server(rest, context)?)
        }
        None => None,
    };

    // periodically cleanup (custom):
    //   storage cleanup/packing
    //   tpool.gc()

    // FIXME some sort of join so that the main thread does something ...
    tasks.join();

    if let Some(server) = rest_server {
        server.stop().wait().unwrap()
    }

    Ok(())
}

fn main() {
    let command = match Command::load() {
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
        Ok(v) => v,
    };

    match command {
        Command::Start(start_settings) => {
            if let Err(error) = start(start_settings) {
                eprintln!("jormungandr error: {}", error);
                std::process::exit(1);
            }
        }
        Command::GeneratePrivKey(args) => {
            let priv_key_bech32 = match args.key_type {
                GenPrivKeyType::Ed25519 => gen_priv_key_bech32::<Ed25519>(),
                GenPrivKeyType::Ed25519Bip32 => gen_priv_key_bech32::<Ed25519Bip32>(),
                GenPrivKeyType::Ed25519Extended => gen_priv_key_bech32::<Ed25519Extended>(),
                GenPrivKeyType::FakeMMM => gen_priv_key_bech32::<FakeMMM>(),
                GenPrivKeyType::Curve25519_2HashDH => gen_priv_key_bech32::<Curve25519_2HashDH>(),
            };
            println!("{}", priv_key_bech32);
        }
        Command::GeneratePubKey(args) => {
            let stdin = io::stdin();
            let bech32: Bech32 = if let Some(private_key_str) = args.private_key {
                private_key_str.parse().unwrap()
            } else {
                stdin
                    .lock()
                    .lines()
                    .next()
                    .unwrap()
                    .unwrap()
                    .parse()
                    .unwrap()
            };
            let pub_key_bech32 = match bech32.hrp() {
                Ed25519::SECRET_BECH32_HRP => gen_pub_key_bech32::<Ed25519>(bech32.data()),
                Ed25519Bip32::SECRET_BECH32_HRP => {
                    gen_pub_key_bech32::<Ed25519Bip32>(bech32.data())
                }
                Ed25519Extended::SECRET_BECH32_HRP => {
                    gen_pub_key_bech32::<Ed25519Extended>(bech32.data())
                }
                FakeMMM::SECRET_BECH32_HRP => gen_pub_key_bech32::<FakeMMM>(bech32.data()),
                Curve25519_2HashDH::SECRET_BECH32_HRP => {
                    gen_pub_key_bech32::<Curve25519_2HashDH>(bech32.data())
                }
                other => panic!("Unrecognized private key bech32 HRP: {}", other),
            };
            println!("{}", pub_key_bech32);
        }
        Command::Init(init_settings) => {
            let genesis = ConfigGenesisData::from_genesis(GenesisData {
                address_discrimination: init_settings.address_discrimination,
                start_time: init_settings.blockchain_start,
                slot_duration: init_settings.slot_duration,
                epoch_stability_depth: init_settings.epoch_stability_depth,
                initial_utxos: init_settings.initial_utxos,
                bft_leaders: init_settings.bft_leaders,
                allow_account_creation: init_settings.allow_account_creation,
                linear_fees: init_settings.linear_fee,
            });

            serde_yaml::to_writer(std::io::stdout(), &genesis).unwrap();
        }
    }
}

fn gen_priv_key_bech32<K: AsymmetricKey>() -> Bech32 {
    let rng = ChaChaRng::from_rng(EntropyRng::new()).unwrap();
    let secret = K::generate(rng);
    let hrp = K::SECRET_BECH32_HRP.to_string();
    Bech32::new(hrp, secret.to_base32()).unwrap()
}

fn gen_pub_key_bech32<K: AsymmetricKey>(priv_key_bech32: &[u5]) -> Bech32 {
    let priv_key_bytes = Vec::<u8>::from_base32(priv_key_bech32).unwrap();
    let priv_key = K::secret_from_binary(&priv_key_bytes).unwrap();
    let pub_key = K::compute_public(&priv_key);
    let hrp = K::PUBLIC_BECH32_HRP.to_string();
    Bech32::new(hrp, pub_key.to_base32()).unwrap()
}
