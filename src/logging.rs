use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::Path,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::Instant,
};

use axum::{extract::Request, middleware::Next, response::Response};
use log::Level;

/// Max records waiting to be written. When the disk can't keep up,
/// logging callers block instead of growing memory without bound.
const QUEUE_CAPACITY: usize = 10_000;

/// Default log file path.
pub const DEFAULT_LOG_FILE: &str = "logs/server.log";

/// `io::Write` that hands every formatted record to the writer thread.
pub struct ChannelWriter {
    tx: SyncSender<Vec<u8>>,
}

impl Write for ChannelWriter {
    /// Creates a new `ChannelWriter` with the given synchronous sender.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tx
            .send(buf.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "log writer thread is gone"))?;
        Ok(buf.len())
    }

    // Flushing is done by the writer thread once its queue is drained.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Spawns the `log-writer` thread appending everything received to `out`.
/// The thread exits after all `ChannelWriter`s are dropped.
pub fn spawn_writer<W: Write + Send + 'static>(out: W) -> (ChannelWriter, JoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel(QUEUE_CAPACITY);
    let handle = thread::Builder::new()
        .name("log-writer".into())
        .spawn(move || writer_loop(rx, out))
        .expect("failed to spawn log-writer thread");
    (ChannelWriter { tx }, handle)
}

/// Writer loop for the background log writer thread.
/// Receives log records from the channel and writes them to the output.
fn writer_loop<W: Write>(rx: Receiver<Vec<u8>>, out: W) {
    let mut out = BufWriter::new(out);
    // Block for the next record, then drain whatever else is queued and flush
    // once: batches writes under load, keeps the file current when idle.
    while let Ok(record) = rx.recv() {
        let _ = out.write_all(&record);
        loop {
            match rx.try_recv() {
                Ok(record) => {
                    let _ = out.write_all(&record);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let _ = out.flush();
                    return;
                }
            }
        }
        let _ = out.flush();
    }
}

/// Opens the log file at the given path, creating parent directories if necessary.
fn open_log_file(path: &Path) -> io::Result<File> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Sets up `env_logger` (level from `RUST_LOG`, `info` by default) writing
/// to `path` through the background writer thread.
pub fn init(path: &Path) -> io::Result<()> {
    let file = open_log_file(path)?;
    let (writer, _handle) = spawn_writer(file);
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Pipe(Box::new(writer)))
        .write_style(env_logger::WriteStyle::Never)
        .format_timestamp_millis()
        .try_init()
        .map_err(io::Error::other)
}

/// Determines the log level based on the HTTP status code. 5xx are errors, 4xx are warnings, others are info.
fn level_for(status: u16) -> Level {
    match status {
        500.. => Level::Error,
        400..=499 => Level::Warn,
        _ => Level::Info,
    }
}

/// Middleware: one line per request — client ip, method, uri, status, latency.
/// Logs the request after it has been processed by the next middleware or handler.
pub async fn log_requests(req: Request, next: Next) -> Response {
    /// Record the start time of the request to measure latency.
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();
    // Behind nginx the peer is always the proxy, the real client is in X-Real-IP.
    let client = req
        .headers()
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_owned();

    let resp = next.run(req).await;

    let status = resp.status();
    log::log!(
        target: "http",
        level_for(status.as_u16()),
        "{client} \"{method} {uri}\" {} {:.3}ms",
        status.as_u16(),
        // Calculate the elapsed time in milliseconds.
        start.elapsed().as_secs_f64() * 1000.0
    );
    resp
}

#[cfg(test)]
pub(crate) use tests::log_capture;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, OnceLock};

    /// Captures log output in a shared buffer for testing purposes.
    #[derive(Clone, Default)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Spawns a background writer thread that writes log records to the given writer.
    #[test]
    fn writer_thread_writes_all_records_in_order() {
        let buf = SharedBuf::default();
        let (mut writer, handle) = spawn_writer(buf.clone());
        for i in 0..1000 {
            writeln!(writer, "line {i}").unwrap();
        }
        drop(writer);
        handle.join().unwrap();

        let out = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        let expected: String = (0..1000).map(|i| format!("line {i}\n")).collect();
        assert_eq!(out, expected);
    }

    /// Ensures that the background writer thread runs on a separate named thread.
    #[test]
    fn writer_runs_on_separate_named_thread() {
        struct ThreadName(Arc<Mutex<Option<String>>>);
        impl Write for ThreadName {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                *self.0.lock().unwrap() = thread::current().name().map(String::from);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let seen = Arc::new(Mutex::new(None));
        let (mut writer, handle) = spawn_writer(ThreadName(seen.clone()));
        writer.write_all(b"x").unwrap();
        drop(writer);
        handle.join().unwrap();
        assert_eq!(seen.lock().unwrap().as_deref(), Some("log-writer"));
    }

    /// Ensures that writing to the channel fails when the background thread is gone.
    #[test]
    fn write_fails_when_thread_is_gone() {
        let (tx, rx) = mpsc::sync_channel(1);
        drop(rx);
        let mut writer = ChannelWriter { tx };
        let err = writer.write(b"x").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    /// Ensures that log records are flushed to the file even if the background writer thread is not explicitly shut down.
    #[test]
    fn records_are_flushed_to_file_without_shutdown() {
        let dir = std::env::temp_dir().join(format!("https_server_log_{}", std::process::id()));
        let path = dir.join("nested").join("test.log");
        let (mut writer, _handle) = spawn_writer(open_log_file(&path).unwrap());
        writer.write_all(b"hello\n").unwrap();

        // The thread flushes as soon as its queue is empty; poll for it.
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let content = loop {
            let content = fs::read_to_string(&path).unwrap();
            if !content.is_empty() || Instant::now() > deadline {
                break content;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        };
        drop(writer);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(content, "hello\n");
    }

    /// Ensures that opening the log file appends to it rather than overwriting it.
    #[test]
    fn open_log_file_appends() {
        let dir = std::env::temp_dir().join(format!("https_server_append_{}", std::process::id()));
        let path = dir.join("a.log");
        open_log_file(&path).unwrap().write_all(b"one\n").unwrap();
        open_log_file(&path).unwrap().write_all(b"two\n").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(content, "one\ntwo\n");
    }

    /// Ensures that HTTP status codes are mapped to the correct log levels.
    #[test]
    fn status_to_level() {
        assert_eq!(level_for(200), Level::Info);
        assert_eq!(level_for(304), Level::Info);
        assert_eq!(level_for(400), Level::Warn);
        assert_eq!(level_for(404), Level::Warn);
        assert_eq!(level_for(500), Level::Error);
        assert_eq!(level_for(503), Level::Error);
    }

    /// Test logger capturing records. The `log` logger is process-global, so
    /// it's installed once and tests look up their own lines by unique uri.
    /// Provides a way to inspect captured log records for testing purposes.
    pub(crate) struct Capture(Mutex<Vec<(Level, String, String)>>);

    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, r: &log::Record) {
            self.0
                .lock()
                .unwrap()
                .push((r.level(), r.target().to_owned(), r.args().to_string()));
        }
        fn flush(&self) {}
    }

    impl Capture {
        /// Finds all log records containing the specified `needle` in their message.
        ///
        /// Returns a vector of tuples `(level, target, message)` for each matching record.
        pub(crate) fn find(&self, needle: &str) -> Vec<(Level, String, String)> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, _, msg)| msg.contains(needle))
                .cloned()
                .collect()
        }
    }

    /// Installs the capturing logger (once per test binary) and returns it.
    pub(crate) fn log_capture() -> &'static Capture {
        static CAPTURE: OnceLock<&'static Capture> = OnceLock::new();
        CAPTURE.get_or_init(|| {
            let cap: &'static Capture = Box::leak(Box::new(Capture(Mutex::new(Vec::new()))));
            log::set_logger(cap).unwrap();
            log::set_max_level(log::LevelFilter::Trace);
            cap
        })
    }
}
