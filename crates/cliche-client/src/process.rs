//! Optional local process ownership. Commands still use WebSocket.
use crate::{Client, Error, ErrorKind, Execution};
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    sync::watch,
};

/// How to launch an operator-provided cliche installation.
#[derive(Clone)]
pub enum Launch {
    /// Native executable, including a Nix store/profile/result path.
    Native {
        /// Executable path. No shell is used.
        executable: PathBuf,
    },
    /// JVM installation. System properties are placed before `-jar`.
    Jar {
        /// Java executable path.
        java: PathBuf,
        /// cliche JAR path.
        jar: PathBuf,
    },
}
/// Managed startup configuration. Seed provisioning is exclusively operator-owned.
#[derive(Clone)]
pub struct ProcessConfig {
    /// Executable or JAR to launch.
    pub launch: Launch,
    /// Dedicated data directory; must already contain valid cliche configuration.
    pub data_dir: PathBuf,
    /// Dedicated local WebSocket port (nonzero).
    pub port: u16,
    /// mainnet, testnet, regtest, or signet.
    pub network: String,
    /// Total startup deadline, including repeated connection attempts.
    pub startup_timeout: Duration,
    /// Time allowed after termination request before force-kill.
    pub shutdown_grace: Duration,
    /// Optional expected node pubkey, checked before returning the client.
    pub expected_pubkey: Option<String>,
}
impl ProcessConfig {
    /// Configure native cliche with loopback port 12000 and mainnet.
    pub fn native(executable: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            launch: Launch::Native {
                executable: executable.into(),
            },
            data_dir: data_dir.into(),
            port: 12000,
            network: "mainnet".into(),
            startup_timeout: Duration::from_secs(30),
            shutdown_grace: Duration::from_secs(5),
            expected_pubkey: None,
        }
    }
}
/// Exit status of an owned process. None means termination by signal/no exit code.
#[derive(Clone, Copy, Debug)]
pub struct ProcessExit {
    /// OS exit code, if available.
    pub code: Option<i32>,
}
struct Owner {
    stop: watch::Sender<bool>,
    exit: watch::Receiver<Option<ProcessExit>>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}
/// Owns one local child. Client clones do not gain process ownership.
pub struct ManagedCliche {
    client: Client,
    owner: Owner,
}
impl ManagedCliche {
    /// Start cliche, retain its stdin, discard/drain diagnostics, and verify ping.
    /// Cancelling this future signals child shutdown. Never adopts an existing node.
    pub async fn start(config: ProcessConfig) -> Result<Self, Error> {
        if config.port == 0
            || !matches!(
                config.network.as_str(),
                "mainnet" | "testnet" | "regtest" | "signet"
            )
            || config.startup_timeout.is_zero()
            || config.startup_timeout > Duration::from_secs(86400)
            || config.shutdown_grace > Duration::from_secs(300)
        {
            return Err(Error::invalid("invalid managed process configuration"));
        }
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|_| process_error("data directory unavailable"))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(config.data_dir.join(".cliche-client.lock"))
            .map_err(|_| process_error("ownership lock unavailable"))?;
        lock.try_lock_exclusive()
            .map_err(|_| process_error("data directory already managed by another client"))?;
        // Preflight prevents accidentally adopting a listener already present.
        // cliche does not honor our lock: an unrelated process can still race us.
        let preflight =
            std::net::TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, config.port))
                .map_err(|_| process_error("managed websocket port is already in use"))?;
        let mut command = match &config.launch {
            Launch::Native { executable } => Command::new(executable),
            Launch::Jar { java, .. } => Command::new(java),
        };
        command
            .arg(format!("-Dcliche.datadir={}", config.data_dir.display()))
            .arg("-Dcliche.websocket.host=127.0.0.1")
            .arg(format!("-Dcliche.websocket.port={}", config.port))
            .arg(format!("-Dcliche.network={}", config.network))
            .arg("-Dcliche.json.compact=true");
        if let Launch::Jar { jar, .. } = &config.launch {
            command.arg("-jar").arg(jar);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        drop(preflight);
        let child = command
            .spawn()
            .map_err(|_| process_error("could not spawn cliche executable"))?;
        let (stop, stop_rx) = watch::channel(false);
        let (exit_tx, exit) = watch::channel(None);
        let mut owner = Owner { stop, exit };
        tokio::spawn(supervise(
            child,
            lock,
            config.shutdown_grace,
            stop_rx,
            exit_tx,
        ));
        let endpoint = format!("ws://127.0.0.1:{}/", config.port);
        let connect = async {
            loop {
                let mut builder =
                    Client::builder(&endpoint).connect_timeout(Duration::from_millis(250));
                if let Some(pubkey) = &config.expected_pubkey {
                    builder = builder.expected_pubkey(pubkey);
                }
                match builder.connect().await {
                    Ok(client) => return Ok(client),
                    Err(error) if error.kind == ErrorKind::Protocol => return Err(error),
                    Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
                }
            }
        };
        let client = tokio::select! {
            biased;
            _ = async { let _ = owner.exit.wait_for(|exit| exit.is_some()).await; } => return Err(process_error("cliche exited before readiness")),
            result = tokio::time::timeout(config.startup_timeout, connect) => match result {
                Ok(Ok(client)) => client,
                Ok(Err(error)) => { owner.stop.send_replace(true); let _ = owner.exit.wait_for(|e| e.is_some()).await; return Err(error); }
                Err(_) => { owner.stop.send_replace(true); let _ = owner.exit.wait_for(|e| e.is_some()).await; return Err(process_error("cliche startup deadline exceeded")); }
            }
        };
        let mut exit = owner.exit.clone();
        let monitor = client.clone();
        tokio::spawn(async move {
            let _ = exit.wait_for(|e| e.is_some()).await;
            monitor.close().await;
        });
        Ok(Self { client, owner })
    }
    /// Shared client reference; cloning it does not keep process ownership alive.
    pub fn client(&self) -> &Client {
        &self.client
    }
    /// Current process exit status, if it has exited.
    pub fn exit_status(&self) -> Option<ProcessExit> {
        *self.owner.exit.borrow()
    }
    /// Close the connection, terminate, and reap the child. Consumes ownership.
    pub async fn shutdown(mut self) -> ProcessExit {
        self.client.close().await;
        self.owner.stop.send_replace(true);
        let _ = self.owner.exit.wait_for(|e| e.is_some()).await;
        self.exit_status().unwrap_or(ProcessExit { code: None })
    }
}
fn process_error(message: &str) -> Error {
    Error::new(ErrorKind::Process, message, Execution::NotSent)
}
async fn drain(mut pipe: impl tokio::io::AsyncRead + Unpin) {
    let mut buffer = [0_u8; 8192];
    while let Ok(count) = pipe.read(&mut buffer).await {
        if count == 0 {
            break;
        }
    }
}
async fn supervise(
    mut child: Child,
    _lock: File,
    grace: Duration,
    mut stop: watch::Receiver<bool>,
    exit: watch::Sender<Option<ProcessExit>>,
) {
    let _stdin = child.stdin.take(); // Keep open until the owned process ends.
    let stdout = child.stdout.take().map(|pipe| tokio::spawn(drain(pipe)));
    let stderr = child.stderr.take().map(|pipe| tokio::spawn(drain(pipe)));
    let status = tokio::select! {
        status = child.wait() => status.ok(),
        _ = async { let _ = stop.wait_for(|stop| *stop).await; } => {
            #[cfg(unix)]
            if let Some(pid) = child.id().and_then(|id| i32::try_from(id).ok()) {
                let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGTERM);
            }
            #[cfg(not(unix))]
            let _ = child.start_kill();
            match tokio::time::timeout(grace, child.wait()).await {
                Ok(status) => status.ok(),
                Err(_) => { let _ = child.start_kill(); child.wait().await.ok() }
            }
        }
    };
    // Descendants holding pipes must not delay shutdown or leak drain tasks.
    for task in [stdout, stderr].into_iter().flatten() {
        task.abort();
        let _ = task.await;
    }
    exit.send_replace(Some(ProcessExit {
        code: status.and_then(|s| s.code()),
    }));
}
