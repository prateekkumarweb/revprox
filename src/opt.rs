use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum Opt {
    /// Start client
    Client {
        /// Config file
        #[structopt(short, long, parse(from_os_str))]
        config: PathBuf,
    },
    /// Start server
    Server {
        /// Config file
        #[structopt(short, long, parse(from_os_str))]
        config: PathBuf,

        /// Port to run on
        #[structopt(short, long, default_value = "9000")]
        port: u16,

        /// Use tls
        #[structopt(long)]
        tls: bool,
    },
}
