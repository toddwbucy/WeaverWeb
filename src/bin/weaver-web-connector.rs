//! The connector: runs on the agents' own box as the operator's uid
//! and holds every box-bound reach - gate sockets, verb invocation,
//! trace sinks, the load-state observable, the declaration read. It
//! renders nothing, stores nothing, and only dials out (Spec section
//! 8, which names the connector and the one dialed link): the server's
//! address is the one line that changes when the presentation stack
//! moves.

use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use weaver_web::config::ConnectorConfig;
use weaver_web::wire;

#[derive(Parser)]
#[command(
    name = "weaver-web-connector",
    about = "The box-side half of weaver-web: the link's connector."
)]
struct Args {
    /// Path to the connector's TOML configuration file.
    #[arg(long, default_value = "/etc/weaver-web/connector.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weaver_web=info".into()),
        )
        .init();

    let args = Args::parse();
    let cfg = Arc::new(ConnectorConfig::load(&args.config)?);
    tracing::info!(
        "config loaded: {} agent(s), dialing {}",
        cfg.agents.len(),
        cfg.server
    );
    if cfg.admin_env {
        // The recommended sudoers fragment permits only the wrapper,
        // which this pairing does not invoke - and the mismatch
        // surfaces as a verb refusal, not here, unless it is named
        // here (review of #350).
        tracing::warn!(
            "admin_env = true: verbs pass WEAVER_ADMIN_CONFIG through sudo, which the \
             recommended wrapper-shape sudoers fragment denies. If verbs refuse, install \
             deploy/weaver-admin-verb and set admin_bin to it with admin_env = false, or \
             use the fragment's direct-binary alternative."
        );
    }

    tokio::select! {
        _ = wire::connector_run(cfg) => {}
        _ = shutdown_signal() => {}
    }
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    tracing::info!("shutdown signal received, stopping");
}
