//! The server: HTTP for browsers, the store, and the link listener
//! the connector dials (PRD section 5). Holds everything that is not
//! box-bound and reaches the box only through the link.

use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use weaver_web::config::ServerConfig;
use weaver_web::traceview::TraceViews;
use weaver_web::{store, web, wire};

#[derive(Parser)]
#[command(
    name = "weaver-web",
    about = "The WeaverTools suite's frontend server."
)]
struct Args {
    /// Path to the server's TOML configuration file.
    #[arg(long, default_value = "/etc/weaver-web/config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "weaver_web=info,sqlx=warn".into()),
        )
        .init();

    let args = Args::parse();
    let cfg = Arc::new(ServerConfig::load(&args.config)?);
    tracing::info!("config loaded: {} provider(s)", cfg.providers.len());

    let store = store::Store::connect(&cfg.database).await?;
    tracing::info!("store connected, migrations applied");

    let link = wire::Link::new();
    let traces = TraceViews::new();

    let link_listener = tokio::net::TcpListener::bind(&cfg.link_listen).await?;
    tracing::info!("link listening on {}", cfg.link_listen);
    let (ev_tx, mut ev_rx) = tokio::sync::mpsc::channel(1024);
    tokio::spawn(wire::serve(link.clone(), link_listener, ev_tx));

    // The link event pump: each box's hello starts trace views
    // for its agents (roster-by-hello, Spec section 8),
    // trace frames feed the rings, and a box's
    // link loss marks exactly its own agents' views.
    {
        let traces = traces.clone();
        tokio::spawn(async move {
            while let Some(ev) = ev_rx.recv().await {
                match ev {
                    wire::LinkEvent::Hello(agents) => {
                        for a in &agents {
                            // A view that already holds events is
                            // getting a fresh backfill: bracket it.
                            if traces.has_events(a) {
                                traces.mark(a, "link reconnected: a fresh backfill follows");
                            }
                            traces.ensure(a);
                        }
                        tracing::info!("hello: {} agent(s) admitted", agents.len());
                    }
                    wire::LinkEvent::Trace { agent, event } => {
                        traces.ingest(&agent, event);
                    }
                    wire::LinkEvent::Down(agents) => {
                        for a in &agents {
                            traces.mark(a, "link to this agent's box lost");
                        }
                    }
                }
            }
        });
    }

    // **The instrument's surfaces are mounted on the store alone**, per
    // Spec section 6: a surface that renders what is kept reads the store
    // and nothing else. They carry their own state rather than the
    // legacy admin `AppState`, keeping record reads independent of the link.
    let instrument = weaver_web::surfaces::routes().with_state(store.clone());

    let state = web::AppState {
        cfg: cfg.clone(),
        traces,
        link,
    };

    let listener = tokio::net::TcpListener::bind(&cfg.listen).await?;
    tracing::info!("listening on {}", cfg.listen);
    axum::serve(listener, web::router(state).merge(instrument))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
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
