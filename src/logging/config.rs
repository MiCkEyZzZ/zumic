use crate::logging::{filters, formatter, sinks};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

pub fn init_logging() {
    let env_filter = filters::build_filter();

    let formatter_layer = formatter::build_formatter().boxed();
    let console_sink = sinks::console::layer().boxed();
    let (file_sink, _guard) = sinks::file::layer(); // guard сохрани при необходимости
    let file_sink = file_sink.boxed();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(formatter_layer)
        .with(console_sink)
        .with(file_sink)
        .init();
}
