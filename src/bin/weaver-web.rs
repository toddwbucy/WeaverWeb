//! The server: HTTP for browsers, the store, the register of agents and
//! the link listener the two connectors connect to (Spec sections 2.12 and
//! 8), and the register verbs the operator runs on it. Holds everything
//! that is not box-bound and reaches no agent: the connectors are the one
//! party that does, and they are clients of this process.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use weaver_web::config::ServerConfig;
use weaver_web::link::{Authority, Listener, Plane, verbs};
use weaver_web::traceview::TraceViews;
use weaver_web::{store, web};

#[derive(Parser)]
#[command(
    name = "weaver-web",
    about = "The WeaverTools suite's frontend server, and its register verbs."
)]
struct Args {
    /// Path to the server's TOML configuration file.
    #[arg(long, default_value = "/etc/weaver-web/config.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve: load the authority, connect the store, start the listener
    /// and the HTTP surfaces. The default when no verb is given.
    Serve,
    /// The server's authority.
    Authority {
        #[command(subcommand)]
        verb: AuthorityVerb,
    },
    /// Register an agent: its row, two credentials, two client configs.
    Register {
        /// The box the agent reports from, as the operator names it.
        r#box: String,
        /// The agent's name, the one admin knows it by on its box.
        name: String,
        /// Where the two client configs are written.
        #[arg(long)]
        out: PathBuf,
        /// Who is registering, for the row's author member.
        #[arg(long)]
        author: Option<String>,
    },
    /// Revoke one credential; its live connection closes in this act.
    Revoke {
        /// The agent as ag-<sixteen hex> or box/name.
        agent: String,
        /// gate or admin.
        plane: Plane,
        #[arg(long)]
        author: Option<String>,
    },
    /// Rotate both credentials; both planes drop until the new configs
    /// are installed.
    Rotate {
        /// The agent as ag-<sixteen hex> or box/name.
        agent: String,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        author: Option<String>,
    },
    /// The register as it stands, presence derived on each row.
    Agents,
}

#[derive(Subcommand)]
enum AuthorityVerb {
    /// Create the authority once; refuses to overwrite one that stands.
    /// Needs the store, where its audit record is written first.
    Init {
        /// Further names or addresses the server's certificate carries,
        /// beside the config's server_name.
        #[arg(long)]
        san: Vec<String>,
        #[arg(long)]
        author: Option<String>,
    },
    /// Replace the authority, which revokes every credential.
    Rotate {
        #[arg(long)]
        san: Vec<String>,
        #[arg(long)]
        author: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let cfg = Arc::new(ServerConfig::load(&args.config)?);
    match args.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(cfg).await,
        command => {
            let answer = run_verb(&cfg, command).await;
            println!("{}", serde_json::to_string_pretty(&answer.value)?);
            if answer.ok {
                Ok(())
            } else {
                std::process::exit(1)
            }
        }
    }
}

async fn run_verb(cfg: &ServerConfig, command: Command) -> verbs::Answer {
    match command {
        Command::Serve => unreachable!("serve is not a verb"),
        Command::Authority {
            verb: AuthorityVerb::Init { san, author },
        } => match store::Store::connect(&cfg.database).await {
            Ok(store) => verbs::authority_init(&store, cfg, &san, author.as_deref()).await,
            Err(e) => verbs::Answer {
                value: serde_json::json!({
                    "verb": "authority init",
                    "ok": false,
                    "error": format!("the store could not be reached, so nothing was done: {e:#}"),
                }),
                ok: false,
            },
        },
        Command::Authority {
            verb: AuthorityVerb::Rotate { san, author },
        } => match store::Store::connect(&cfg.database).await {
            Ok(store) => verbs::authority_rotate(&store, cfg, &san, author.as_deref()).await,
            Err(e) => verbs::Answer {
                value: serde_json::json!({ "verb": "authority rotate", "ok": false, "error": format!("{e:#}") }),
                ok: false,
            },
        },
        Command::Register {
            r#box,
            name,
            out,
            author,
        } => {
            with_store_and_authority(cfg, "register", |store, authority| async move {
                verbs::register(
                    &store,
                    cfg,
                    &authority,
                    &r#box,
                    &name,
                    &out,
                    author.as_deref(),
                )
                .await
            })
            .await
        }
        Command::Revoke {
            agent,
            plane,
            author,
        } => match store::Store::connect(&cfg.database).await {
            Ok(store) => verbs::revoke(&store, &agent, plane, author.as_deref()).await,
            Err(e) => verbs::Answer {
                value: serde_json::json!({ "verb": "revoke", "ok": false, "error": format!("{e:#}") }),
                ok: false,
            },
        },
        Command::Rotate { agent, out, author } => {
            with_store_and_authority(cfg, "rotate", |store, authority| async move {
                verbs::rotate(&store, cfg, &authority, &agent, &out, author.as_deref()).await
            })
            .await
        }
        Command::Agents => match store::Store::connect(&cfg.database).await {
            Ok(store) => verbs::agents(&store).await,
            Err(e) => verbs::Answer {
                value: serde_json::json!({ "verb": "agents", "ok": false, "error": format!("{e:#}") }),
                ok: false,
            },
        },
    }
}

async fn with_store_and_authority<F, Fut>(cfg: &ServerConfig, verb: &str, f: F) -> verbs::Answer
where
    F: FnOnce(store::Store, Authority) -> Fut,
    Fut: std::future::Future<Output = verbs::Answer>,
{
    let authority = match Authority::load(&cfg.authority_dir) {
        Ok(a) => a,
        Err(e) => {
            return verbs::Answer {
                value: serde_json::json!({ "verb": verb, "ok": false, "error": format!("{e:#}") }),
                ok: false,
            };
        }
    };
    if let Err(e) = verbs::name_agrees(cfg, &authority) {
        return verbs::Answer {
            value: serde_json::json!({ "verb": verb, "ok": false, "error": format!("{e:#}") }),
            ok: false,
        };
    }
    match store::Store::connect(&cfg.database).await {
        Ok(store) => f(store, authority).await,
        Err(e) => verbs::Answer {
            value: serde_json::json!({ "verb": verb, "ok": false, "error": format!("{e:#}") }),
            ok: false,
        },
    }
}

async fn serve(cfg: Arc<ServerConfig>) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weaver_web=info,sqlx=warn".into()),
        )
        .init();
    tracing::info!("config loaded: {} provider(s)", cfg.providers.len());

    // **The authority is loaded before anything listens, and never
    // minted here** (Spec section 8): absent, the server refuses to start.
    let authority = Authority::load(&cfg.authority_dir)?;
    if authority.server_name() != cfg.server_name {
        anyhow::bail!(
            "the config's server_name is {} but the authority was minted for {}; the connectors verify the latter, so the config is changed back or the authority rotated",
            cfg.server_name,
            authority.server_name()
        );
    }
    tracing::info!(
        "authority loaded, fingerprint {}",
        &authority.fingerprint()[..12]
    );

    let store = store::Store::connect(&cfg.database).await?;
    tracing::info!("store connected, migrations applied");

    let link = Listener::start(
        store.clone(),
        &authority,
        &cfg.link_listen,
        Duration::from_secs(cfg.silence_bound_secs),
    )
    .await?;

    // **The instrument's surfaces are mounted on the store alone**, per
    // Spec section 6: a surface that renders what is kept reads the store
    // and nothing else. They carry their own state rather than the
    // legacy admin `AppState`, keeping record reads independent of the link.
    let instrument = weaver_web::surfaces::routes().with_state(store.clone());

    // The legacy admin routes answer 503 and reach no agent; their trace
    // views are the listener's live window, so the one ring per agent is
    // shared rather than duplicated until act 5 removes them.
    let state = web::AppState {
        cfg: cfg.clone(),
        traces: TraceViews::clone(link.windows()),
    };

    let listener = tokio::net::TcpListener::bind(&cfg.listen).await?;
    tracing::info!("listening on {}", cfg.listen);
    // The listener halting itself (its lock session lost) ends the process,
    // so the operator's supervisor restarts it into a clean start.
    let halting = link.clone();
    tokio::select! {
        served = axum::serve(listener, web::router(state).merge(instrument))
            .with_graceful_shutdown(shutdown_signal()) => served?,
        why = halting.halted() => anyhow::bail!("the listener halted: {why}"),
    }
    drop(link);
    Ok(())
}

/// SIGTERM or ctrl-c stops accepting requests and lets in-flight ones
/// finish.
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    tracing::info!("shutdown signal received, stopping");
}
