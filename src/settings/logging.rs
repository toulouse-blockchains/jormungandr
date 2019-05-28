use slog::{Drain, Logger};
use slog_async::Async;
use std::{convert::TryInto, io, str::FromStr};

#[derive(Debug)]
pub struct LogSettings {
    pub verbosity: slog::Level,
    pub output: LogOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Format of the logger.
pub enum LogOutput {
    Stderr,
    StderrJson,
    #[cfg(unix)]
    Syslog,
    #[cfg(feature = "systemd")]
    Journald,
}

impl FromStr for LogOutput {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &*s.trim().to_lowercase() {
            "stderr" => Ok(LogOutput::Stderr),
            "stderr_json" => Ok(LogOutput::StderrJson),
            #[cfg(unix)]
            "syslog" => Ok(LogOutput::Syslog),
            #[cfg(feature = "systemd")]
            "journald" => Ok(LogOutput::Journald),
            other => Err(format!("unknown format '{}'", other)),
        }
    }
}

impl LogOutput {
    fn try_to_async_drain(self) -> Result<Async, Error> {
        Ok(match self {
            LogOutput::Stderr => {
                let decorator = slog_term::TermDecorator::new().build();
                let drain = slog_term::FullFormat::new(decorator).build().fuse();
                Async::new(drain).build()
            }
            LogOutput::StderrJson => {
                let drain = slog_json::Json::default(std::io::stderr()).fuse();
                Async::new(drain).build()
            }
            #[cfg(unix)]
            LogOutput::Syslog => {
                let drain = slog_syslog::unix_3164(slog_syslog::Facility::LOG_USER)?.fuse();
                Async::new(drain).build()
            }
            #[cfg(feature = "systemd")]
            LogOutput::Journald => {
                let drain = slog_journald::JournaldDrain.fuse();
                Async::new(drain).build()
            }
        })
    }
}

impl LogSettings {
    pub fn try_to_async_drain(&self) -> Result<impl Drain<Ok = (), Err = ()>, Error> {
        let drain = self.output.try_to_async_drain()?.fuse();
        Ok(slog::LevelFilter::new(drain, self.verbosity).fuse())
    }

    pub fn to_logger(&self) -> Result<Logger, Error> {
        let output = TryInto::<Async>::try_into(&self.output)?.fuse();
        let drain = slog::LevelFilter::new(output, self.verbosity).fuse();
        Ok(slog::Logger::root(drain, o!()))
    }
}

custom_error! {pub Error
    SyslogAccessFailed { source: io::Error } = "syslog access failed",
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;

    #[test]
    fn stderr_smoke_test() {
        smoke_test(LogOutput::Stderr)
    }

    #[test]
    fn stderr_json_smoke_test() {
        smoke_test(LogOutput::StderrJson)
    }

    #[cfg(unix)]
    #[test]
    fn syslog_smoke_test() {
        smoke_test(LogOutput::Syslog)
    }

    #[cfg(feature = "systemd")]
    #[test]
    fn journald_smoke_test() {
        smoke_test(LogOutput::Journald)
    }

    fn smoke_test(output: LogOutput) {
        let settings = LogSettings {
            verbosity: slog::Level::Debug,
            output,
        };

        let logger = settings.to_logger().expect("Failed to create logger");
        debug!(logger, "smoke test");
    }
}
