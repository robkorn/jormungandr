use super::{grpc, Connection, NetworkBlockConfig};

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

// This could be replaced by a boxed network-core abstraction.
pub type Client<B: NetworkBlockConfig> = grpc::Client<B>;

/// Map of client connections established to other peers.
pub struct Connections<B: NetworkBlockConfig> {
    shared: Arc<RwLock<HashMap<Connection, Client<B>>>>,
}

impl<B: NetworkBlockConfig> Clone for Connections<B> {
    fn clone(&self) -> Self {
        Connections {
            shared: self.shared.clone(),
        }
    }
}

impl<B: NetworkBlockConfig> Default for Connections<B> {
    fn default() -> Self {
        Connections {
            shared: Default::default(),
        }
    }
}

impl<B: NetworkBlockConfig> Connections<B> {
    pub fn add_connection(&self, addr: Connection, state: Client<B>) {
        let map = self.shared.write().unwrap();
        map.insert(addr, state);
    }
}
