use std::collections::BTreeMap;
use std::sync::mpsc;

pub(super) struct Task {
    pub key: String,
    pub dependencies: Vec<String>,
    pub is_source: bool,
}

pub(super) fn run<R: Send>(
    tasks: Vec<Task>,
    workers: usize,
    jobs: usize,
    operation: impl Fn(&str, usize) -> Result<R, String> + Sync,
    mut complete: impl FnMut(String, Result<R, String>) -> bool,
) {
    let mut pending: BTreeMap<_, _> = tasks
        .into_iter()
        .map(|task| (task.key.clone(), task))
        .collect();
    let keys: std::collections::BTreeSet<_> = pending.keys().cloned().collect();
    for task in pending.values_mut() {
        task.dependencies.retain(|key| keys.contains(key));
    }
    let priority: BTreeMap<_, _> = keys
        .iter()
        .map(|key| {
            let dependants = pending
                .values()
                .filter(|task| task.dependencies.contains(key))
                .count();
            (key.clone(), dependants)
        })
        .collect();
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
            while active < workers && available > 0 {
                let ready: Vec<_> = pending
                    .values()
                    .filter(|task| {
                        task.dependencies
                            .iter()
                            .all(|key| completed.get(key) == Some(&true))
                    })
                    .collect();
                let Some(task) = ready
                    .iter()
                    .max_by_key(|task| (priority[&task.key], std::cmp::Reverse(&task.key)))
                else {
                    break;
                };
                let slots = ready.len().min(workers - active).min(available);
                let allocation = if task.is_source {
                    available.div_ceil(slots)
                } else {
                    1
                };
                let key = task.key.clone();
                pending.remove(&key);
                available -= allocation;
                active += 1;
                let (sender, operation) = (sender.clone(), &operation);
                scope.spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        operation(&key, allocation)
                    }))
                    .unwrap_or_else(|_| Err("package worker panicked".into()));
                    let _ = sender.send((key, allocation, result));
                });
            }
            if active == 0 {
                for (key, _) in std::mem::take(&mut pending) {
                    complete(key, Err("unresolved package dependencies".into()));
                }
                break;
            }
            let (key, allocation, result) = receiver.recv().unwrap();
            available += allocation;
            active -= 1;
            let is_success = complete(key.clone(), result);
            completed.insert(key, is_success);
        }
    });
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
        );
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
        );
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
        );
        assert!(results["broken"].is_err());
        assert!(results["independent"].is_ok());
    }
}
