use questboard::run;

use tracing::error;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(true)
        .with_line_number(true)
        .init();

    let db_url =
        "postgres://questboard:questboard@localhost:5432/questboard?sslmode=disable".to_string();

    if let Err(e) = run(db_url).await {
        error!(?e, "server exited with error");
        std::process::exit(1);
    }
}

