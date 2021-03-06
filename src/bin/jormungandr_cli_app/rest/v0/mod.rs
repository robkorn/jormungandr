mod node;
mod utxo;

use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum V0 {
    /// Node information
    Node(node::Node),
    /// UTXO information
    Utxo(utxo::Utxo),
}

impl V0 {
    pub fn exec(self) {
        match self {
            V0::Node(node) => node.exec(),
            V0::Utxo(utxo) => utxo.exec(),
        }
    }
}
