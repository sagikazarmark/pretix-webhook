use std::{
    io::{BufRead, BufReader},
    net::TcpListener,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[test]
fn simple_startup_reports_the_route_and_its_security_warnings() {
    let unrestricted_stderr = startup_output(
        command("127.0.0.1:0"),
        "warning: no HTTP Basic credentials configured for /webhook",
    );
    assert!(unrestricted_stderr.contains("listening on http://127.0.0.1:"));
    assert!(!unrestricted_stderr.contains("listening on http://127.0.0.1:0 "));
    assert!(unrestricted_stderr.contains("with 1 route(s)"));
    assert!(unrestricted_stderr.contains("pretix webhook route configured at /webhook"));
    assert!(unrestricted_stderr.contains(
        "warning: no filters configured for /webhook; accepting all events from all organizers"
    ));
}

#[test]
fn multi_startup_reports_each_route_and_only_its_applicable_warnings() {
    let config_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/startup-diagnostics-multi.toml"
    );
    let mut process = command("127.0.0.1:0");
    process.args(["--config", config_path]).env(
        "PRIVATE_CREDENTIAL_REFERENCE",
        "private-user:private-password",
    );

    let stderr = startup_output(
        process,
        "warning: no HTTP Basic credentials configured for /incoming/restricted-public",
    );

    assert!(stderr.contains("with 2 route(s)"), "{stderr:?}");
    assert!(
        stderr.contains("pretix webhook route configured at /incoming/unrestricted-authenticated"),
        "{stderr:?}"
    );
    assert!(
        stderr.contains("pretix webhook route configured at /incoming/restricted-public"),
        "{stderr:?}"
    );
    assert!(stderr.contains(
        "warning: no filters configured for /incoming/unrestricted-authenticated; accepting all events from all organizers"
    ));
    assert!(!stderr.contains(
        "warning: no HTTP Basic credentials configured for /incoming/unrestricted-authenticated"
    ));
    assert!(!stderr.contains("warning: no filters configured for /incoming/restricted-public"));
    assert!(!stderr.contains("private-organizer"));
    assert!(!stderr.contains("private-event"));
    assert!(!stderr.contains("PRIVATE_CREDENTIAL_REFERENCE"));
    assert!(!stderr.contains("private-user"));
    assert!(!stderr.contains("private-password"));
}

#[test]
fn rejects_invalid_endpoint_configuration_before_binding() {
    for (arguments, expected_error) in [
        (
            ["--allow-organizer", " padded"],
            "leading and trailing whitespace are not allowed",
        ),
        (["--path", "hooks"], "it must start with '/'"),
        (["--credential", "malformed"], "expected USERNAME:PASSWORD"),
        (
            [
                "--config",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/empty-multi.toml"
                ),
            ],
            "at least one [[webhooks]] entry is required",
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let bind = listener.local_addr().unwrap().to_string();
        let output = command(&bind).args(arguments).output().unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();

        assert!(!output.status.success());
        assert!(
            stderr.contains(expected_error),
            "expected {expected_error:?} in {stderr:?}"
        );
        assert!(
            !stderr.contains(" padded"),
            "filter value leaked: {stderr:?}"
        );
        assert!(!stderr.contains("Address already in use"));
    }
}

#[test]
fn credential_startup_diagnostics_identify_the_reference_without_leaking_its_value() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = listener.local_addr().unwrap().to_string();
    let config_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/credential-multi.toml"
    );
    let output = command(&bind)
        .args(["--config", config_path])
        .env("FIRST_OLD_CREDENTIAL", "private-user-without-a-password")
        .env("FIRST_CURRENT_CREDENTIAL", "valid:credential")
        .env("SECOND_CREDENTIAL", "valid:credential")
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success());
    assert!(stderr.contains("/incoming/first"), "{stderr:?}");
    assert!(stderr.contains("FIRST_OLD_CREDENTIAL"), "{stderr:?}");
    assert!(!stderr.contains("private-user-without-a-password"));
    assert!(!stderr.contains("valid:credential"));
    assert!(!stderr.contains("Address already in use"));
}

#[test]
fn aggregated_configuration_validation_precedes_listener_binding() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = listener.local_addr().unwrap().to_string();
    let config_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/invalid-semantic-multi.toml"
    );
    let output = command(&bind)
        .args(["--config", config_path])
        .env(
            "DUPLICATE_CREDENTIAL_REFERENCE",
            "private-user:private-secret",
        )
        .env_remove("MISSING_CREDENTIAL_REFERENCE")
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success());
    for expected in [
        "duplicate organizer slug",
        "duplicate resolved webhook path",
        "invalid relative webhook path",
        "MISSING_CREDENTIAL_REFERENCE",
    ] {
        assert!(
            stderr.contains(expected),
            "missing {expected:?} in {stderr:?}"
        );
    }
    // Every accumulated error must be readable on its own line rather than
    // escaped into one `Debug`-rendered string.
    assert!(
        !stderr.contains("\\n"),
        "report was escaped onto one line: {stderr:?}"
    );
    assert!(
        stderr.matches("\n- webhooks entry ").count() > 1,
        "report was not rendered as a list: {stderr:?}"
    );
    for sensitive in [
        "private-organizer",
        "private-event",
        "DUPLICATE_CREDENTIAL_REFERENCE",
        "private-user",
        "private-secret",
    ] {
        assert!(!stderr.contains(sensitive), "value leaked in {stderr:?}");
    }
    assert!(!stderr.contains("Address already in use"));
}

fn command(bind: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pretix-webhook"));
    command
        .args(["--bind", bind])
        .env_remove("PRETIX_WEBHOOK_ALLOW_ORGANIZERS")
        .env_remove("PRETIX_WEBHOOK_ALLOW_EVENTS")
        .env_remove("PRETIX_WEBHOOK_CREDENTIALS")
        .env_remove("PRETIX_WEBHOOK_PATH")
        .env_remove("PRETIX_WEBHOOK_PREFIX");
    command
}

fn startup_output(mut command: Command, final_diagnostic: &str) -> String {
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            sender.send(line.unwrap()).unwrap();
        }
    });
    let mut output = String::new();

    loop {
        let line = match receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(line) => line,
            Err(error) => {
                child.kill().unwrap();
                child.wait().unwrap();
                reader.join().unwrap();
                panic!("timed out waiting for {final_diagnostic:?}: {error}; output: {output:?}");
            }
        };
        output.push_str(&line);
        output.push('\n');
        if output.contains(final_diagnostic) {
            assert!(
                child.try_wait().unwrap().is_none(),
                "server exited during startup: {output:?}"
            );
            child.kill().unwrap();
            child.wait().unwrap();
            break;
        }
    }

    reader.join().unwrap();
    for line in receiver.try_iter() {
        output.push_str(&line);
        output.push('\n');
    }
    output
}
