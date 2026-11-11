//! `weaver-web-stage --params <file> --out <dir>`: the install's staging
//! tool (Spec 8.1, `weaver_web::stage`). It renders the units, composes the
//! connector configs and writes them with a manifest under `<dir>`, as the
//! operator's own user; applying them is the operator's. One JSON object on
//! stdout, the exit status agreeing, the register verbs' shape.

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "weaver-web-stage",
    about = "Stage the units and connector configs of an install, with a manifest, for the operator to apply."
)]
struct Args {
    /// The parameter file, kept outside the repository (Spec 8.1).
    #[arg(long)]
    params: PathBuf,
    /// The directory to stage into: absent, or empty, the operator's own and
    /// 0700 or tighter, since it will hold the connectors' keys.
    #[arg(long)]
    out: PathBuf,
}

/// Stage, print the one JSON answer on stdout, and exit 0 where it is `ok`
/// and 1 otherwise. A usage error is clap's, before any answer, as the
/// register verbs' is.
fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let answer = run(&args);
    let ok = answer["ok"].as_bool().unwrap_or(false);
    println!("{answer}");
    if ok {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

/// The answer for one invocation: read the parameter file, take `--out` as
/// `stage::out_path` gives it, and stage as the effective uid.
fn run(args: &Args) -> serde_json::Value {
    let refused = |e: anyhow::Error| serde_json::json!({ "verb": "stage", "ok": false, "error": format!("{e:#}") });
    let text = match std::fs::read_to_string(&args.params) {
        Ok(t) => t,
        Err(e) => {
            return refused(anyhow::anyhow!(
                "the parameter file {} cannot be read: {e}",
                args.params.display()
            ));
        }
    };
    let cwd = if args.out.is_absolute() {
        PathBuf::from("/")
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd,
            Err(e) => return refused(anyhow::anyhow!("the working directory cannot be read: {e}")),
        }
    };
    let out = weaver_web::stage::out_path(&cwd, &args.out);
    // SAFETY: geteuid takes nothing and cannot fail.
    let euid = unsafe { libc::geteuid() };
    match weaver_web::stage::stage(&text, &out, euid) {
        Ok(staged) => serde_json::json!({
            "verb": "stage",
            "ok": true,
            "manifest": staged.manifest,
            "lines": staged.lines,
        }),
        Err(e) => refused(e),
    }
}
