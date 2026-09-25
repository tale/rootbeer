//! Process-wide progress events, drawn by the CLI.

use std::sync::OnceLock;

pub enum Event<'a> {
    Download {
        url: &'a str,
        received: u64,
        total: Option<u64>,
    },
    Install {
        name: &'a str,
    },
    Done,
}

static OBSERVER: OnceLock<fn(&Event)> = OnceLock::new();

/// Registers the process-wide receiver for progress [`Event`]s.
pub fn observe(observer: fn(&Event)) {
    let _ = OBSERVER.set(observer);
}

pub(crate) fn report(event: &Event) {
    if let Some(observer) = OBSERVER.get() {
        observer(event);
    }
}

/// Reports [`Event::Done`] when dropped, so a failed step still clears its line.
pub(crate) struct Step;

impl Drop for Step {
    fn drop(&mut self) {
        report(&Event::Done);
    }
}

pub(crate) fn install(name: &str) -> Step {
    report(&Event::Install { name });
    Step
}
