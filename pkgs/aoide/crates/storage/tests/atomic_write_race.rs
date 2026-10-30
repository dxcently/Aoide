//! Two threads of one process write one target through `atomic_write`, 500
//! whole contents each, while a third thread reads the target the whole time.
//! A temp name shared by both writers lets a rename find its temp already
//! moved (ENOENT), or lets a reader catch the other writer's bytes mid-write.
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const WRITES_PER_THREAD: usize = 500;

/// One whole content: a tag and a number name it, and the padding is long
/// enough that a torn read cannot pass for one.
fn content(tag: char, i: usize) -> String {
    format!("{tag}{i:06}{}", "~".repeat(8 * 1024))
}

#[test]
fn two_writers_in_one_process_never_fail_and_a_reader_sees_one_whole_content() {
    let dir = std::env::temp_dir().join(format!("aoide-atomic-race-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("graph.json");
    let initial = content('0', 0);
    aoide_storage::fs::atomic_write(&target, &initial).unwrap();
    let mut allowed: HashSet<String> =
        (0..WRITES_PER_THREAD).flat_map(|i| [content('a', i), content('b', i)]).collect();
    allowed.insert(initial);

    let errors = Mutex::new(Vec::new());
    let done = AtomicBool::new(false);
    let (writers_joined, (reads, torn)) = std::thread::scope(|s| {
        let reader = s.spawn(|| {
            let (mut reads, mut torn) = (0usize, Vec::new());
            while !done.load(Ordering::SeqCst) {
                match std::fs::read_to_string(&target) {
                    Ok(seen) => {
                        reads += 1;
                        if !allowed.contains(&seen) {
                            torn.push(seen.chars().take(24).collect::<String>());
                        }
                    }
                    Err(e) => torn.push(format!("read failed: {e}")),
                }
            }
            (reads, torn)
        });
        let writers: Vec<_> = ['a', 'b']
            .into_iter()
            .map(|tag| {
                let (target, errors) = (&target, &errors);
                s.spawn(move || {
                    for i in 0..WRITES_PER_THREAD {
                        if let Err(e) = aoide_storage::fs::atomic_write(target, &content(tag, i)) {
                            errors.lock().unwrap_or_else(|p| p.into_inner()).push(format!("{tag}{i}: {e}"));
                        }
                    }
                })
            })
            .collect();
        let writers_joined = writers.into_iter().all(|w| w.join().is_ok());
        done.store(true, Ordering::SeqCst);
        (writers_joined, reader.join().expect("the reader panicked"))
    });

    let errors = errors.into_inner().unwrap_or_else(|p| p.into_inner());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(writers_joined, "a writer thread panicked");
    assert!(
        errors.is_empty(),
        "{} of {} writes failed, first: {:?}",
        errors.len(),
        2 * WRITES_PER_THREAD,
        errors.first()
    );
    assert!(reads > 0, "the reader never read");
    assert!(torn.is_empty(), "{} reads were not one whole content, first: {:?}", torn.len(), torn.first());
}
