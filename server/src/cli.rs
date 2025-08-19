#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[clap(subcommand)]
    pub(crate) command: Action,
}

#[derive(Debug, clap::Subcommand)]
pub enum Action {
    /// Get telemetry data from a local socket and display them as TUI
    Local(Local),
    /// Aggregate multiple TCP stream of telemetry data and display them as TUI
    Aggregator(Aggregator),
    /// Get telemetry data from a local socket and serve them as TCP server
    Proxy(Proxy),
}

#[derive(Debug, clap::Parser)]
pub struct Local {
    #[clap(long)]
    /// Local socket path
    pub(crate) socket: String,
    /// Delete the socket file even if exist yet
    #[clap(long, default_value = "false")]
    pub(crate) force: bool,
}

#[derive(Debug, clap::Parser)]
pub struct Proxy {
    #[clap(long)]
    /// TCP server address
    pub address: String,
    #[clap(long)]
    /// TCP server port
    pub port: u16,
    #[clap(long)]
    /// Local socket path
    pub(crate) socket: String,
    /// Delete the socket file even if exist yet
    #[clap(long, default_value = "false")]
    pub(crate) force: bool,
}

fn parse_backend(s: &str) -> Result<(String, u16), String> {
    let (addr, port) = s
        .split_once(':')
        .ok_or_else(|| format!("Invalid backend: {}", s))?;

    Ok((
        addr.to_string(),
        port.parse()
            .map_err(|_| format!("Invalid backend: {}", s))?,
    ))
}

#[derive(Debug, clap::Parser)]
pub struct Aggregator {
    #[clap(long, short = 'b', value_parser = parse_backend)]
    backends: Vec<(String, u16)>,
}
