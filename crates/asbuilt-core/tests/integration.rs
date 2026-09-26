// The crate as a consumer sees it: only `pub` items. Shared fixtures are
// in `tests/common/mod.rs`, once.

mod common;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use asbuilt_core::{Config, Error};
use common::{PollError, Workspace, poll_until};

#[test]
fn a_config_written_at_the_root_is_what_load_returns() {
    let ws = Workspace::new();
    ws.write("asbuilt.toml", "[output]\npath = \"arch/model.c4\"\n");

    let config = Config::load(ws.root()).unwrap();

    assert_eq!(config.output.path, "arch/model.c4");
}

#[test]
fn a_config_in_a_subdirectory_is_not_found_from_the_root() {
    let ws = Workspace::new();
    ws.write("sub/asbuilt.toml", "[output]\npath = \"never.c4\"\n");

    let config = Config::load(ws.root()).unwrap();

    assert_eq!(config, Config::default());
}

#[test]
fn a_typo_in_the_output_table_names_the_file_it_is_in() {
    let ws = Workspace::new();
    let path = ws.write("asbuilt.toml", "[output]\npth = \"x\"\n");

    match Config::load(ws.root()) {
        Err(Error::Config { path: p, .. }) => assert_eq!(p, path),
        other => panic!("expected Config error, got {other:?}"),
    }
}

#[test]
fn a_poll_stops_at_its_deadline_and_says_what_it_waited_for() {
    let result: Result<(), PollError<()>> = poll_until(
        "a value that never arrives",
        Duration::from_millis(60),
        || Ok(None),
    );

    match result {
        Err(PollError::Deadline { what, waited }) => {
            assert_eq!(what, "a value that never arrives");
            assert!(
                waited >= Duration::from_millis(60),
                "gave up early after {waited:?}"
            );
        }
        other => panic!("expected Deadline, got {other:?}"),
    }
}

#[test]
fn a_probe_error_fails_the_poll_immediately() {
    let result: Result<(), PollError<&str>> =
        poll_until("anything", Duration::from_secs(5), || {
            Err("driver went away")
        });

    assert!(
        matches!(result, Err(PollError::Probe("driver went away"))),
        "got {result:?}"
    );
}

// The shape for a test that needs something a fresh clone may not have
// (a tool, a service, real time). It is gated, never skipped: a missing
// precondition is a reason string on the attribute, and the lane that has
// the precondition runs it with `--run-ignored only`.
#[test]
#[ignore = "waits on real time; run with: cargo nextest run --run-ignored only -E 'test(/^live_/)'"]
fn live_poll_sees_a_value_produced_on_another_thread() {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        tx.send(7).unwrap();
    });

    let got = poll_until(
        "a value from the producer thread",
        Duration::from_secs(2),
        || match rx.try_recv() {
            Ok(v) => Ok(Some(v)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(e @ mpsc::TryRecvError::Disconnected) => Err(e),
        },
    );

    assert!(matches!(got, Ok(7)), "got {got:?}");
}
