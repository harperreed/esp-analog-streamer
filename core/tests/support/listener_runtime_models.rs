// ABOUTME: Checks actual runtime admission, cancellation and epoch ownership under Loom.
// ABOUTME: Calls commit directly; channel delivery and wall-clock timeout scheduling are excluded.
use super::*;
use crate::models::check;
use loom::thread;

fn request(force: bool, token: Arc<Mutex<bool>>) -> Request {
    let (notify, _) = mpsc::sync_channel(1);
    Request {
        prepared: Prepared::new(OutputFormat::Flac, false).unwrap(),
        force,
        token,
        deadline: Duration::from_secs(86400),
        outcome: Arc::new(Mutex::new(None)),
        notify,
    }
}

#[test]
fn token_revocation_races_actual_commit() {
    check(|| {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let old = worker.epoch.clone();
        let token: loom::sync::Arc<loom::sync::Mutex<bool>> = Arc::new(Mutex::new(true));
        let _: &loom::sync::Mutex<loom::sync::Arc<Epoch>> = &runtime.active;
        let pending = request(true, token.clone());
        let replacement = pending.prepared.epoch.clone();
        let outcome = pending.outcome.clone();
        let revoke = thread::spawn({
            let token = token.clone();
            move || *token.lock().unwrap() = false
        });
        let commit = thread::spawn({
            let runtime = runtime.clone();
            move || {
                let committed = worker.commit(&runtime, pending);
                (worker, committed)
            }
        });
        revoke.join().unwrap();
        let (mut worker, committed) = commit.join().unwrap();
        assert_eq!(old.retired(), committed);
        assert_eq!(
            Arc::ptr_eq(&runtime.active.lock().unwrap(), &replacement),
            committed
        );
        assert_eq!(
            matches!(*outcome.lock().unwrap(), Some(Ok(true))),
            committed
        );
        let active = runtime.active.lock().unwrap().clone();
        assert!(!worker.commit(&runtime, request(true, token)));
        assert!(Arc::ptr_eq(&runtime.active.lock().unwrap(), &active));
        assert_eq!(runtime.listeners.load(Ordering::Acquire), 0);
        drop((worker, runtime, outcome, replacement, active));
        assert_eq!(Arc::strong_count(&old), 1);
    });
}

fn admission_commit(force: bool) {
    check(move || {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let old = worker.epoch.clone();
        let pending = request(force, Arc::new(Mutex::new(true)));
        let replacement = pending.prepared.epoch.clone();
        let outcome = pending.outcome.clone();
        let admit = thread::spawn({
            let runtime = runtime.clone();
            move || runtime.admit("GET /stream.flac HTTP/1.1").1.unwrap()
        });
        let commit = thread::spawn({
            let runtime = runtime.clone();
            move || {
                let committed = worker.commit(&runtime, pending);
                (worker, committed)
            }
        });
        let lease = admit.join().unwrap();
        let (worker, committed) = commit.join().unwrap();
        let leased_old = Arc::ptr_eq(&lease.epoch, &old);
        if force {
            assert!(committed);
        } else {
            assert_eq!(committed, !leased_old);
        }
        assert_eq!(old.retired(), committed);
        assert_eq!(
            matches!(*outcome.lock().unwrap(), Some(Ok(true))),
            committed
        );
        assert_eq!(
            Arc::ptr_eq(&runtime.active.lock().unwrap(), &replacement),
            committed
        );
        assert_eq!(runtime.listeners.load(Ordering::Acquire), 1);
        assert_eq!(lease.epoch.retired(), leased_old && committed);
        drop(lease);
        assert_eq!(runtime.listeners.load(Ordering::Acquire), 0);
        drop((worker, runtime, replacement, outcome));
        assert_eq!(Arc::strong_count(&old), 1);
    });
}
#[test]
fn admission_races_idle_commit() {
    admission_commit(false);
}
#[test]
fn admission_races_forced_commit() {
    admission_commit(true);
}

#[test]
fn expired_request_cannot_commit() {
    check(|| {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let old = worker.epoch.clone();
        let mut pending = request(true, Arc::new(Mutex::new(true)));
        pending.deadline = Duration::ZERO;
        let outcome = pending.outcome.clone();
        assert!(!worker.commit(&runtime, pending));
        assert!(outcome.lock().unwrap().as_ref().unwrap().is_err());
        assert!(Arc::ptr_eq(&runtime.active.lock().unwrap(), &old));
        assert!(!old.retired());
        drop((worker, runtime));
        assert_eq!(Arc::strong_count(&old), 1);
    });
}

#[test]
fn completed_outcome_cannot_commit() {
    check(|| {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let old = worker.epoch.clone();
        let pending = request(true, Arc::new(Mutex::new(true)));
        *pending.outcome.lock().unwrap() = Some(Ok(false));
        assert!(!worker.commit(&runtime, pending));
        assert!(Arc::ptr_eq(&runtime.active.lock().unwrap(), &old));
        assert!(!old.retired());
    });
}

#[test]
fn outcome_revocation_races_actual_commit() {
    check(|| {
        let (runtime, mut worker) = Runtime::new(OutputFormat::Flac, false).unwrap();
        let old = worker.epoch.clone();
        let pending = request(true, Arc::new(Mutex::new(true)));
        let outcome = pending.outcome.clone();
        let revoked_outcome = outcome.clone();
        let revoke = thread::spawn(move || {
            let mut outcome = revoked_outcome.lock().unwrap();
            if outcome.is_none() {
                *outcome = Some(Err(io::Error::other("revoked")));
            }
        });
        let commit = thread::spawn({
            let runtime = runtime.clone();
            move || {
                let committed = worker.commit(&runtime, pending);
                (worker, committed)
            }
        });
        revoke.join().unwrap();
        let (worker, committed) = commit.join().unwrap();
        assert_eq!(old.retired(), committed);
        assert_eq!(
            matches!(*outcome.lock().unwrap(), Some(Ok(true))),
            committed
        );
        assert_eq!(
            Arc::ptr_eq(&runtime.active.lock().unwrap(), &old),
            !committed
        );
        drop((worker, runtime));
        assert_eq!(Arc::strong_count(&old), 1);
    });
}
