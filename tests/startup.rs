//! Startup acceptance for W2, "the conversation half goes, and the server
//! starts on the current schema", and assessment-2026-09-19-project-state F1.
//! Spawns the real binary on a fresh store, checks the Record session gate,
//! seeds a claim, and checks restart and bounded connection failure.
//! No Spec assertion covers startup or the composition root.

use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Own every child and scratch file even when an assertion unwinds.
struct Server {
    child: Child,
    directory: PathBuf,
    address: SocketAddr,
    /// The certificate a TLS server answers with, which requests trust.
    ca: Option<PathBuf>,
}

impl Server {
    fn spawn(database: &str) -> Self {
        Self::spawn_with(database, database)
    }

    /// A server on `database`, its authority minted by `authority init`
    /// against `init_with`: the one database the two differ on is the
    /// unreachable one, where the server's own failure is what is checked.
    fn spawn_with(database: &str, init_with: &str) -> Self {
        Self::spawn_full(database, init_with, None)
    }

    /// A server on `database` whose browser listener serves TLS with the
    /// certificate and key given, under the origin `https://localhost:<port>`.
    fn spawn_tls(database: &str, certificate: &std::path::Path, key: &std::path::Path) -> Self {
        Self::spawn_full(database, database, Some((certificate, key)))
    }

    fn spawn_full(
        database: &str,
        init_with: &str,
        tls: Option<(&std::path::Path, &std::path::Path)>,
    ) -> Self {
        let http = TcpListener::bind("127.0.0.1:0").unwrap();
        let link = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = http.local_addr().unwrap();
        let directory =
            std::env::temp_dir().join(format!("weaver-startup-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let config = directory.join("config.toml");
        let mut values = toml::Table::new();
        values.insert("listen".into(), address.to_string().into());
        values.insert(
            "link_listen".into(),
            link.local_addr().unwrap().to_string().into(),
        );
        values.insert("database".into(), init_with.into());
        if let Some((certificate, key)) = tls {
            values.insert(
                "tls_certificate".into(),
                certificate.display().to_string().into(),
            );
            values.insert("tls_key".into(), key.display().to_string().into());
            values.insert(
                "origin".into(),
                format!("https://localhost:{}", address.port()).into(),
            );
            values.insert("rp_id".into(), "localhost".into());
        }
        // The server refuses to start without an authority (Spec 8), so
        // the acceptance mints one first, under the scratch directory. Since
        // act 11's audit, `authority init` needs the store and writes its
        // audit record there first, so it is the one that applies the schema
        // to the fresh store, and the server then starts on what it applied.
        let authority = directory.join("authority");
        values.insert(
            "authority_dir".into(),
            authority.display().to_string().into(),
        );
        fs::write(&config, toml::to_string(&values).unwrap()).unwrap();
        let init = Command::new(env!("CARGO_BIN_EXE_weaver-web"))
            .arg("--config")
            .arg(&config)
            .args(["authority", "init"])
            .output()
            .unwrap();
        assert!(
            init.status.success(),
            "authority init failed: {}",
            String::from_utf8_lossy(&init.stdout)
        );
        values.insert("database".into(), database.into());
        fs::write(&config, toml::to_string(&values).unwrap()).unwrap();
        let stdout = File::create(directory.join("stdout")).unwrap();
        let stderr = File::create(directory.join("stderr")).unwrap();
        drop((http, link));
        let child = Command::new(env!("CARGO_BIN_EXE_weaver-web"))
            .arg("--config")
            .arg(config)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .unwrap();
        Self {
            child,
            directory,
            address,
            ca: tls.map(|(certificate, _)| certificate.to_owned()),
        }
    }

    fn logs(&self) -> String {
        format!(
            "stdout:\n{}\nstderr:\n{}",
            fs::read_to_string(self.directory.join("stdout")).unwrap(),
            fs::read_to_string(self.directory.join("stderr")).unwrap(),
        )
    }

    fn wait_for_listen(&mut self) {
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("server exited before listening: {status}\n{}", self.logs());
            }
            if TcpStream::connect_timeout(&self.address, Duration::from_millis(100)).is_ok() {
                assert!(self.child.try_wait().unwrap().is_none(), "{}", self.logs());
                return;
            }
            assert!(
                Instant::now() < until,
                "server did not listen within 15s\n{}",
                self.logs()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// curl bounds each request and decodes HTTP transfer framing for the test.
    fn get(&self, path: &str, token: Option<&str>) -> (u16, String) {
        self.request("GET", path, token)
    }

    fn request(&self, method: &str, path: &str, token: Option<&str>) -> (u16, String) {
        let mut curl = Command::new("curl");
        curl.arg("--request").arg(method);
        curl.args([
            "--silent",
            "--show-error",
            "--max-time",
            "5",
            "--noproxy",
            "*",
            "--write-out",
            "\n%{http_code}",
        ]);
        if let Some(token) = token {
            curl.arg("--cookie")
                .arg(format!("__Host-weaver_session={token}"));
        }
        let url = match &self.ca {
            Some(ca) => {
                let port = self.address.port();
                curl.arg("--cacert").arg(ca);
                curl.arg("--resolve")
                    .arg(format!("localhost:{port}:127.0.0.1"));
                format!("https://localhost:{port}{path}")
            }
            None => format!("http://{}{path}", self.address),
        };
        let output = curl.arg(url).output().unwrap();
        assert!(
            output.status.success(),
            "curl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        let (body, status) = text.rsplit_once('\n').unwrap();
        (status.parse().unwrap(), body.into())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[tokio::test]
#[ignore = "needs DATABASE_URL naming a disposable PostgreSQL; see the W2 goal"]
async fn real_server_starts_on_current_schema() {
    let database =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must name a fresh disposable database");
    let pool = sqlx::PgPool::connect(&database).await.unwrap();
    let migrated: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !migrated,
        "startup acceptance requires a never-migrated database; recreate it before running"
    );

    let mut server = Server::spawn(&database);
    server.wait_for_listen();
    assert_eq!(
        server.get("/record", None),
        (
            401,
            "this surface is read under a session. Sign in and ask again.".into()
        )
    );

    // **A person's session, opened in the store** (act 11, PR 4a): the
    // person, the passkey it was opened with, and the session, as sign-in
    // will open them.
    let token = uuid::Uuid::new_v4().to_string();
    let person: String = sqlx::query_scalar(
        "INSERT INTO person (name, name_key) VALUES ('startup-check', 'startup-check') RETURNING person_id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let passkey: String = sqlx::query_scalar(
        "INSERT INTO passkey (credential_id, person_id, credential) VALUES ('startup-cred', $1, '{}') RETURNING passkey_id",
    )
    .bind(&person)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO session (bearer_digest, person_id, passkey_id) VALUES ($1, $2, $3)")
        .bind(format!("{:x}", Sha256::digest(token.as_bytes())))
        .bind(&person)
        .bind(&passkey)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = server.get("/record", Some(&token));
    assert_eq!(status, 200);
    assert!(
        body.contains("startup-check"),
        "record must render the session's person: {body}"
    );
    let (status, body) = server.get("/admin", None);
    eprintln!("W2 /admin measurement: status={status}, body={body:?}");
    // The subsequent W2 ruling holds every retained handler behind 503.
    for (method, path) in [
        ("GET", "/admin/lifecycle"),
        ("POST", "/admin/lifecycle/startup-check/start"),
        ("GET", "/admin/agents/startup-check/config"),
        ("GET", "/admin/trace/startup-check"),
        ("GET", "/admin/trace/startup-check/stream"),
    ] {
        // A request that changes state meets the `Origin` check first (act
        // 11, PR 4a), and this server has no origin configured.
        let refused = if method == "POST" {
            (
                403,
                "no origin is configured, so this server serves no request that changes state"
                    .into(),
            )
        } else {
            (
                503,
                "the legacy admin surface is unavailable until the session/IAM act.".into(),
            )
        };
        for cookie in [None, Some(token.as_str())] {
            assert_eq!(
                server.request(method, path, cookie),
                refused,
                "legacy route {method} {path} must refuse before any handler work"
            );
        }
    }

    drop(server);

    let mut restarted = Server::spawn(&database);
    restarted.wait_for_listen();
    assert_eq!(restarted.get("/record", Some(&token)).0, 200);
    drop(restarted);

    // **A TLS start** (act 11, PR 3): a certificate minted here for
    // localhost, never written outside this run's temporary directory; the
    // browser listener answers over TLS and not in the clear.
    let tls_dir = tempfile::tempdir().unwrap();
    let key = rcgen::KeyPair::generate().unwrap();
    let certificate = rcgen::CertificateParams::new(vec!["localhost".to_string()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let (certificate_path, key_path) = (tls_dir.path().join("a.crt"), tls_dir.path().join("a.key"));
    fs::write(&certificate_path, certificate.pem()).unwrap();
    fs::write(&key_path, key.serialize_pem()).unwrap();
    let mut tls = Server::spawn_tls(&database, &certificate_path, &key_path);
    tls.wait_for_listen();
    assert_eq!(
        tls.get("/record", Some(&token)).0,
        200,
        "the record answers over TLS\n{}",
        tls.logs()
    );
    let plain = Command::new("curl")
        .args(["--silent", "--max-time", "5", "--noproxy", "*"])
        .arg(format!("http://{}/record", tls.address))
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&plain.stdout).contains("startup-check"),
        "a plain request reached a surface of the TLS listener"
    );
    drop(tls);

    let mut invalid = Server::spawn_with("postgres:///nope?host=/nonexistent", &database);
    let until = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(status) = invalid.child.try_wait().unwrap() {
            assert!(
                !status.success(),
                "an unreachable database must fail startup"
            );
            eprintln!("W2 invalid database: {status}\n{}", invalid.logs());
            break;
        }
        assert!(
            Instant::now() < until,
            "invalid database startup hung\n{}",
            invalid.logs()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    pool.close().await;
}
