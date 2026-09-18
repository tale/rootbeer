use std::io::{self, IsTerminal, Write};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(crate) struct Progress {
    worker: Option<(Sender<()>, JoinHandle<()>)>,
}

impl Progress {
    pub(crate) fn new(label: &'static str) -> Self {
        if !io::stderr().is_terminal() || std::env::var("TERM").as_deref() == Ok("dumb") {
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

fn draw(label: &str, frame: usize, started: Instant) {
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
