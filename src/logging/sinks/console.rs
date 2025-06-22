use tracing_subscriber::{fmt::Layer, registry::LookupSpan};

pub fn layer<S>() -> Layer<S>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer().with_writer(std::io::stdout)
}
