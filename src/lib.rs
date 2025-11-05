use tokio::net::TcpListener;
use sqlx::{postgres::PgPoolOptions};
use tracing::error;
use sqlx::Error as SqlxError;
mod routes;
pub async fn run(database_url: String) -> anyhow::Result<()> {
   

    let db_pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .map_err(|e| {
            match &e {
                SqlxError::PoolTimedOut => error!("DB pool timed out obtaining a connection"),
                SqlxError::Configuration(_) => error!("Invalid DB config"),
                _ => error!(?e, "DB connect error"),
            }
            e
        })?;


    let app = routes::create_routes(db_pool)
        .await
        .inspect_err(|e| error!(?e, "building router failed"))?;

    let listener = TcpListener::bind("0.0.0.0:3000")
        .await.inspect_err(|e| error!(?e, "building tcplistener"))?;

    axum::serve(listener, app)
        .await
        .inspect_err(|e| error!(?e, "server error"))?;

    Ok(())
}

