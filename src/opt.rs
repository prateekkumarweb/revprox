use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Opt {
    /// Config file
    #[structopt(short, long, parse(from_os_str))]
    pub config: PathBuf,

    /// Port to run on
    #[structopt(short, long, default_value = "9000")]
    pub port: u16,
}
