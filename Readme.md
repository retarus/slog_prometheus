# slog_prometheus - Logging metrics

### Why?

Loads of times, using the [tracing](https://github.com/tokio-rs/tracing) ecosystem
is a very good option.
But sometimes, I have a small piece of software, which does simple logging and as
I try to be a responsible dev, it should be monitored. So i give it some metrics.

This library tries to add a few simple metrics about what has been logged, making
it easy to have a catch-all alert for things logging errors.

### How?

```rust
let drain = slog_prometheus::MonitoringDrainBuilder::new(drain)
        .build()
        .expect("Failed configuring setting up prometheus")
        .fuse();
```

Feel free to open an issue if you encounter any problems or are interested in features.

