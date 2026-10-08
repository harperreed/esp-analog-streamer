// ABOUTME: Checks actual std thread launch results stay distinct from pthread configuration cleanup.
// ABOUTME: Covers post-launch reset errors and real rejected stack allocation without ESP-IDF mocks.

#[path = "../../firmware/src/threads.rs"]
mod threads;

#[test]
fn successful_thread_launch_is_not_retryable_after_reset_failure() {
    let (finished, receiver) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().spawn(move || finished.send(()).unwrap());
    let mut diagnostic = None;
    let result = threads::finish_spawn(
        spawned,
        Err(std::io::Error::other("pthread reset failed")),
        |error| diagnostic = Some(error.to_string()),
    );
    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    assert!(result.is_ok(), "an existing worker must never be retried");
    assert_eq!(diagnostic.as_deref(), Some("pthread reset failed"));
}

#[test]
fn actual_launch_error_survives_an_independent_reset_failure() {
    let spawned = std::thread::Builder::new()
        .stack_size(usize::MAX)
        .spawn(|| ());
    assert!(spawned.is_err());
    let expected = spawned.as_ref().unwrap_err().to_string();
    let mut diagnostic = None;
    let result = threads::finish_spawn(
        spawned,
        Err(std::io::Error::other("pthread reset failed")),
        |error| diagnostic = Some(error.to_string()),
    );
    assert_eq!(result.unwrap_err().to_string(), expected);
    assert_eq!(diagnostic.as_deref(), Some("pthread reset failed"));
}
