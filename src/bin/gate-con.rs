//! gate-con: the data plane's connector, on the agent's box (Spec sections
//! 7.1 and 8). Reads its config (what `weaver-web register` wrote, plus
//! `gate_socket`), connects to the server over the link, and relays each
//! turn the server asks for to the agent's gate socket and its close back.
//! The config's path has no default: a default would be a box path in the
//! repository.

use clap::Parser;
use std::path::PathBuf;
use tokio::sync::watch;
use weaver_web::link::client::{Backoff, LinkStatus};
use weaver_web::link::gate_con::{self, GateConConfig};

#[derive(Parser)]
#[command(
    name = "gate-con",
    about = "The data plane's connector: relays the server's turns to the agent's gate."
)]
struct Args {
    /// Path to gate-con's TOML config: the file `weaver-web register`
    /// wrote, with `gate_socket` added at install.
    #[arg(long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gate_con=info,weaver_web=info".into()),
        )
        .init();
    let args = Args::parse();
    // Refuses to start on a config another party could read or swap, one
    // minted for the admin plane, or one missing a member.
    let cfg = GateConConfig::load(&args.config)?;
    tracing::info!(
        "gate-con for {} relaying to its gate, {} turns in flight at most, server {}",
        cfg.link.agent,
        cfg.turns_in_flight,
        cfg.link.server
    );
    let (stop, shutdown) = watch::channel(false);
    tokio::spawn(async move {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                match term.as_mut() {
                    Some(t) => { t.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {}
        }
        tracing::info!("gate-con stopping: turns in flight finish within the grace");
        let _ = stop.send(true);
    });
    let (status, _) = watch::channel(LinkStatus::default());
    gate_con::run(cfg, Backoff::default(), shutdown, &status).await
}
