mod build;

use structopt::StructOpt;

/// Send request to node REST API
#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum Transaction {
    /// Build signed transaction binary blob and write it to stdout
    Build(build::Build),
}

impl Transaction {
    pub fn exec(self) {
        match self {
            Transaction::Build(build) => build.exec(),
        }
    }
}
