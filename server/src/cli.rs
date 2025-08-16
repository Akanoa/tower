#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[clap(long)]
    pub(crate) socket: String,
    /// Delete the socket file even if exist yet
    #[clap(long, default_value = "false")]
    pub(crate) force: bool,
}
