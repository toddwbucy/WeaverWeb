//! gate-con: the data plane's connector, on the agent's box (Spec sections
//! 7.1 and 8). Reads its config (what `weaver-web register` wrote, plus
//! `gate_socket`), connects to the server over the link, and relays each
//! turn the server asks for to the agent's gate socket and its close back.
//! The config's path has no default: a default would be a box path in the
//! repository. Beside the service, `request` and `adopt` make and complete
//! its config on the box at an appliance's enrollment (Spec 8.2), each
//! writing JSON lines and exiting with the status its outcome names.

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;
use tokio::sync::watch;
use weaver_web::link::box_commands::{self, AdoptArgs, RequestArgs, parse_args, printer};
use weaver_web::link::client::{Backoff, LinkStatus};
use weaver_web::link::frames::Plane;
use weaver_web::link::gate_con::{self, GateConConfig};

#[derive(Parser)]
#[command(
    name = "gate-con",
    about = "The data plane's connector: relays the server's turns to the agent's gate.",
    subcommand_negates_reqs = true,
    args_conflicts_with_subcommands = true
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
    /// Path to gate-con's TOML config: the file `weaver-web register`
    /// wrote, or `adopt` completed, with `gate_socket` added at install.
    #[arg(long, required = true)]
    config: Option<PathBuf>,
}

/// The enrollment's commands on the box (Spec 8.2).
#[derive(clap::Subcommand)]
enum Command {
    /// Create the config holding this connector's new key alone, and print
    /// its certificate request. No dial.
    Request(RequestArgs),
    /// Complete the config from the pairing's answer, once its certificate
    /// is over this connector's key and verifies under its authority.
    Adopt(AdoptArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let args: Args = parse_args("gate-con", &["request", "adopt"]);
    if let Some(command) = args.command {
        let status = match command {
            Command::Request(a) => {
                box_commands::request(&mut printer("gate-con request"), Plane::Gate, &a.config)
            }
            Command::Adopt(a) => box_commands::adopt(
                &mut printer("gate-con adopt"),
                Plane::Gate,
                &a.config,
                &a.answer,
            ),
        };
        return Ok(ExitCode::from(status.code() as u8));
    }
    let config = args
        .config
        .expect("clap requires --config without a command");
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gate_con=info,weaver_web=info".into()),
        )
        .init();
    // Refuses to start on a config another party could read or swap, one
    // minted for the admin plane, or one missing a member.
    let cfg = GateConConfig::load(&config)?;
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
            // A ctrl-c handler that cannot be installed never fires, rather
            // than stopping gate-con at start into a supervisor's loop.
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
        tracing::info!("gate-con stopping: turns in flight finish within the grace");
        let _ = stop.send(true);
    });
    let (status, _) = watch::channel(LinkStatus::default());
    gate_con::run(cfg, Some(config), Backoff::default(), shutdown, &status).await?;
    Ok(ExitCode::SUCCESS)
}
