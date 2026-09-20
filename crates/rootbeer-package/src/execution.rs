use std::io;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// Shared cancellation and wall-clock budget for one package operation.
#[derive(Debug, Clone, Default)]
pub struct Execution {
    deadline: Option<Instant>,
    cancelled: Arc<AtomicBool>,
}

impl Execution {
    pub fn with_timeout(timeout: Duration) -> io::Result<Self> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .filter(|_| !timeout.is_zero())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "execution timeout must be positive and representable",
                )
            })?;
        Ok(Self {
            deadline: Some(deadline),
            ..Self::default()
        })
    }

    /// Flag shared with callers and process signal handlers.
    pub fn cancellation_flag(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn check(&self) -> io::Result<()> {
        self.remaining().map(|_| ())
    }

    pub fn remaining(&self) -> io::Result<Option<Duration>> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "package execution cancelled",
            ));
        }
        self.deadline
            .map(|deadline| {
                deadline
                    .checked_duration_since(Instant::now())
                    .filter(|remaining| !remaining.is_zero())
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::TimedOut,
                            "package execution deadline reached",
                        )
                    })
            })
            .transpose()
    }

    pub(crate) fn sleep(&self, duration: Duration) -> io::Result<()> {
        let end = Instant::now() + duration;
        loop {
            self.check()?;
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(());
            }
            std::thread::sleep(remaining.min(Duration::from_millis(50)));
        }
    }
}
