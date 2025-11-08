mod oauth;

use axum::Router;
use axum::routing::get;
use minijinja_autoreload::AutoReloader;
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::response::IntoResponse;
use minijinja::Environment;
use minijinja::context;
use minijinja::path_loader;
use std::time::Duration;
use tower_http::LatencyUnit;
use tower_http::classify::ServerErrorsFailureClass;
use tower_http::trace::DefaultMakeSpan;
use tower_http::trace::DefaultOnResponse;
use tracing::Level;
use tracing::Span;
use tracing::error;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub templates: Arc<AutoReloader>,
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    let env = match state.templates.acquire_env() {
        Ok(env) => env,
        Err(e) => {
            error!(?e, "failed to aquire template env");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let tmpl = match env.get_template("helloworld.html") {
        Ok(t) => t,
        Err(e) => {
            error!(?e, "template not found or failed to parse");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let html = match tmpl.render(context! {
        title => "Questboard",
        crate => env!("CARGO_PKG_NAME"),
        version => env!("CARGO_PKG_VERSION"),
    }) {
        Ok(s) => s,
        Err(e) => {
            error!(?e, "template render error");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    Html(html).into_response()
}

pub async fn create_routes(db_pool: PgPool) -> anyhow::Result<Router> {
    let reloader = AutoReloader::new(|notifier| {
        let template_path = "templates";
        let mut env = Environment::new();
        env.set_loader(path_loader(template_path));
        notifier.watch_path(template_path, true);
        Ok(env)
    });

    let app_state = AppState {
        db_pool,
        templates: Arc::new(reloader),
    };

    Ok(Router::new()
        .route("/", get(index))
        .route("/auth/google/start", get(oauth::login_start))
        .route("/oauth/google/callback", get(oauth::oauth_callback))
        .route("/logout", get(oauth::logout))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(
                    DefaultMakeSpan::new()
                        .level(Level::INFO)
                        .include_headers(false),
                )
                .on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Millis),
                )
                .on_failure(
                    |class: ServerErrorsFailureClass, latency: Duration, _span: &Span| {
                        tracing::warn!(
                            error.class = %format!("{class:?}"),
                            latency.ms = %latency.as_millis(),
                            "request failed"
                        );
                    },
                ),
        )
        .with_state(app_state))
}
