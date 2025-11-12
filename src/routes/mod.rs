mod oauth;

use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
};
use minijinja::{Environment, context, path_loader};
use minijinja_autoreload::AutoReloader;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tower_http::{
    LatencyUnit,
    classify::ServerErrorsFailureClass,
    trace::TraceLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse},
};
use tracing::{Level, Span, error};

use crate::auth::{AuthSession, User};
use axum_login::login_required;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub templates: Arc<AutoReloader>,
}

async fn index(State(state): State<AppState>, auth: AuthSession) -> impl IntoResponse {
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

    let current_user: Option<User> = auth.user.clone();

    let html = match tmpl.render(context! {
        title => "Questboard",
        crate => env!("CARGO_PKG_NAME"),
        version => env!("CARGO_PKG_VERSION"),
        user => current_user,
    }) {
        Ok(s) => s,
        Err(e) => {
            error!(?e, "template render error");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    Html(html).into_response()
}

async fn account(State(state): State<AppState>, auth: AuthSession) -> impl IntoResponse {
    let env = match state.templates.acquire_env() {
        Ok(env) => env,
        Err(e) => {
            error!(?e, "failed to acquire template env");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let tmpl = match env.get_template("account.html") {
        Ok(t) => t,
        Err(e) => {
            error!(?e, "template not found/parse failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let html = match tmpl.render(minijinja::context! {
        title => "Account",
        version => env!("CARGO_PKG_VERSION"),
        user => auth.user.clone(),
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
        .route(
            "/account",
            get(account).route_layer(login_required!(
                crate::auth::DbBackend,
                login_url = "/auth/google/start"
            )),
        )
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
