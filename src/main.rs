use questboard::run;

use tracing::{error};
use tracing_subscriber::{fmt, EnvFilter};



#[tokio::main]
async fn main(){

    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let db_url = "postgres://user:pass@localhost/db".to_string();
    if let Err(e) = run(db_url).await {
        error!(?e, "server exited with error");
        std::process::exit(1);
    }
}
