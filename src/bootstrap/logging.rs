//! Tracing subscriber setup driven by effective `LoggingConfig`.
//!
//! Both the filter (level) and the output layer (format, destination) are wrapped in
//! [`reload::Layer`] so every logging option can be changed at runtime via the admin API.

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, RwLock};

use tracing_subscriber::{
    fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer, Registry, reload,
};

use crate::config::LoggingConfig;

/// Subscriber stack after the reloadable filter layer is attached.
type FilteredRegistry = tracing_subscriber::layer::Layered<
    reload::Layer<EnvFilter, Registry>,
    Registry,
>;

/// Hot-reload handles for the global tracing subscriber.
pub struct LoggingReload {
    filter_handle: reload::Handle<EnvFilter, Registry>,
    output_handle: reload::Handle<Box<dyn Layer<FilteredRegistry> + Send + Sync>, FilteredRegistry>,
    appender_guard: RwLock<Option<tracing_appender::non_blocking::WorkerGuard>>,
}

impl LoggingReload {
    /// Rebuild and apply a full logging configuration (level, format, output, file path, rotation).
    pub fn reload(&self, logging: &LoggingConfig) -> Result<(), String> {
        self.filter_handle
            .reload(env_filter_for_level(&logging.level))
            .map_err(|e| e.to_string())?;

        let (output_layer, guard) = build_output_layer(logging)?;
        *self.appender_guard.write().unwrap() = guard;
        self.output_handle
            .reload(output_layer)
            .map_err(|e| e.to_string())
    }
}

/// Keeps reload handles and the file-appender worker guard alive for the process lifetime.
pub struct TracingGuard {
    reload: Arc<LoggingReload>,
}

impl TracingGuard {
    pub fn reload(&self) -> Arc<LoggingReload> {
        self.reload.clone()
    }
}

/// Build the `EnvFilter` used for the server crates. `RUST_LOG` wins when set.
pub fn env_filter_for_level(level: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "elidune_server={},tower_http=debug,z3950_rs=debug",
            level
        ))
    })
}

/// Initialise the global tracing subscriber from the effective logging configuration.
pub fn init(logging: &LoggingConfig) -> Result<TracingGuard, String> {
    let (filter_layer, filter_handle) = reload::Layer::new(env_filter_for_level(&logging.level));
    let (output_layer, appender_guard) = build_output_layer(logging)?;
    let (output_layer, output_handle) = reload::Layer::new(output_layer);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(output_layer)
        .init();

    Ok(TracingGuard {
        reload: Arc::new(LoggingReload {
            filter_handle,
            output_handle,
            appender_guard: RwLock::new(appender_guard),
        }),
    })
}

fn build_output_layer(
    logging: &LoggingConfig,
) -> Result<
    (
        Box<dyn Layer<FilteredRegistry> + Send + Sync>,
        Option<tracing_appender::non_blocking::WorkerGuard>,
    ),
    String,
> {
    let log_format = logging.format.as_str();

    match logging.output.as_str() {
        "syslog" => {
            if journald::available() {
                Ok((build_fmt_layer(log_format, journald::Writer), None))
            } else {
                eprintln!("journald socket unavailable, falling back to stderr");
                Ok((build_fmt_layer(log_format, std::io::stderr), None))
            }
        }
        "stderr" => Ok((build_fmt_layer(log_format, std::io::stderr), None)),
        "file" => {
            let (non_blocking, guard) = build_file_writer(logging)?;
            Ok((build_fmt_layer_writer(log_format, non_blocking), Some(guard)))
        }
        _ => Ok((build_fmt_layer(log_format, std::io::stdout), None)),
    }
}

fn build_file_writer(
    logging: &LoggingConfig,
) -> Result<
    (
        tracing_appender::non_blocking::NonBlocking,
        tracing_appender::non_blocking::WorkerGuard,
    ),
    String,
> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};

    let file_path = logging.file_path.as_deref().ok_or_else(|| {
        "logging.output = \"file\" requires logging.file_path to be set".to_string()
    })?;
    let dir = Path::new(file_path).parent().unwrap_or(Path::new("."));
    let filename = Path::new(file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("elidune.log");

    let rotation = logging.file_rotation.as_deref().unwrap_or("daily");
    match rotation {
        "monthly" => {
            let writer = monthly::Writer::open(dir, filename)
                .map_err(|e| format!("monthly log file: {e}"))?;
            Ok(tracing_appender::non_blocking(writer))
        }
        "weekly" => Ok(tracing_appender::non_blocking(RollingFileAppender::new(
            Rotation::WEEKLY,
            dir,
            filename,
        ))),
        "never" => Ok(tracing_appender::non_blocking(RollingFileAppender::new(
            Rotation::NEVER,
            dir,
            filename,
        ))),
        _ => Ok(tracing_appender::non_blocking(RollingFileAppender::new(
            Rotation::DAILY,
            dir,
            filename,
        ))),
    }
}

/// Monthly log rotation (`tracing-appender` has no native monthly period).
mod monthly {
    use std::fs::{File, OpenOptions};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use chrono::Utc;

    pub struct Writer {
        inner: Mutex<WriterInner>,
    }

    struct WriterInner {
        dir: PathBuf,
        filename: String,
        period: String,
        file: Option<File>,
    }

    impl Writer {
        pub fn open(dir: &Path, filename: &str) -> io::Result<Self> {
            std::fs::create_dir_all(dir)?;
            Ok(Self {
                inner: Mutex::new(WriterInner {
                    dir: dir.to_path_buf(),
                    filename: filename.to_string(),
                    period: String::new(),
                    file: None,
                }),
            })
        }
    }

    impl Write for Writer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut inner = self.inner.lock().unwrap();
            inner.ensure_current()?;
            inner
                .file
                .as_mut()
                .expect("monthly log file open")
                .write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            let mut inner = self.inner.lock().unwrap();
            inner.ensure_current()?;
            inner
                .file
                .as_mut()
                .expect("monthly log file open")
                .flush()
        }
    }

    impl WriterInner {
        fn ensure_current(&mut self) -> io::Result<()> {
            let period = Utc::now().format("%Y-%m").to_string();
            if self.period == period && self.file.is_some() {
                return Ok(());
            }
            let path = self.path_for(&period);
            self.period = period;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            self.file = Some(file);
            Ok(())
        }

        fn path_for(&self, period: &str) -> PathBuf {
            let path = Path::new(&self.filename);
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("elidune");
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|e| format!(".{e}"))
                .unwrap_or_default();
            self.dir.join(format!("{stem}.{period}{ext}"))
        }
    }
}

/// JSON layer safe for hot-reload: span metadata is omitted because open spans created
/// before a layer swap lack the `FormattedFields` extensions the JSON formatter expects.
fn json_layer<W>(writer: W) -> Box<dyn Layer<FilteredRegistry> + Send + Sync>
where
    W: for<'w> fmt::MakeWriter<'w> + Send + Sync + 'static,
{
    Box::new(
        fmt::layer()
            .json()
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(writer)
            .with_ansi(false),
    )
}

fn build_fmt_layer<W>(format: &str, writer: W) -> Box<dyn Layer<FilteredRegistry> + Send + Sync>
where
    W: for<'w> fmt::MakeWriter<'w> + Send + Sync + 'static,
{
    match format {
        "json" => json_layer(writer),
        "plain" => Box::new(fmt::layer().compact().with_ansi(false).with_writer(writer)),
        _ => Box::new(fmt::layer().with_writer(writer)),
    }
}

fn build_fmt_layer_writer(
    format: &str,
    writer: tracing_appender::non_blocking::NonBlocking,
) -> Box<dyn Layer<FilteredRegistry> + Send + Sync> {
    match format {
        "json" => json_layer(writer),
        "plain" => Box::new(fmt::layer().compact().with_ansi(false).with_writer(writer)),
        _ => Box::new(fmt::layer().with_ansi(false).with_writer(writer)),
    }
}

/// Journald writer compatible with hot-reload (unlike `tracing_journald::Layer`, which panics
/// on spans created before the layer was installed).
mod journald {
    use std::io::Write;

    #[cfg(unix)]
    const JOURNAL_SOCKET: &str = "/run/systemd/journal/socket";

    #[derive(Clone, Copy, Default)]
    pub struct Writer;

    pub fn available() -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::net::UnixDatagram;
            UnixDatagram::unbound()
                .and_then(|sock| sock.connect(JOURNAL_SOCKET).map(|_| sock))
                .is_ok()
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Writer {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            *self
        }
    }

    impl Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            send_line(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn send_line(buf: &[u8]) {
        #[cfg(unix)]
        {
            use std::os::unix::net::UnixDatagram;

            let Ok(sock) = UnixDatagram::unbound() else {
                return;
            };
            if sock.connect(JOURNAL_SOCKET).is_err() {
                return;
            }

            let message = String::from_utf8_lossy(buf).replace('\n', "\\n");
            let payload = format!("MESSAGE={message}\nPRIORITY=6\n");
            let _ = sock.send(payload.as_bytes());
        }

        #[cfg(not(unix))]
        {
            let _ = std::io::stderr().write_all(buf);
        }
    }
}
