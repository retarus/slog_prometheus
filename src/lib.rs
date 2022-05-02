//! Record prometheus metrics for messages logged.
//! 
//! Use the MonitoringDrainBuilder to configure a drain, putting it in an
//! appropriate spot in the slog drain stack.
//! 
//! ```rust
//! use slog::{info, Drain};
//! 
//! let decorator = slog_term::TermDecorator::new().build();
//! let drain = slog_term::FullFormat::new(decorator).build().fuse();
//! let drain = slog_async::Async::new(drain).build();
//! 
//! let drain = slog_prometheus::MonitoringDrainBuilder::new(drain)
//!     .build()
//!     .expect("Failed configuring setting up prometheus")
//!     .fuse();
//! let drain = slog::LevelFilter::new(drain, slog::Level::Info).fuse();
//! 
//! let logger = slog::Logger::root(drain, slog::o!());
//! 
//! info!(logger, "Finished setting up!");
//! 
//! ```
#![warn(missing_docs)]
use prometheus::{
    core::{AtomicF64, GenericCounter},
    Counter, CounterVec, Opts, Registry,
};
use slog::{Drain, Level, LOG_LEVEL_NAMES};
use std::str::FromStr;
use thiserror::Error;

/// During build, prometheus might return an error, which requires this crate
/// to return an error.
/// 
/// Currently, this is the only error returned.
#[derive(Error, Debug)]
pub enum MonitoringDrainError {
    /// A prometheus build error
    #[error(transparent)]
    Prometheus(#[from] prometheus::Error),
}

/// The main monitoring struct, implementing slog::Drain
pub struct MonitoringDrain<D: Drain> {
    core: D,
    log_events: [GenericCounter<AtomicF64>; LEVEL_COUNT],
    log_events_failed: GenericCounter<AtomicF64>,
}

const LEVEL: &str = "level";
const LEVEL_NO: &str = "level_no";
const LEVEL_COUNT: usize = 6;

/// Helper struct to build the MonitoringDrain conveniently
pub struct MonitoringDrainBuilder<'a, 'b, D: Drain> {
    core: D,
    registry: &'b Registry,
    level_field: &'a str,
    level_no_field: &'a str,
}

impl<'a, 'b, D: Drain> MonitoringDrainBuilder<'a, 'b, D> {
    /// At least a drain is required to build this drain
    pub fn new(drain: D) -> Self {
        Self {
            core: drain,
            registry: prometheus::default_registry(),
            level_field: LEVEL,
            level_no_field: LEVEL_NO,
        }
    }

    /// Use a custom registry instead of prometheus::default_registry()
    pub fn registry(mut self, registry: &'b Registry) -> Self {
        self.registry = registry;
        self
    }

    /// Set the name of the prometheus label containing the log level
    pub fn level_field(mut self, level_field: &'a str) -> Self {
        self.level_field = level_field;
        self
    }

    /// Set the name of the prometheus lable containing the log level number
    pub fn level_no_field(mut self, level_no_field: &'a str) -> Self {
        self.level_no_field = level_no_field;
        self
    }

    /// Build the monitoring drain
    pub fn build(self) -> Result<MonitoringDrain<D>, MonitoringDrainError> {
        let opts = Opts::new("log_events", "Log events emitted by this logger.");
        let metrics_builder = CounterVec::new(opts, &[self.level_field, self.level_no_field])?;
        self.registry.register(Box::new(metrics_builder.clone()))?;

        let mut log_events: Vec<GenericCounter<AtomicF64>> = Vec::new();
        for &level_str in LOG_LEVEL_NAMES[1..].iter() {
            let level =
                Level::from_str(level_str).expect("Iterating directly over the sourced array");
            log_events.push(
                metrics_builder.with_label_values(&[level.as_str(), &level.as_usize().to_string()]),
            );
        }

        let level_array: [GenericCounter<AtomicF64>; LEVEL_COUNT] = log_events
            .try_into()
            .expect("Source is built directly via iteration over the source array");

        let opts = Opts::new("log_events_failed", "Log events which failed to be logged.");
        let log_events_failed = Counter::with_opts(opts)?;
        self.registry
            .register(Box::new(log_events_failed.clone()))?;

        Ok(MonitoringDrain {
            core: self.core,
            log_events: level_array,
            log_events_failed,
        })
    }
}

impl<D: Drain> Drain for MonitoringDrain<D> {
    type Ok = D::Ok;

    type Err = D::Err;

    fn log(
        &self,
        record: &slog::Record,
        values: &slog::OwnedKVList,
    ) -> std::result::Result<Self::Ok, Self::Err> {
        let level = record.level();
        let level_no = level.as_usize();

        let metric = &self.log_events[level_no - 1];
        metric.inc();

        let res = self.core.log(record, values);

        if res.is_err() {
            self.log_events_failed.inc();
        }

        res
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use prometheus::Registry;
    use slog::{info, o, Drain, Record, LOG_LEVEL_NAMES};

    use crate::{MonitoringDrainBuilder, LEVEL_COUNT};

    struct StoringDrain<'a> {
        records: &'a AtomicUsize,
    }

    impl<'a> Drain for StoringDrain<'a> {
        type Ok = ();

        type Err = ();

        fn log(
            &self,
            _: &Record,
            _: &slog::OwnedKVList,
        ) -> std::result::Result<Self::Ok, Self::Err> {
            // self.records.
            self.records.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[test]
    fn log_success() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let drain = StoringDrain { records: &COUNTER };

        let registry = Registry::new();
        let drain = MonitoringDrainBuilder::new(drain)
            .registry(&registry)
            .build()
            .expect("No error during default drain creation")
            .fuse();

        let _log = slog::Logger::root(drain, o!());
        info!(_log, "A info message");

        // Ensure the underlying logger is called
        assert_eq!(COUNTER.load(Ordering::Relaxed), 1);

        // Gobble metrics from registry
        let metrics = registry.gather();

        // Check if the above log event was recorded to the registry
        for m in metrics {
            if let "log_events" = m.get_name() {
                assert_eq!(
                    1 as f64,
                    m.get_metric().get(3).unwrap().get_counter().get_value()
                );
            }
            if let "log_events_failed" = m.get_name() {
                assert_eq!(None, m.get_metric().get(3));
            }
        }
    }

    struct FailDrain;

    impl Drain for FailDrain {
        type Ok = ();

        type Err = ();

        fn log(
            &self,
            _: &Record,
            _: &slog::OwnedKVList,
        ) -> std::result::Result<Self::Ok, Self::Err> {
            Err(())
        }
    }

    #[test]
    fn log_failure() {
        let drain = FailDrain {};

        let registry = prometheus::default_registry();
        let drain = MonitoringDrainBuilder::new(drain)
            .registry(&registry)
            .build()
            .expect("No error during default drain creation")
            .ignore_res();

        let _log = slog::Logger::root(drain, o!());
        info!(_log, "A info message");

        // Gobble metrics from registry
        let metrics = registry.gather();

        // Check if the above log event was recorded to the registry
        for m in metrics {
            if let "log_events" = m.get_name() {
                assert_eq!(
                    1 as f64,
                    m.get_metric().get(3).unwrap().get_counter().get_value()
                );
            }
            if let "log_events_failed" = m.get_name() {
                assert_eq!(
                    1 as f64,
                    m.get_metric().get(0).unwrap().get_counter().get_value()
                );
            }
        }
    }

    #[test]
    fn check_same_size() {
        // Ensure these match, otherwise retrieving the level doesn't work
        assert_eq!(LEVEL_COUNT, LOG_LEVEL_NAMES.len() - 1);
    }
}
