use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;
use std::time::Instant;

/// A unique package operation and the operations that must complete before it.
pub struct Task {
    pub key: String,
    pub dependencies: Vec<String>,
    pub is_source: bool,
}

/// Runs dependency-ready operations within package-worker and compiler-slot limits.
/// Source allocations share `jobs`; imports receive one job without reserving compiler slots.
/// Dependencies absent from `tasks` must already be satisfied by the caller.
pub fn run<R: Send>(
    tasks: Vec<Task>,
    workers: usize,
    jobs: usize,
    operation: impl Fn(&str, usize) -> Result<R, String> + Sync,
    mut complete: impl FnMut(String, Result<R, String>) -> bool,
) -> Result<(), String> {
    if !(1..=64).contains(&workers) || !(1..=64).contains(&jobs) {
        return Err("workers and jobs must be between 1 and 64".into());
    }
    let mut pending = BTreeMap::new();
    for task in tasks {
        if pending.insert(task.key.clone(), task).is_some() {
            return Err("duplicate build task".into());
        }
    }
    let keys: BTreeSet<_> = pending.keys().cloned().collect();
    for task in pending.values_mut() {
        task.dependencies.retain(|key| keys.contains(key));
    }
    let mut priority = BTreeMap::new();
    while priority.len() < pending.len() {
        let previous = priority.len();
        for key in pending.keys() {
            if priority.contains_key(key) {
                continue;
            }
            let dependants: Vec<_> = pending
                .values()
                .filter(|task| task.dependencies.contains(key))
                .collect();
            if dependants
                .iter()
                .all(|task| priority.contains_key(&task.key))
            {
                let depth = dependants
                    .iter()
                    .map(|task| priority[&task.key])
                    .max()
                    .unwrap_or(0);
                priority.insert(key.clone(), depth + 1);
            }
        }
        if priority.len() == previous {
            return Err("build task graph contains a cycle".into());
        }
    }
    let mut completed = BTreeMap::new();
    let mut available = jobs;
    let mut active = 0;
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        while !pending.is_empty() || active > 0 {
            let blocked: Vec<_> = pending
                .values()
                .filter_map(|task| {
                    task.dependencies
                        .iter()
                        .find(|key| completed.get(*key) == Some(&false))
                        .map(|dependency| (task.key.clone(), dependency.clone()))
                })
                .collect();
            for (key, dependency) in blocked {
                pending.remove(&key);
                complete(key.clone(), Err(format!("dependency {dependency} failed")));
                completed.insert(key, false);
            }
            while active < workers {
                let ready: Vec<_> = pending
                    .values()
                    .filter(|task| {
                        task.dependencies
                            .iter()
                            .all(|key| completed.get(key) == Some(&true))
                            && (!task.is_source || available > 0)
                    })
                    .collect();
                let Some(task) = ready
                    .iter()
                    .max_by_key(|task| (priority[&task.key], std::cmp::Reverse(&task.key)))
                else {
                    break;
                };
                let is_source = task.is_source;
                let allocation = if is_source {
                    let slots = ready
                        .iter()
                        .filter(|task| task.is_source)
                        .count()
                        .min(workers - active)
                        .min(available);
                    available.div_ceil(slots)
                } else {
                    0
                };
                let key = task.key.clone();
                pending.remove(&key);
                available -= allocation;
                active += 1;
                let (sender, operation) = (sender.clone(), &operation);
                eprintln!(
                    "START {key} ({}; jobs={})",
                    if is_source { "source" } else { "import" },
                    allocation.max(1)
                );
                scope.spawn(move || {
                    let started = Instant::now();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        operation(&key, allocation.max(1))
                    }))
                    .unwrap_or_else(|_| Err("package worker panicked".into()));
                    let _ = sender.send((key, allocation, result, started.elapsed()));
                });
            }
            if active == 0 {
                continue;
            }
            let (key, allocation, result, elapsed) = receiver.recv().unwrap();
            available += allocation;
            active -= 1;
            eprintln!("FINISH {key} ({:.1}s)", elapsed.as_secs_f64());
            let is_success = complete(key.clone(), result);
            completed.insert(key, is_success);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Condvar, Mutex,
    };

    fn task(key: &str, dependencies: &[&str]) -> Task {
        Task {
            key: key.into(),
            dependencies: dependencies.iter().map(|key| (*key).into()).collect(),
            is_source: true,
        }
    }

    #[test]
    fn independent_sources_overlap_within_the_cpu_budget_then_unlock_dependants() {
        let arrivals = Mutex::new(0);
        let ready = Condvar::new();
        let occupied = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let finished = Mutex::new(Vec::new());
        let mut results = BTreeMap::new();
        run(
            vec![
                task("compiler", &[]),
                task("library", &[]),
                task("app", &["compiler", "library"]),
            ],
            2,
            3,
            |key, jobs| {
                let current = occupied.fetch_add(jobs, Ordering::SeqCst) + jobs;
                peak.fetch_max(current, Ordering::SeqCst);
                assert!(current <= 3);
                if key == "app" {
                    assert_eq!(finished.lock().unwrap().len(), 2);
                    assert_eq!(jobs, 3);
                } else {
                    let mut arrivals = arrivals.lock().unwrap();
                    *arrivals += 1;
                    ready.notify_all();
                    let (_arrivals, timeout) = ready
                        .wait_timeout_while(arrivals, std::time::Duration::from_secs(5), |count| {
                            *count < 2
                        })
                        .unwrap();
                    assert!(!timeout.timed_out());
                }
                finished.lock().unwrap().push(key.to_string());
                occupied.fetch_sub(jobs, Ordering::SeqCst);
                Ok(())
            },
            |key, result| {
                let is_success = result.is_ok();
                results.insert(key, result);
                is_success
            },
        )
        .unwrap();
        assert_eq!(peak.load(Ordering::SeqCst), 3);
        assert_eq!(results.len(), 3);
        assert!(results.values().all(Result::is_ok));
    }

    #[test]
    fn dependency_failures_block_consumers_without_discarding_independent_work() {
        let started = Mutex::new(Vec::new());
        let mut results = BTreeMap::new();
        run(
            vec![
                task("broken", &[]),
                task("consumer", &["broken"]),
                task("leaf", &["consumer"]),
                task("independent", &[]),
            ],
            2,
            2,
            |key, _| {
                started.lock().unwrap().push(key.to_string());
                if key == "broken" {
                    Err("failed check".into())
                } else {
                    Ok(())
                }
            },
            |key, result| {
                let is_success = result.is_ok();
                results.insert(key, result);
                is_success
            },
        )
        .unwrap();
        assert_eq!(started.into_inner().unwrap().len(), 2);
        assert!(results["independent"].is_ok());
        assert!(results["consumer"].as_ref().unwrap_err().contains("broken"));
        assert!(results["leaf"].is_err());
    }

    #[test]
    fn worker_panics_release_capacity_and_finish_the_run() {
        let mut results = BTreeMap::new();
        run(
            vec![task("broken", &[]), task("independent", &[])],
            1,
            2,
            |key, _| {
                assert_ne!(key, "broken");
                Ok(())
            },
            |key, result| {
                let is_success = result.is_ok();
                results.insert(key, result);
                is_success
            },
        )
        .unwrap();
        assert!(results["broken"].is_err());
        assert!(results["independent"].is_ok());
    }
    #[test]
    fn imports_overlap_sources_without_reserving_compiler_slots() {
        let arrivals = Mutex::new(0);
        let ready = Condvar::new();
        let mut import = task("z-archive", &[]);
        import.is_source = false;
        run(
            vec![import, task("compiler", &[])],
            2,
            4,
            |key, jobs| {
                assert_eq!(jobs, if key == "compiler" { 4 } else { 1 });
                let mut arrivals = arrivals.lock().unwrap();
                *arrivals += 1;
                ready.notify_all();
                let (_arrivals, timeout) = ready
                    .wait_timeout_while(arrivals, std::time::Duration::from_secs(5), |count| {
                        *count < 2
                    })
                    .unwrap();
                assert!(!timeout.timed_out());
                Ok(())
            },
            |_, result| result.is_ok(),
        )
        .unwrap();
    }

    #[test]
    fn longer_dependency_chains_start_before_alphabetical_leaves() {
        let started = Mutex::new(Vec::new());
        run(
            vec![
                task("a", &[]),
                task("z", &[]),
                task("middle", &["z"]),
                task("last", &["middle"]),
            ],
            1,
            1,
            |key, _| {
                started.lock().unwrap().push(key.to_string());
                Ok(())
            },
            |_, result| result.is_ok(),
        )
        .unwrap();
        assert_eq!(&started.into_inner().unwrap()[..2], &["z", "middle"]);
    }

    #[test]
    fn invalid_graphs_never_start_work() {
        for tasks in [
            vec![task("a", &["b"]), task("b", &["a"])],
            vec![task("a", &[]), task("a", &[])],
        ] {
            assert!(run(
                tasks,
                2,
                2,
                |_, _| -> Result<(), String> { panic!("invalid graph started work") },
                |_, _| true
            )
            .is_err());
        }
    }
}
