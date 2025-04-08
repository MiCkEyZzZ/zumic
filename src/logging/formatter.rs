use tracing_subscriber::fmt::{self, format::FmtSpan, Layer};
use tracing_subscriber::registry::LookupSpan;

pub fn build_formatter<S>() -> Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_thread_names(true)
        .with_thread_ids(true)
}
