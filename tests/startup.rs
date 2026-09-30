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
}

impl Server {
    fn spawn(database: &str) -> Self {
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
            curl.arg("--cookie").arg(format!("weaver_session={token}"));
        }
        let output = curl
            .arg(format!("http://{}{path}", self.address))
            .output()
            .unwrap();
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
            "this surface is read under a session. Open one and ask again.".into()
        )
    );

    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO session (bearer_digest, claimed_name, role) VALUES ($1, 'startup-check', 'user')")
        .bind(format!("{:x}", Sha256::digest(token.as_bytes())))
        .execute(&pool).await.unwrap();
    let (status, body) = server.get("/record", Some(&token));
    assert_eq!(status, 200);
    assert!(
        body.contains("startup-check"),
        "record must render the seeded claim: {body}"
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
        for cookie in [None, Some(token.as_str())] {
            assert_eq!(
                server.request(method, path, cookie),
                (
                    503,
                    "the legacy admin surface is unavailable until the session/IAM act.".into()
                ),
                "legacy route {method} {path} must refuse before any handler work"
            );
        }
    }

    drop(server);

    let mut restarted = Server::spawn(&database);
    restarted.wait_for_listen();
    assert_eq!(restarted.get("/record", Some(&token)).0, 200);
    drop(restarted);

    let mut invalid = Server::spawn("postgres:///nope?host=/nonexistent");
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
