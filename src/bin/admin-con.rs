//! admin-con: the management plane's connector, beside the agent as its own
//! service user (Spec sections 7.2 and 8). Reads its config (what
//! `weaver-web register` wrote, plus the box facts the install adds),
//! connects to the server over the link, relays the agent's trace, read
//! through the relay's door, with replay and marked discontinuities, and
//! answers verb asks through its
//! sudo invoker, the one privileged invocation in this crate: the fixed
//! `weaver-admin <verb> <agent>` lines the box's sudo rule grants, with
//! nothing from the link in the command. A conduit, it never acts on the
//! agent on its own: its stop waits for the verb in flight and invokes
//! nothing. The config's path has no default: a default would be a box path
//! in the repository. Beside the service, `request`, `pair` and `adopt` enroll
//! an appliance (Spec 8.2), and `setup describe` and `setup report` are the
//! setup connection's two asks (Spec 8.3), each writing JSON lines and exiting
//! with the status its outcome names.

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;
use tokio::sync::watch;
use weaver_web::link::admin_con::{self, AdminConConfig};
use weaver_web::link::box_commands::{
    self, AdoptArgs, PairArgs, RequestArgs, SetupCommand, parse_args, printer,
};
use weaver_web::link::client::{Backoff, LinkStatus};
use weaver_web::link::frames::Plane;

#[derive(Parser)]
#[command(
    name = "admin-con",
    about = "The management plane's connector: relays the agent's trace and answers the server's verbs.",
    subcommand_negates_reqs = true,
    args_conflicts_with_subcommands = true
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
    /// Path to admin-con's TOML config: the file `weaver-web register`
    /// wrote, or `adopt` completed, with `admin_socket` and `admin_log`
    /// added once an agent is laid (Spec 7.2).
    #[arg(long, required = true)]
    config: Option<PathBuf>,
}

/// The enrollment's commands and the setup connection's asks (Spec 8.2,
/// 8.3).
#[derive(clap::Subcommand)]
enum Command {
    /// Create the config holding this connector's new key alone, and print
    /// its certificate request. No dial.
    Request(RequestArgs),
    /// Dial the console under the pairing protocol, show the code, and
    /// write the console's answer once the operator enters it.
    Pair(PairArgs),
    /// Complete the config from the pairing's answer, once its certificate
    /// is over this connector's key and verifies under its authority.
    Adopt(AdoptArgs),
    /// The setup connection's asks, under the admin plane's credential.
    #[command(subcommand)]
    Setup(SetupCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let args: Args = parse_args("admin-con", &["request", "pair", "adopt", "setup"]);
    if let Some(command) = args.command {
        let status = match command {
            Command::Request(a) => {
                box_commands::request(&mut printer("admin-con request"), Plane::Admin, &a.config)
            }
            Command::Pair(a) => box_commands::pair(&mut printer("admin-con pair"), &a).await,
            Command::Adopt(a) => box_commands::adopt(
                &mut printer("admin-con adopt"),
                Plane::Admin,
                &a.config,
                &a.answer,
            ),
            Command::Setup(SetupCommand::Describe { config }) => {
                box_commands::setup_describe(&mut printer("admin-con setup describe"), &config)
                    .await
            }
            Command::Setup(SetupCommand::Report { config, stage }) => {
                box_commands::setup_report(
                    &mut printer("admin-con setup report"),
                    &config,
                    stage.into(),
                    std::io::stdin().lock(),
                )
                .await
            }
        };
        return Ok(ExitCode::from(status.code() as u8));
    }
    let config = args
        .config
        .expect("clap requires --config without a command");
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "admin_con=info,weaver_web=info".into()),
        )
        .init();
    // Refuses to start on a config another party could read or swap, one
    // minted for the gate plane, or one missing a member.
    let cfg = AdminConConfig::load(&config)?;
    // **One fact chooses** (Spec 7.2): the laid agent's box facts, all
    // three or none, pick the invoker in `admin_con::run_as_configured` and this line.
    match cfg.laid() {
        Some(laid) => tracing::info!(
            "admin-con for {} reading its trace through the relay, server {}, verbs asked of the service at {}",
            cfg.link.agent,
            cfg.link.server,
            laid.admin_socket.display()
        ),
        None => tracing::info!(
            "admin-con for {}, server {}: no agent is laid, so no relay is dialed and no verb is invoked",
            cfg.link.agent,
            cfg.link.server
        ),
    }
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
        tracing::info!(
            "admin-con stopping: the verb in flight is waited for, and nothing is invoked"
        );
        let _ = stop.send(true);
    });
    let (status, _) = watch::channel(LinkStatus::default());
    admin_con::run_as_configured(cfg, Some(config), Backoff::default(), shutdown, &status).await?;
    Ok(ExitCode::SUCCESS)
}
