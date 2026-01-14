mod oauth;
mod quests;
mod ui;

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
use std::{sync::Arc, time::Duration};
use time::{Duration as TimeDuration, OffsetDateTime};
use tower_http::{
    LatencyUnit,
    classify::ServerErrorsFailureClass,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::{Level, Span, error};

use axum_login::login_required;

use crate::{
    auth::{AuthSession, User},
    routes::quests::quests_router,
    services::quest_service,
};

#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub templates: Arc<AutoReloader>,
}

fn effective_streak_display(user: &User, now: OffsetDateTime) -> i32 {
    let today = now.date();
    match user.last_active_date {
        Some(d) if d == today || d == today - TimeDuration::days(1) => user.current_streak,
        _ => 0,
    }
}

async fn index(State(state): State<AppState>, auth: AuthSession) -> impl IntoResponse {
    let current_user: Option<User> = auth.user.clone();

    let now = OffsetDateTime::now_utc();
    let quests = if let Some(ref u) = current_user {
        match quest_service::list_quests_for_user(&state.db_pool, u.id, now).await {
            Ok(list) => Some(list),
            Err(e) => {
                error!(?e, "failed to load quests for user");
                None
            }
        }
    } else {
        None
    };

    let env = match state.templates.acquire_env() {
        Ok(env) => env,
        Err(e) => {
            error!(?e, "failed to aquire template env");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let tmpl = match env.get_template("home.html") {
        Ok(t) => t,
        Err(e) => {
            error!(?e, "template not found or failed to parse");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let level = current_user.as_ref().map(|u| level_info(u.xp_total));
    let now = OffsetDateTime::now_utc();
    let streak_display = current_user
        .as_ref()
        .map(|u| effective_streak_display(u, now))
        .unwrap_or(0);

    let html = match tmpl.render(context! {
        title => "Questboard",
        crate => env!("CARGO_PKG_NAME"),
        version => env!("CARGO_PKG_VERSION"),
        user => current_user,
        quests => quests,
        level => level,
        streak_display => streak_display,
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
        .merge(quests_router()) // JSON API
        .merge(ui::ui_router()) // HTMX UI endpoints
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

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct LevelInfo {
    pub level: i64,
    pub level_start_xp: i64,
    pub next_level_xp: i64,
    pub into_level: i64,
    pub needed_for_next: i64,
}

pub fn level_info(xp_total: i64) -> LevelInfo {
    let level = ((xp_total as f64 / 100.0).sqrt().floor() as i64) + 1;
    let level_start_xp = (level - 1) * (level - 1) * 100;
    let next_level_xp = level * level * 100;

    let into_level = xp_total - level_start_xp;
    let needed_for_next = next_level_xp - level_start_xp;

    LevelInfo {
        level,
        level_start_xp,
        next_level_xp,
        into_level,
        needed_for_next,
    }
}
