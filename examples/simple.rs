use std::net::SocketAddr;

use slog::{info, warn, Drain};

fn main() {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build();

    let drain = slog_prometheus::MonitoringDrainBuilder::new(drain)
        .build()
        .expect("Failed configuring setting up prometheus")
        .fuse();
    let drain = slog::LevelFilter::new(drain, slog::Level::Info).fuse();

    let logger = slog::Logger::root(drain, slog::o!());

    info!(logger, "Finished setting up!");

    // Start exporter
    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let _ = prometheus_exporter::start(addr).expect("Failed binding metrics listener");
    info!(logger, "Started metrics listener");

    warn!(logger, "Logging an important warning");

    let body = reqwest::blocking::get("http://127.0.0.1:8080/metrics")
        .expect("Failed fetching metrics")
        .text()
        .expect("Failed getting request body text");
    println!("{}", body);
}
