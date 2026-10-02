//! admin-con: the management plane's connector, beside the agent as its own
//! unprivileged service user (Spec sections 7.2 and 8). Reads its config
//! (what `weaver-web register` wrote, plus `trace_file`), connects to the
//! server over the link, tails the agent's trace file with replay and marked
//! discontinuities, and answers verb asks through its invoker. **The invoker
//! it ships runs nothing**: it answers an empty `grants`, so the ceiling is
//! empty until WeaverAgent #50 lands. The config's path has no default: a
//! default would be a box path in the repository.

use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;
use weaver_web::link::admin_con::{self, AdminConConfig, NoVerbs};
use weaver_web::link::client::{Backoff, LinkStatus};

#[derive(Parser)]
#[command(
    name = "admin-con",
    about = "The management plane's connector: relays the agent's trace and answers the server's verbs."
)]
struct Args {
    /// Path to admin-con's TOML config: the file `weaver-web register`
    /// wrote, with `trace_file` added at install.
    #[arg(long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "admin_con=info,weaver_web=info".into()),
        )
        .init();
    let args = Args::parse();
    // Refuses to start on a config another party could read or swap, one
    // minted for the gate plane, or one missing a member.
    let cfg = AdminConConfig::load(&args.config)?;
    tracing::info!(
        "admin-con for {} tailing its trace file, server {}, an empty ceiling until WeaverAgent #50 lands",
        cfg.link.agent,
        cfg.link.server
    );
    let (stop, shutdown) = watch::channel(false);
    tokio::spawn(async move {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            // A ctrl-c handler that cannot be installed never fires, rather
            // than stopping admin-con at start into a supervisor's loop.
            _ = async {
                if tokio::signal::ctrl_c().await.is_err() {
                    std::future::pending::<()>().await;
                }
            } => {}
            _ = async {
                match term.as_mut() {
                    Some(t) => { t.recv().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {}
        }
        tracing::info!("admin-con stopping: a verb in flight finishes within the grace");
        let _ = stop.send(true);
    });
    let (status, _) = watch::channel(LinkStatus::default());
    admin_con::run(
        cfg,
        Arc::new(NoVerbs),
        Some(args.config),
        Backoff::default(),
        shutdown,
        &status,
    )
    .await
}
