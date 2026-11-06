//! Standalone gateway. Plain gRPC is loopback-only unless a trusted proxy is explicit.
use clap::Parser;
use cliche_client::{
    Client, ReconnectPolicy,
    events::ConnectionState,
    process::{ManagedCliche, ProcessConfig},
};
use cliche_gateway::{Gateway, GatewayPolicy, proto::cliche_service_server::ClicheServiceServer};
use std::{collections::HashSet, net::SocketAddr, path::PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "JSON gRPC gateway for cliche; defaults to read-only command access"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:12001")]
    listen: SocketAddr,
    #[arg(
        long,
        default_value = "ws://127.0.0.1:12000/",
        conflicts_with = "executable"
    )]
    cliche_url: String,
    #[arg(long, requires = "data_dir")]
    executable: Option<PathBuf>,
    #[arg(long, requires = "executable")]
    data_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 12000)]
    cliche_port: u16,
    #[arg(long, default_value = "mainnet")]
    network: String,
    #[arg(long)]
    expected_pubkey: Option<String>,
    #[arg(long)]
    expected_chain: Option<String>,
    #[arg(long)]
    reconnect: bool,
    #[arg(
        long,
        default_value = "ping,get-info,check-payment,list-payments,get-address"
    )]
    allow_methods: String,
    #[arg(long)]
    disable_events: bool,
    /// Opt into plaintext behind an authenticated TLS-terminating trusted proxy.
    #[arg(long)]
    trusted_proxy: bool,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    // Secrets only come from the environment, never process arguments.
    let token = std::env::var("CLICHE_GATEWAY_TOKEN").ok();
    if !args.listen.ip().is_loopback()
        && (!args.trusted_proxy || token.as_ref().is_none_or(|t| t.is_empty()))
    {
        return Err("nonloopback serving requires --trusted-proxy and CLICHE_GATEWAY_TOKEN".into());
    }
    let managed = if let Some(executable) = args.executable {
        if args.reconnect || args.expected_chain.is_some() {
            return Err("managed mode currently uses process exit monitoring; reconnect/expected-chain are attach-only".into());
        }
        let mut config =
            ProcessConfig::native(executable, args.data_dir.ok_or("missing data directory")?);
        config.port = args.cliche_port;
        config.network = args.network;
        config.expected_pubkey = args.expected_pubkey.clone();
        Some(ManagedCliche::start(config).await?)
    } else {
        None
    };
    let client = if let Some(managed) = &managed {
        managed.client().clone()
    } else {
        let mut builder = Client::builder(args.cliche_url);
        if args.reconnect {
            builder = builder.reconnect(ReconnectPolicy::default());
        }
        if let Some(pubkey) = args.expected_pubkey {
            builder = builder.expected_pubkey(pubkey);
        }
        if let Some(chain) = args.expected_chain {
            builder = builder.expected_chain(chain);
        }
        builder.connect().await?
    };
    let methods = if args.allow_methods == "*" {
        None
    } else {
        Some(
            args.allow_methods
                .split(',')
                .map(str::to_owned)
                .collect::<HashSet<_>>(),
        )
    };
    let policy = GatewayPolicy {
        bearer_token: token,
        allowed_methods: methods,
        allow_events: !args.disable_events,
        ..Default::default()
    };
    let service = Gateway::new(client.clone(), policy)?;
    let (health, health_service) = tonic_health::server::health_reporter();
    let mut status = client.watch_status();
    let monitor = tokio::spawn(async move {
        loop {
            let state = *status.borrow_and_update();
            health
                .set_service_status(
                    "cliche.v1.ClicheService",
                    if state == ConnectionState::Connected {
                        tonic_health::ServingStatus::Serving
                    } else {
                        tonic_health::ServingStatus::NotServing
                    },
                )
                .await;
            if state == ConnectionState::Closed || status.changed().await.is_err() {
                break;
            }
        }
    });
    eprintln!("cliche gRPC gateway listening on {}", args.listen);
    let result = tonic::transport::Server::builder()
        .add_service(health_service)
        .add_service(
            ClicheServiceServer::new(service)
                .max_decoding_message_size(4 * 1024 * 1024 + 1024)
                .max_encoding_message_size(4 * 1024 * 1024 + 1024),
        )
        .serve_with_shutdown(args.listen, async {
            shutdown_signal().await;
            client.close().await;
        })
        .await;
    client.close().await;
    if let Some(managed) = managed {
        managed.shutdown().await;
    }
    let _ = monitor.await;
    result?;
    Ok(())
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
