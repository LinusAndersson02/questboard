use std::time::Duration;

use tokio::net::TcpListener;
use tracing::{error, info};

use sqlx::{Error as SqlxError, postgres::PgPoolOptions};

mod auth;
mod routes;

use axum::Router;

use axum_login::{
    AuthManagerLayerBuilder,
    tower_sessions::{ExpiredDeletion, Expiry, SessionManagerLayer, cookie::SameSite},
};
use time::Duration as TimeDuration;
use tower_sessions_sqlx_store::PostgresStore;

pub async fn run(database_url: String) -> anyhow::Result<()> {
    let db_pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
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

    let store = PostgresStore::new(db_pool.clone());

    store.migrate().await?;
    let _cleanup = {
        let store = store.clone();
        tokio::spawn(store.continuously_delete_expired(tokio::time::Duration::from_secs(60 * 60)))
    };

    let session_layer = SessionManagerLayer::new(store)
    .with_name(if cfg!(debug_assertions) { "questboard_session" } else { "__Host-questboard" })
    .with_http_only(true)
    .with_secure(!cfg!(debug_assertions)) 
    .with_same_site(SameSite::Lax)
    .with_expiry(Expiry::OnInactivity(TimeDuration::hours(24)));

    
    let backend = auth::DbBackend {
        pool: db_pool.clone(),
    };
    let auth_layer = AuthManagerLayerBuilder::new(backend, session_layer).build();

    let app: Router = routes::create_routes(db_pool)
        .await
        .inspect_err(|e| error!(?e, "building router failed"))?
        .layer(auth_layer);

    let bind_addr = "0.0.0.0:3000";
    let listener = TcpListener::bind(bind_addr)
        .await
        .inspect_err(|e| error!(?e, "building tcplistener"))?;

    info!("server is now running at {:?}", bind_addr);

    axum::serve(listener, app)
        .await
        .inspect_err(|e| error!(?e, "server error"))?;

    Ok(())
}
