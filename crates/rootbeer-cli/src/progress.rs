use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rootbeer_core::package::progress::Event;

static IS_REPORTING: AtomicBool = AtomicBool::new(false);
static LAST_DOWNLOAD_DRAW: Mutex<Option<Instant>> = Mutex::new(None);

pub(crate) struct Progress {
    worker: Option<(Sender<()>, JoinHandle<()>)>,
}

impl Progress {
    pub(crate) fn new(label: &'static str) -> Self {
        if !is_interactive() {
            eprintln!("  {label}...");
            return Self { worker: None };
        }

        let (sender, receiver) = mpsc::channel();
        let started = Instant::now();
        draw(label, 0, started);
        let worker = thread::spawn(move || {
            let mut frame = 0;
            while receiver.recv_timeout(Duration::from_millis(100))
                == Err(mpsc::RecvTimeoutError::Timeout)
            {
                frame += 1;
                draw(label, frame, started);
            }
            let mut stderr = io::stderr().lock();
            let _ = write!(stderr, "\r\x1b[2K");
            let _ = stderr.flush();
        });
        Self {
            worker: Some((sender, worker)),
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        if let Some((sender, worker)) = self.worker.take() {
            let _ = sender.send(());
            let _ = worker.join();
        }
    }
}

fn is_interactive() -> bool {
    io::stderr().is_terminal() && std::env::var("TERM").as_deref() != Ok("dumb")
}

/// Draws package progress on one line; registered with `progress::observe`.
pub(crate) fn report(event: &Event) {
    if !is_interactive() {
        return;
    }

    let mut last = LAST_DOWNLOAD_DRAW
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let line = match event {
        Event::Done => {
            *last = None;
            IS_REPORTING.store(false, Ordering::Relaxed);
            let mut stderr = io::stderr().lock();
            let _ = write!(stderr, "\r\x1b[2K");
            let _ = stderr.flush();
            return;
        }
        Event::Install { name } => format!("installing {name}"),
        Event::Download {
            url,
            received,
            total,
        } => {
            if last.is_some_and(|drawn| drawn.elapsed() < Duration::from_millis(100)) {
                return;
            }
            *last = Some(Instant::now());
            let size = match total {
                Some(total) if total >= received && *total > 0 => format!(
                    "{} / {} ({}%)",
                    bytes(*received),
                    bytes(*total),
                    received * 100 / total
                ),
                _ => bytes(*received),
            };
            format!("downloading {} {size}", download_name(url))
        }
    };

    IS_REPORTING.store(true, Ordering::Relaxed);
    let mut stderr = io::stderr().lock();
    let _ = write!(stderr, "\r\x1b[2K  {line}");
    let _ = stderr.flush();
}

fn download_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = name.split('@').next().unwrap_or(name);
    name.chars().take(48).collect()
}

fn bytes(count: u64) -> String {
    let count = count as f64;
    match count {
        count if count >= 1e9 => format!("{:.1} GB", count / 1e9),
        count if count >= 1e6 => format!("{:.1} MB", count / 1e6),
        count => format!("{:.0} KB", count / 1e3),
    }
}

fn draw(label: &str, frame: usize, started: Instant) {
    if IS_REPORTING.load(Ordering::Relaxed) {
        return;
    }
    let frames = ['|', '/', '-', '\\'];
    let mut stderr = io::stderr().lock();
    let _ = write!(
        stderr,
        "\r\x1b[2K  {} {label} ({:.1}s)",
        frames[frame % frames.len()],
        started.elapsed().as_secs_f32()
    );
    let _ = stderr.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_downloads_by_their_last_path_segment() {
        assert_eq!(
            download_name("https://github.com/o/r/releases/download/v1/rg-14.tar.gz?x=1"),
            "rg-14.tar.gz"
        );
        assert_eq!(download_name("ghcr://owner/tool@sha256:abc"), "tool");
    }

    #[test]
    fn formats_sizes_in_decimal_units() {
        assert_eq!(bytes(512), "1 KB");
        assert_eq!(bytes(12_300_000), "12.3 MB");
        assert_eq!(bytes(2_500_000_000), "2.5 GB");
    }
}
