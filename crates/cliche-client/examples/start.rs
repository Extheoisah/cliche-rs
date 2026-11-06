//! Start an operator-configured cliche; no seed is accepted on the command line.
use cliche_client::process::{ManagedCliche, ProcessConfig};
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let executable = args
        .next()
        .ok_or("usage: start EXECUTABLE DATA_DIR PORT NETWORK")?;
    let data_dir = args.next().ok_or("missing data directory")?;
    let mut config = ProcessConfig::native(executable, data_dir);
    config.port = args.next().ok_or("missing port")?.parse()?;
    config.network = args.next().ok_or("missing network")?;
    let managed = ManagedCliche::start(config).await?;
    println!("{}", managed.client().ping().await?);
    managed.shutdown().await;
    Ok(())
}
