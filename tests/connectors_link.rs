//! conforms: web-the-webauthn-library-links-into-the-server-alone
//!
//! **The connectors carry no `libcrypto`** (design section 2): gate-con and
//! admin-con run on agent boxes, where a new shared library is a
//! provisioning change, so the WebAuthn library's OpenSSL is the server
//! binary's alone. Two instruments: `ldd` on the binaries this build made,
//! the server's showing the library is read at all; and the dependency graph
//! of the install's connector build (`--no-default-features`, CLAUDE.md),
//! which holds no `openssl-sys` at all.

use std::process::Command;

fn ldd(binary: &str) -> Option<String> {
    let output = Command::new("ldd").arg(binary).output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// **This build's connectors link no `libcrypto`, and its server does**:
/// the connectors are built with the `passkeys` feature on here, as `cargo
/// test` builds them, and the linker drops the library they never call.
#[test]
fn the_connectors_link_no_libcrypto_and_the_server_does() {
    let Some(server) = ldd(env!("CARGO_BIN_EXE_weaver-web")) else {
        eprintln!("skipped: no ldd on this host");
        return;
    };
    assert!(
        server.contains("libcrypto"),
        "the server binary links libcrypto, which shows ldd is read: {server}"
    );
    for connector in [
        env!("CARGO_BIN_EXE_gate-con"),
        env!("CARGO_BIN_EXE_admin-con"),
    ] {
        let linked = ldd(connector).unwrap();
        assert!(
            !linked.contains("libcrypto") && !linked.contains("libssl"),
            "{connector} links OpenSSL: {linked}"
        );
    }
}

/// **The install's connector build compiles no OpenSSL at all**: without
/// the default features, `openssl-sys` is in no normal dependency of the
/// package, so the connectors cannot link it whatever the linker does.
#[test]
fn the_connector_build_without_passkeys_holds_no_openssl() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--locked",
            "--offline",
            "-e",
            "normal",
            "--no-default-features",
            "--prefix",
            "none",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8_lossy(&output.stdout);
    assert!(tree.contains("weaver-web"), "the tree was read: {tree}");
    for crate_name in ["openssl-sys", "webauthn-rs"] {
        assert!(
            !tree
                .lines()
                .any(|line| line.starts_with(&format!("{crate_name} "))),
            "{crate_name} is in the connector build's graph"
        );
    }
}
