//! **The commands on the box** (Spec 8.2, 8.3), on the connectors' binaries
//! beside the service, which runs as built: `request` and `adopt` on both,
//! and `pair` and `setup describe|report` on admin-con. Each writes its lines
//! through `box_line::Printer` and exits with the status its outcome names.
//!
//! **The pairing takes three steps on the box**, so each connector makes its
//! own key under its own account and nothing of this repository runs as root
//! to parse what the network sent: `request` under each connector's account,
//! `pair` under an unprivileged account the image names, and `adopt` under
//! each connector's account again.

use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::link::box_line::{Nothing, Printer, Status};
use crate::link::client::{ASK_SECS, Asked, Link, LinkConfig, parse_config, read_private};
use crate::link::frames::{FromClient, PairReport, Plane, REPORT_BOUND, SetupAsk, Stage, ToClient};
use crate::link::key_config::{self, AdoptRefused, AnswerFile, RequestRefused};
use crate::link::pair_client::{self, Claim, DialError, Heard, LastWord};
use crate::link::pair_code::grouped;

/// The members a connector's config carries that the setup connection
/// reads, for an error that names one.
const LINK_MEMBERS: &[&str] = &[
    "server",
    "server_name",
    "agent",
    "agent_id",
    "plane",
    "server_certificate",
    "certificate",
    "key",
    "local_address",
];

/// **`request`'s arguments**: the config to create, which must not exist.
#[derive(Debug, clap::Args)]
pub struct RequestArgs {
    /// The connector's config to create, holding its new key alone.
    #[arg(long)]
    pub config: PathBuf,
}

/// **`adopt`'s arguments**: the config `request` created and the answer
/// `pair` wrote.
#[derive(Debug, clap::Args)]
pub struct AdoptArgs {
    /// The connector's config from `request`, completed in place.
    #[arg(long)]
    pub config: PathBuf,
    /// The answer file `pair` wrote.
    #[arg(long)]
    pub answer: PathBuf,
}

/// **`pair`'s arguments** (Spec 8.2).
#[derive(Debug, clap::Args)]
pub struct PairArgs {
    /// The console's address and port, the link address.
    #[arg(long)]
    pub console: String,
    /// What `admin-con request` printed: its `request` line.
    #[arg(long)]
    pub admin_request: PathBuf,
    /// The survey's report, one JSON object within 64 KiB, which crosses
    /// with the pairing as its receipt, kept as the `pairing` stage's.
    #[arg(long)]
    pub report: PathBuf,
    /// The management card's address to dial from (Spec 8), an IP literal;
    /// absent, the dial goes by the machine's routes.
    #[arg(long)]
    pub local_address: Option<std::net::IpAddr>,
    /// The file the answer is written to, which must not exist.
    #[arg(long)]
    pub answer: PathBuf,
    /// The box's name, in the register's grammar. An appliance always
    /// passes it. The default, the machine's own name, serves a host.
    #[arg(long = "box")]
    pub r#box: Option<String>,
}

/// **`setup`'s two asks** (Spec 8.3).
#[derive(Debug, clap::Subcommand)]
pub enum SetupCommand {
    /// The agent, its models, the models drive's need, the laying once
    /// Confirm stands, and the SSH keys.
    Describe {
        /// admin-con's config.
        #[arg(long)]
        config: PathBuf,
    },
    /// The survey's report on standard input, kept on the setup as its
    /// stage's latest.
    Report {
        /// admin-con's config.
        #[arg(long)]
        config: PathBuf,
        #[arg(long, value_enum)]
        stage: StageArg,
    },
}

/// A report's stage as the command takes it: `pairing` alone, the one
/// stage of `weaver-survey/2`, so version 1's `drives` is a usage error.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum StageArg {
    Pairing,
}

impl From<StageArg> for Stage {
    fn from(stage: StageArg) -> Stage {
        match stage {
            StageArg::Pairing => Stage::Pairing,
        }
    }
}

/// The process's own printer for `command`.
pub fn printer(command: &'static str) -> Printer<std::io::Stdout, std::io::Stderr> {
    Printer::new(command, std::io::stdout(), std::io::stderr())
}

/// **The binary's arguments, or a `usage` line**: an argument a box command
/// refuses is answered as every other outcome is, a line and status 2,
/// clap's words on standard error through the printer. Help, and the
/// service's own arguments, are clap's as built.
pub fn parse_args<A: clap::Parser>(binary: &'static str, commands: &[&str]) -> A {
    match A::try_parse() {
        Ok(args) => args,
        Err(e) => {
            use clap::error::ErrorKind;
            let boxed = std::env::args_os()
                .nth(1)
                .and_then(|a| a.to_str().map(|a| commands.contains(&a)))
                .unwrap_or(false);
            if !boxed
                || matches!(
                    e.kind(),
                    ErrorKind::DisplayHelp
                        | ErrorKind::DisplayVersion
                        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                )
            {
                e.exit();
            }
            let mut p = printer(binary);
            for line in e.render().to_string().lines().filter(|l| !l.is_empty()) {
                p.reason(line);
            }
            let status = p.outcome(Status::Usage, Status::Usage.name(), &Nothing {});
            std::process::exit(status.code())
        }
    }
}

#[derive(Serialize)]
struct RequestLine<'a> {
    request: &'a str,
}

#[derive(Serialize)]
struct ReasonLine<'a> {
    reason: &'a str,
}

#[derive(Serialize)]
struct AddressLine<'a> {
    address: &'a str,
}

#[derive(Serialize)]
struct Adopted<'a> {
    agent: &'a str,
    agent_id: &'a str,
    plane: &'a str,
}

#[derive(Serialize)]
struct Waiting<'a> {
    code: &'a str,
    fingerprint: &'a str,
    expires_in_secs: u64,
}

#[derive(Serialize)]
struct Paired<'a> {
    agent: &'a str,
    agent_id: &'a str,
    #[serde(rename = "box")]
    r#box: &'a str,
}

/// `unknown`'s fingerprint: the answer's one certificate, admin-con's
/// (Spec 8.2).
#[derive(Serialize)]
struct Fingerprints {
    admin: String,
}

/// **`request`** (Spec 8.2): the config created holding the new key alone,
/// and its `request` line. No dial.
pub fn request<O: Write, E: Write>(p: &mut Printer<O, E>, plane: Plane, config: &Path) -> Status {
    match key_config::request(plane, config) {
        Ok(request) => {
            let status = p.outcome(Status::Done, "request", &RequestLine { request: &request });
            // **The way on from an undelivered line**: the config stands
            // with its key and no certificate, which no console has seen.
            if status != Status::Done {
                p.reason(&format!(
                    "{} was made with a new key and no certificate, and its request line was not delivered: remove it and run request again",
                    config.display()
                ));
            }
            status
        }
        Err(RequestRefused::Exists(why)) => p.ended(Status::Usage, &why),
        Err(RequestRefused::Fault(why)) => p.ended(Status::Fault, &why),
    }
}

/// **`adopt`** (Spec 8.2): `adopted`, or `refused` with its reason and
/// nothing written.
pub fn adopt<O: Write, E: Write>(
    p: &mut Printer<O, E>,
    plane: Plane,
    config: &Path,
    answer: &Path,
) -> Status {
    match key_config::adopt(plane, config, answer) {
        Ok(answer) => p.outcome(
            Status::Done,
            "adopted",
            &Adopted {
                agent: &answer.agent,
                agent_id: &answer.agent_id,
                plane: plane.as_str(),
            },
        ),
        Err(AdoptRefused::Usage(why)) => p.ended(Status::Usage, &why),
        Err(AdoptRefused::Refused(reason)) => {
            p.outcome(Status::Refused, "refused", &ReasonLine { reason })
        }
        Err(AdoptRefused::Fault(why)) => p.ended(Status::Fault, &why),
    }
}

/// The longest request line `pair` reads: a connector's `request` line is
/// far inside it.
const REQUEST_LINE_BOUND: usize = 16 * 1024;

/// **A report file `pair` carries** (Spec 8.2): read never through a link,
/// a regular file, UTF-8, one JSON object within `REPORT_BOUND`, else the
/// `usage` it is, before any dial.
fn read_report_file(path: &Path) -> Result<String, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| format!("opening {}: {e}", path.display()))?;
    let regular = file.metadata().map(|m| m.file_type().is_file());
    if !matches!(regular, Ok(true)) {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(REPORT_BOUND as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("reading {}: {e}", path.display()))?;
    let Ok(report) = String::from_utf8(bytes) else {
        return Err(format!("{} is not UTF-8", path.display()));
    };
    if !crate::link::frames::one_report(&report) {
        return Err(format!(
            "{} is not one JSON object within {REPORT_BOUND} bytes",
            path.display()
        ));
    }
    Ok(report)
}

/// A request file as `pair` reads it: what `request` printed, its one
/// `request` line, never through a link and within its bound.
fn read_request(path: &Path) -> Result<String, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| format!("opening {}: {e}", path.display()))?;
    let regular = file.metadata().map(|m| m.file_type().is_file());
    if !matches!(regular, Ok(true)) {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut text = String::new();
    file.take(REQUEST_LINE_BOUND as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("reading {}: {e}", path.display()))?;
    #[derive(serde::Deserialize)]
    struct Line {
        event: String,
        request: String,
    }
    match serde_json::from_str::<Line>(text.trim_end()) {
        Ok(line)
            if line.event == "request"
                && line
                    .request
                    .starts_with("-----BEGIN CERTIFICATE REQUEST-----")
                && text.len() <= REQUEST_LINE_BOUND =>
        {
            Ok(line.request)
        }
        _ => Err(format!(
            "{} is not the request line a connector's request printed",
            path.display()
        )),
    }
}

/// The machine's own name, the default box.
fn own_name() -> Option<String> {
    let mut name = [0u8; 256];
    // SAFETY: the buffer is valid for its length, and gethostname writes
    // at most that many bytes.
    if unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len()) } != 0 {
        return None;
    }
    let end = name.iter().position(|b| *b == 0)?;
    String::from_utf8(name[..end].to_vec()).ok()
}

/// A certificate's fingerprint as the pairing page shows it.
fn grouped_fingerprint(pem: &str) -> String {
    let der = rustls_pemfile::certs(&mut pem.as_bytes())
        .next()
        .and_then(|c| c.ok());
    match der {
        Some(der) => grouped(&crate::link::authority::fingerprint(der.as_ref())),
        None => String::new(),
    }
}

/// **A dial that ended before the box waits**, as its outcome.
pub fn dial_failed<O: Write, E: Write>(
    p: &mut Printer<O, E>,
    address: &str,
    error: &DialError,
) -> Status {
    match error {
        DialError::Unreachable(why) => {
            p.reason(why);
            p.outcome(Status::Unreachable, "unreachable", &AddressLine { address })
        }
        DialError::Refused(reason) => p.outcome(Status::Refused, "refused", &ReasonLine { reason }),
        DialError::Expired => p.outcome(Status::Expired, "expired", &Nothing {}),
        DialError::Fault(why) => p.ended(Status::Fault, why),
    }
}

/// **What a waiting box heard, other than its answer**, as its outcome.
/// Nothing is written before the box's word, so an end here is never
/// unknown.
pub fn heard<O: Write, E: Write>(p: &mut Printer<O, E>, heard: &Heard) -> Status {
    match heard {
        Heard::Expired => p.outcome(Status::Expired, "expired", &Nothing {}),
        Heard::Refused(reason) => p.outcome(Status::Refused, "refused", &ReasonLine { reason }),
        Heard::Closed => p.ended(
            Status::Fault,
            "the console closed the connection with no word",
        ),
        Heard::Silent => p.ended(
            Status::Fault,
            "the console went silent for four heartbeats' cadence",
        ),
        Heard::Fault(why) => p.ended(Status::Fault, why),
        Heard::Answer(_) => p.ended(Status::Fault, "an answer out of turn"),
    }
}

/// **`pair`** (Spec 8.2): its inputs read before any dial, one `waiting`
/// line with the code and the authority's fingerprint, then the answer
/// written to its file, the box's word, and the console's last word.
pub async fn pair<O: Write, E: Write>(p: &mut Printer<O, E>, args: &PairArgs) -> Status {
    let admin_request = match read_request(&args.admin_request) {
        Ok(request) => request,
        Err(why) => return p.ended(Status::Usage, &why),
    };
    let reports = match read_report_file(&args.report) {
        Ok(report) => vec![PairReport {
            stage: Stage::Pairing,
            report,
        }],
        Err(why) => return p.ended(Status::Usage, &why),
    };
    if std::fs::symlink_metadata(&args.answer).is_ok() {
        return p.ended(
            Status::Usage,
            &format!(
                "{} exists: the answer is written to a new file",
                args.answer.display()
            ),
        );
    }
    let r#box = match args.r#box.clone().or_else(own_name) {
        Some(name) if crate::link::verbs::is_box(&name) => name,
        Some(name) => {
            return p.ended(
                Status::Usage,
                &format!("{name} is not a box's name in the register's grammar: pass --box"),
            );
        }
        None => {
            return p.ended(
                Status::Usage,
                "the machine's own name could not be read: pass --box",
            );
        }
    };
    let claim = Claim {
        r#box,
        admin_request,
        reports,
    };
    let mut dialed = match pair_client::dial_from(&args.console, args.local_address, &claim).await {
        Ok(dialed) => dialed,
        Err(e) => return dial_failed(p, &args.console, &e),
    };
    // **A code nobody can read ends the pairing at once**: the box closes
    // and the console lets it go, rather than waiting out its life.
    if let Err(why) = p.line(
        "waiting",
        &Waiting {
            code: &dialed.code().to_string(),
            fingerprint: &dialed.fingerprint(),
            expires_in_secs: dialed.expires_in().as_secs(),
        },
    ) {
        return p.ended(
            Status::Fault,
            &format!(
                "standard output did not take the waiting line, so its code cannot be read: {why}"
            ),
        );
    }
    let answer = match dialed.next().await {
        Heard::Answer(answer) => answer,
        other => return heard(p, &other),
    };
    let fingerprints = Fingerprints {
        admin: grouped_fingerprint(&answer.admin),
    };
    let file = AnswerFile {
        server: args.console.clone(),
        server_name: answer.server_name.clone(),
        agent: answer.agent.clone(),
        agent_id: answer.agent_id.clone(),
        r#box: answer.r#box.clone(),
        authority: answer.authority.clone(),
        gate: None,
        admin: answer.admin.clone(),
    };
    // **No word before the answer is written**: a box that cannot write it
    // sends none, and the console writes nothing.
    if let Err(e) = file.write_new(&args.answer) {
        return p.ended(Status::Fault, &format!("{e:#}"));
    }
    if dialed.written().await.is_err() {
        p.reason("the box's word may not have reached the console: compare the fingerprint with the pairing page's");
        return p.outcome(Status::Unknown, "unknown", &fingerprints);
    }
    match dialed.last_word().await {
        LastWord::Paired => p.outcome(
            Status::Done,
            "paired",
            &Paired {
                agent: &answer.agent,
                agent_id: &answer.agent_id,
                r#box: &answer.r#box,
            },
        ),
        LastWord::Refused(reason) => {
            p.outcome(Status::Refused, "refused", &ReasonLine { reason: &reason })
        }
        LastWord::Unknown => {
            p.reason("the connection ended after the answer was written and before the console's last word: compare the fingerprint with the pairing page's");
            p.outcome(Status::Unknown, "unknown", &fingerprints)
        }
    }
}

/// admin-con's link from its config, or the `usage` it is.
fn setup_link(config: &Path) -> Result<Link, String> {
    let content = read_private(config).map_err(|e| format!("{e:#}"))?;
    let link: LinkConfig =
        parse_config(config, &content, LINK_MEMBERS).map_err(|e| format!("{e:#}"))?;
    link.expect_plane(Plane::Admin)
        .map_err(|e| format!("{e:#}"))?;
    Link::new(link).map_err(|e| format!("{}: {e:#}", config.display()))
}

/// The server's answer to a setup ask, as the outcome.
fn answered<O: Write, E: Write>(
    p: &mut Printer<O, E>,
    address: &str,
    asked: Asked,
    may_have_acted: bool,
) -> Status {
    match asked {
        Asked::Answer(ToClient::Described { described }) => {
            p.outcome(Status::Done, "described", &described)
        }
        Asked::Answer(ToClient::Accepted) => p.outcome(Status::Done, "accepted", &Nothing {}),
        Asked::Answer(ToClient::Refusal { reason }) => p.outcome(
            Status::Refused,
            "refused",
            &ReasonLine {
                reason: reason.as_str(),
            },
        ),
        Asked::Answer(other) => p.ended(
            Status::Fault,
            &format!(
                "the console answered with a {} frame",
                crate::link::client::to_client_name(&other)
            ),
        ),
        Asked::Unreachable(why) => {
            p.reason(&why);
            p.outcome(Status::Unreachable, "unreachable", &AddressLine { address })
        }
        Asked::Ended(why) if may_have_acted => {
            p.reason(&why);
            p.reason("the console may have kept the report: ask again");
            p.outcome(Status::Unknown, "unknown", &Nothing {})
        }
        Asked::Fault(why) => p.ended(Status::Fault, &why),
        Asked::Ended(why) => p.ended(Status::Fault, &why),
    }
}

/// **`setup describe`** (Spec 8.3).
pub async fn setup_describe<O: Write, E: Write>(p: &mut Printer<O, E>, config: &Path) -> Status {
    setup_describe_within(p, config, Duration::from_secs(ASK_SECS)).await
}

/// `setup describe` with its ask bounded at `bound`.
pub(crate) async fn setup_describe_within<O: Write, E: Write>(
    p: &mut Printer<O, E>,
    config: &Path,
    bound: Duration,
) -> Status {
    let link = match setup_link(config) {
        Ok(link) => link,
        Err(why) => return p.ended(Status::Usage, &why),
    };
    let ask = FromClient::Setup {
        agent: link.config().agent.clone(),
        plane: Plane::Admin,
        ask: SetupAsk::Describe,
    };
    let asked = link.ask_within(ask, bound).await;
    answered(p, &link.config().server, asked, false)
}

/// **`setup report`** (Spec 8.3): its input one JSON object within
/// `REPORT_BOUND`, checked and parsed no further, carried as the text read.
/// A larger one is refused `too_large` before the dial.
pub async fn setup_report<O: Write, E: Write, R: Read>(
    p: &mut Printer<O, E>,
    config: &Path,
    stage: Stage,
    input: R,
) -> Status {
    setup_report_within(p, config, stage, input, Duration::from_secs(ASK_SECS)).await
}

/// `setup report` with its ask bounded at `bound`.
pub(crate) async fn setup_report_within<O: Write, E: Write, R: Read>(
    p: &mut Printer<O, E>,
    config: &Path,
    stage: Stage,
    input: R,
    bound: Duration,
) -> Status {
    let link = match setup_link(config) {
        Ok(link) => link,
        Err(why) => return p.ended(Status::Usage, &why),
    };
    let mut bytes = Vec::new();
    if let Err(e) = input.take(REPORT_BOUND as u64 + 1).read_to_end(&mut bytes) {
        return p.ended(Status::Fault, &format!("reading the report: {e}"));
    }
    if bytes.len() > REPORT_BOUND {
        p.reason(&format!("the report passes {REPORT_BOUND} bytes"));
        return p.outcome(
            Status::Refused,
            "refused",
            &ReasonLine {
                reason: "too_large",
            },
        );
    }
    let Ok(report) = String::from_utf8(bytes) else {
        return p.ended(Status::Usage, "the report is not UTF-8");
    };
    let object = report.trim_start().starts_with('{')
        && serde_json::from_str::<serde::de::IgnoredAny>(&report).is_ok();
    if !object {
        return p.ended(Status::Usage, "the report is not one JSON object");
    }
    let ask = FromClient::Setup {
        agent: link.config().agent.clone(),
        plane: Plane::Admin,
        ask: SetupAsk::Report { stage, report },
    };
    let asked = link.ask_within(ask, bound).await;
    answered(p, &link.config().server, asked, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::authority::Authority;
    use crate::link::pair_client::Answer;
    use std::os::unix::fs::PermissionsExt;

    /// **`request` whose line is not delivered says where its config stands**:
    /// `fault`, the config made with its new key, and standard error naming
    /// it and the way on, so a re-run's `Exists` refusal is no dead end.
    ///
    /// Perturbation: ignore the line's failure, and `request` exits `done`
    /// with no request line.
    #[test]
    fn a_request_whose_line_is_not_delivered_names_the_config_it_made() {
        let dir = crate::test_dir::tempdir().unwrap();
        let config = dir.path().join("admin-con.toml");
        let out = crate::link::box_line::tests::Refusing { at_flush: false };
        let mut p = Printer::new("admin-con request", out, Vec::new());
        assert_eq!(request(&mut p, Plane::Admin, &config), Status::Fault);
        assert!(config.is_file(), "the config was made");
        let (_, err) = p.into_inner();
        let err = String::from_utf8(err).unwrap();
        let named = format!(
            "admin-con request: {} was made with a new key and no certificate",
            config.display()
        );
        assert!(err.contains(&named), "{err}");
        assert!(err.contains("remove it and run request again"), "{err}");
    }

    /// The setup asks' bound in these tests, the commands' own being 30 s.
    const BOUND: Duration = Duration::from_millis(1500);

    /// admin-con's config naming `server`, its credential minted by
    /// `authority`.
    fn admin_config(dir: &Path, authority: &Authority, server: &str) -> PathBuf {
        let credential = authority.mint_client("karl", Plane::Admin).unwrap();
        let mut table = toml::Table::new();
        for (member, value) in [
            ("server", server.to_owned()),
            ("server_name", "weaver-web".to_owned()),
            ("agent", "karl".to_owned()),
            ("agent_id", "ag-0000000000000001".to_owned()),
            ("plane", "admin".to_owned()),
            ("server_certificate", authority.certificate_pem().to_owned()),
            ("certificate", credential.certificate_pem),
            ("key", credential.key_pem),
        ] {
            table.insert(member.into(), value.into());
        }
        let path = dir.join("admin-con.toml");
        std::fs::write(&path, toml::to_string(&table).unwrap()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path
    }

    fn line_of(p: Printer<Vec<u8>, Vec<u8>>) -> serde_json::Value {
        let (out, _) = p.into_inner();
        serde_json::from_slice(out.strip_suffix(b"\n").unwrap()).unwrap()
    }

    /// `setup describe` and `setup report` against `config`, each bounded
    /// at `BOUND`: their statuses and lines.
    async fn both_asks(config: &Path) -> [(Status, serde_json::Value); 2] {
        let mut p = Printer::new("admin-con setup describe", Vec::new(), Vec::new());
        let described = setup_describe_within(&mut p, config, BOUND).await;
        let described = (described, line_of(p));
        let mut p = Printer::new("admin-con setup report", Vec::new(), Vec::new());
        let reported = setup_report_within(&mut p, config, Stage::Pairing, &b"{}"[..], BOUND).await;
        [described, (reported, line_of(p))]
    }

    /// **A setup ask whose dial or handshake stalls was never sent**, so
    /// both commands answer `unreachable` with the address dialed: against
    /// a listener that accepts and never handshakes, and one that never
    /// accepts, its queue full, so the connection itself stalls.
    ///
    /// Perturbation: take every stall as after the send, and `describe`
    /// answers `fault` and `report` `unknown`.
    #[tokio::test]
    async fn a_setup_ask_whose_dial_or_handshake_stalls_is_unreachable() {
        let dir = crate::test_dir::tempdir().unwrap();
        let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();

        let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = silent.local_addr().unwrap().to_string();
        let held = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((tcp, _)) = silent.accept().await {
                held.push(tcp);
            }
        });
        let config = admin_config(dir.path(), &authority, &address);
        for (status, line) in both_asks(&config).await {
            assert_eq!(status, Status::Unreachable, "{line}");
            assert_eq!(line["address"], address.as_str(), "{line}");
        }
        held.abort();

        let socket = tokio::net::TcpSocket::new_v4().unwrap();
        socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let full = socket.listen(1).unwrap();
        let at = full.local_addr().unwrap();
        let mut fillers = Vec::new();
        let stalled = loop {
            match std::net::TcpStream::connect_timeout(&at, Duration::from_millis(300)) {
                Ok(tcp) => fillers.push(tcp),
                Err(_) => break true,
            }
            if fillers.len() > 64 {
                break false;
            }
        };
        assert!(stalled, "the queue never filled, so no connection stalls");
        let address = at.to_string();
        std::fs::remove_file(&config).unwrap();
        let config = admin_config(dir.path(), &authority, &address);
        for (status, line) in both_asks(&config).await {
            assert_eq!(status, Status::Unreachable, "{line}");
            assert_eq!(line["address"], address.as_str(), "{line}");
        }
        drop(full);
    }

    /// **A setup ask sent and never answered may have reached the console**:
    /// against a server that completes the handshake, reads the ask and
    /// answers nothing, `setup report` answers `unknown` (6), since the
    /// report may be kept, and `setup describe` `fault` (1), since a
    /// describe changes nothing.
    ///
    /// Perturbation: take every stall as before the send, and both answer
    /// `unreachable`.
    #[tokio::test]
    async fn a_setup_ask_sent_and_unanswered_is_unknown_for_a_report_and_a_fault_for_a_describe() {
        let dir = crate::test_dir::tempdir().unwrap();
        let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(authority.server_tls().unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let mute = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((tcp, _)) = listener.accept().await {
                let Ok(stream) = acceptor.accept(tcp).await else {
                    continue;
                };
                let mut reader = crate::link::frames::LineReader::new(stream);
                let asked = reader.next().await;
                assert!(
                    matches!(asked, crate::link::frames::Line::Frame(_)),
                    "the ask was sent"
                );
                held.push(reader);
            }
        });
        let config = admin_config(dir.path(), &authority, &address);
        let [(described, line), (reported, report_line)] = both_asks(&config).await;
        assert_eq!(
            (described, line["event"].as_str()),
            (Status::Fault, Some("fault"))
        );
        assert_eq!(
            (reported, report_line["event"].as_str()),
            (Status::Unknown, Some("unknown"))
        );
        mute.abort();
    }

    const ESC: &str = "\u{1b}[2J\u{1b}]0;title\u{7}";

    fn printable(bytes: &[u8]) {
        for b in bytes {
            assert!(
                (0x20..0x7f).contains(b) || *b == b'\n',
                "byte {b:#04x} reached the terminal: {}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    /// **ESC reaches the terminal through no fault or reason** (Spec 8.2):
    /// each `DialError` and `Heard` variant, its words carrying an escape
    /// sequence, prints printable ASCII alone and exits with its status.
    ///
    /// Perturbation: write a reason to standard error unescaped, and the
    /// escape byte reaches the terminal.
    #[test]
    fn esc_in_every_dial_error_and_heard_variant_prints_escaped() {
        let address = format!("console{ESC}:7443");
        let dials = [
            (
                DialError::Unreachable(format!("dialing{ESC}")),
                Status::Unreachable,
            ),
            (DialError::Refused(format!("busy{ESC}")), Status::Refused),
            (DialError::Expired, Status::Expired),
            (DialError::Fault(format!("fault{ESC}")), Status::Fault),
        ];
        for (error, status) in dials {
            let mut p = Printer::new("admin-con pair", Vec::new(), Vec::new());
            assert_eq!(dial_failed(&mut p, &address, &error), status, "{error:?}");
            let (out, err) = p.into_inner();
            printable(&out);
            printable(&err);
            let line: serde_json::Value =
                serde_json::from_slice(out.strip_suffix(b"\n").unwrap()).unwrap();
            assert_eq!(line["event"], status.name(), "{error:?}");
            if let DialError::Unreachable(_) = error {
                assert_eq!(line["address"], address.as_str());
            }
        }
        let answer = Answer {
            agent: format!("karl{ESC}"),
            agent_id: format!("ag-{ESC}"),
            r#box: format!("box{ESC}"),
            server_name: format!("weaver{ESC}"),
            authority: format!("pem{ESC}"),
            admin: format!("pem{ESC}"),
        };
        let heard_variants = [
            (Heard::Expired, Status::Expired),
            (
                Heard::Refused(format!("box_and_name_live{ESC}")),
                Status::Refused,
            ),
            (Heard::Closed, Status::Fault),
            (Heard::Silent, Status::Fault),
            (Heard::Fault(format!("a frame{ESC}")), Status::Fault),
            (Heard::Answer(answer), Status::Fault),
        ];
        for (h, status) in heard_variants {
            let mut p = Printer::new("admin-con pair", Vec::new(), Vec::new());
            assert_eq!(heard(&mut p, &h), status, "{h:?}");
            let (out, err) = p.into_inner();
            printable(&out);
            printable(&err);
            if let Heard::Refused(reason) = &h {
                let line: serde_json::Value =
                    serde_json::from_slice(out.strip_suffix(b"\n").unwrap()).unwrap();
                assert_eq!(line["reason"], reason.as_str(), "the line reads back");
            }
            if let Heard::Fault(_) = &h {
                let err = String::from_utf8(err).unwrap();
                assert!(err.contains("a frame\\x1b[2J"), "{err}");
            }
        }
    }
}
