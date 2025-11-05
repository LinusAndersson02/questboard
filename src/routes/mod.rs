use axum::Router;
use axum::routing::get;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
}

pub async fn create_routes(db_pool: PgPool) -> anyhow::Result<axum::Router> {
    let app_state = AppState { db_pool };

    Ok(Router::new()
        .route("/", get(|| async { "hello, world!" }))
        .with_state(app_state))
}
