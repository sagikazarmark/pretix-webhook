use std::{net::TcpListener, process::Command};

#[test]
fn warns_on_stderr_only_when_both_filters_are_omitted() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = listener.local_addr().unwrap().to_string();

    let unrestricted = command(&bind).output().unwrap();
    let unrestricted_stderr = String::from_utf8(unrestricted.stderr).unwrap();
    assert!(
        unrestricted_stderr
            .contains("warning: no filters configured; accepting all events from all organizers")
    );

    let restricted = command(&bind)
        .args(["--allow-organizer", "acmecorp"])
        .output()
        .unwrap();
    let restricted_stderr = String::from_utf8(restricted.stderr).unwrap();
    assert!(!restricted_stderr.contains("warning: no filters configured"));
}

fn command(bind: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pretix-webhook"));
    command
        .args(["--bind", bind])
        .env_remove("PRETIX_WEBHOOK_ALLOW_ORGANIZERS")
        .env_remove("PRETIX_WEBHOOK_ALLOW_EVENTS");
    command
}
