//! The rice stage's lock, on `$STRILLIX_ROOT/stage/.stage.lock`. Rice writers
//! take it; core's conducting writers take `state/stage/.stage.lock` through
//! `aoide_storage::fs::with_stage_lock`. The two files never guard the same
//! state, so neither domain waits on the other, and each domain serialises its
//! own writers.

/// Runs `f` under the rice stage's lock, held for the closure's duration.
/// Best-effort and re-entrant, exactly as the conducting lock is
/// (`aoide_storage::fs::with_lock_in`).
pub fn with_stage_lock<T>(f: impl FnOnce() -> T) -> T {
    aoide_storage::fs::with_lock_in(&crate::paths::stage_dir(), f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, pin_roots, unique_tmp};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    #[test]
    fn the_rice_and_conducting_locks_are_two_files_with_no_shared_state() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_tmp("lock-split-files");
        let _env = pin_roots(&root);
        let rice = crate::paths::stage_dir().join(".stage.lock");
        let conducting = aoide_storage::fs::conducting_stage_dir().join(".stage.lock");
        assert_ne!(rice, conducting, "the two domains must never name one lock file");

        with_stage_lock(|| {});
        aoide_storage::fs::with_stage_lock(|| {});
        assert!(rice.is_file(), "the rice lock lives in $STRILLIX_ROOT/stage");
        assert!(conducting.is_file(), "the conducting lock lives in state/stage");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_held_rice_lock_never_blocks_a_conducting_writer() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_tmp("lock-split-independent");
        let _env = pin_roots(&root);
        let (tx, rx) = mpsc::channel();
        with_stage_lock(|| {
            std::thread::spawn(move || aoide_storage::fs::with_stage_lock(|| tx.send(()).unwrap()));
            rx.recv_timeout(Duration::from_secs(10))
                .expect("a conducting writer must not wait on a held rice lock");
        });
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_second_rice_writer_runs_only_after_the_first_releases() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_tmp("lock-split-rice-serial");
        let _env = pin_roots(&root);
        let released = Arc::new(AtomicBool::new(false));
        let (held_tx, held_rx) = mpsc::channel();
        let first = {
            let released = released.clone();
            std::thread::spawn(move || {
                with_stage_lock(|| {
                    held_tx.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(200));
                    released.store(true, Ordering::SeqCst);
                });
            })
        };
        held_rx.recv().unwrap();
        let saw_release = with_stage_lock(|| released.load(Ordering::SeqCst));
        assert!(saw_release, "the second rice writer must wait behind the first");
        first.join().unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_second_conducting_writer_runs_only_after_the_first_releases() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_tmp("lock-split-conducting-serial");
        let _env = pin_roots(&root);
        let released = Arc::new(AtomicBool::new(false));
        let (held_tx, held_rx) = mpsc::channel();
        let first = {
            let released = released.clone();
            std::thread::spawn(move || {
                aoide_storage::fs::with_stage_lock(|| {
                    held_tx.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(200));
                    released.store(true, Ordering::SeqCst);
                });
            })
        };
        held_rx.recv().unwrap();
        let saw_release = aoide_storage::fs::with_stage_lock(|| released.load(Ordering::SeqCst));
        assert!(saw_release, "the second conducting writer must wait behind the first");
        first.join().unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
}
