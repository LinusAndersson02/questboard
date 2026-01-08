use axum::{
    extract::{Form, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{ get, post, put},
    Router,
};
use minijinja::context;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    auth::{AuthSession, User},
    models::{CreateQuestInput, QuestKind, UpdateQuestInput},
    routes::{level_info, AppState},
    services::quest_service,
};

use axum_login::login_required;

pub fn ui_router() -> Router<AppState> {
    Router::new()
        .route("/ui/quests/new", get(ui_new_quest_modal))
        .route("/ui/quests/{id}/edit", get(ui_edit_quest_modal))
        .route("/ui/modal/close", get(ui_close_modal))
        .route("/ui/quests", post(ui_create_quest))
        .route("/ui/quests/{id}", put(ui_update_quest).delete(ui_delete_quest))
        .route("/ui/quests/{id}/complete", post(ui_complete_quest))
        .route_layer(login_required!(
            crate::auth::DbBackend,
            login_url = "/auth/google/start"
        ))
}

async fn render_template(
    state: &AppState,
    template_name: &str,
    ctx: minijinja::Value,
) -> Result<String, StatusCode> {
    let env = state
        .templates
        .acquire_env()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let tmpl = env
        .get_template(template_name)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tmpl.render(ctx)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn effective_streak_display(user: &User, now: OffsetDateTime) -> i32 {
    let today = now.date();
    match user.last_active_date {
        Some(d) if d == today || d == today - Duration::days(1) => user.current_streak,
        _ => 0,
    }
}

async fn fetch_user(pool: &sqlx::PgPool, user_id: Uuid) -> Result<User, sqlx::Error> {
    sqlx::query_as!(
        User,
        r#"
        SELECT
            id              AS "id!: Uuid",
            email           AS "email!",
            name,
            avatar_url      AS "avatar?",
            xp_total        AS "xp_total!",
            coins           AS "coins!",
            current_streak  AS "current_streak!",
            longest_streak  AS "longest_streak!",
            last_active_date AS "last_active_date?",
            timezone        AS "timezone!",
            google_sub      AS "session_key!"
        FROM users
        WHERE id = $1
        "#,
        user_id
    )
    .fetch_one(pool)
    .await
}

fn close_modal_oob() -> &'static str {
    r#"<div id="modal-root" hx-swap-oob="innerHTML"></div>"#
}

pub async fn ui_new_quest_modal(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let html = render_template(&state, "partials/create_quest_modal.html", context! {}).await?;
    Ok(Html(html))
}

pub async fn ui_edit_quest_modal(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;
    let q = quest_service::get_quest_by_id(&state.db_pool, user.id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let html = render_template(&state, "partials/edit_quest_modal.html", context! { q => q }).await?;
    Ok(Html(html))
}

pub async fn ui_close_modal() -> impl IntoResponse {
    Html(String::new())
}

#[derive(serde::Deserialize)]
pub struct CreateQuestForm {
    pub title: String,
    pub description: String,
    pub kind: QuestKind,
    pub xp_reward: Option<i32>,
    pub coin_reward: Option<i32>,
}

pub async fn ui_create_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Form(form): Form<CreateQuestForm>,
) -> Result<Html<String>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;

    let input = CreateQuestInput {
        title: form.title,
        description: form.description,
        kind: form.kind,
        xp_reward: form.xp_reward,
        coin_reward: form.coin_reward,

        repeat_unit: None,
        repeat_interval: None,
        anchor_date: None,
        start_date: None,
        end_date: None,
        start_at: None,
        due_at: None,
        due_time: None,
        timezone: None,
    };

    let q = quest_service::create_quest(&state.db_pool, user.id, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row_html = render_template(
        &state,
        "partials/quest_row.html",
        context! { q => q, completed => false },
    )
    .await?;

    Ok(Html(format!("{row_html}\n{}", close_modal_oob())))
}

#[derive(serde::Deserialize)]
pub struct UpdateQuestForm {
    pub title: String,
    pub description: String,
    pub kind: QuestKind,
    pub xp_reward: i32,
    pub coin_reward: i32,
}

pub async fn ui_update_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
    Form(form): Form<UpdateQuestForm>,
) -> Result<Html<String>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;

    let input = UpdateQuestInput {
        title: Some(form.title),
        description: Some(form.description),
        kind: Some(form.kind),
        xp_reward: Some(form.xp_reward),
        coin_reward: Some(form.coin_reward),

        repeat_unit: None,
        repeat_interval: None,
        anchor_date: None,
        start_date: None,
        end_date: None,
        start_at: None,
        due_at: None,
        due_time: None,
        timezone: None,
    };

    let updated = quest_service::update_quest(&state.db_pool, user.id, id, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

   let now = OffsetDateTime::now_utc();
    let completed = if let Some((ps, pe)) = quest_service::current_period_for_quest(&updated, now) {
        sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM quest_completions
                WHERE quest_id = $1 AND period_start = $2 AND period_end = $3
            ) AS "exists!"
            "#,
            updated.id,
            ps,
            pe
        )
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or(false)
    } else {
        false
    };

    let row_html = render_template(
        &state,
        "partials/quest_row.html",
        context! { q => updated, completed => completed },
    )
    .await?;

    Ok(Html(format!("{row_html}\n{}", close_modal_oob())))
}

pub async fn ui_delete_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;

    let deleted = quest_service::delete_quest(&state.db_pool, user.id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    // Remove the row (outerHTML swap target will be replaced with empty)
    Ok(Html(String::new()))
}

pub async fn ui_complete_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;
    let now = OffsetDateTime::now_utc();

    let quest = quest_service::get_quest_by_id(&state.db_pool, user.id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let result = quest_service::complete_quest_and_reward(&state.db_pool, user.id, &quest, now)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let completed = matches!(
        result,
        quest_service::CompleteQuestResult::Completed(_)
            | quest_service::CompleteQuestResult::AlreadyCompleted
    );

    let row_html = render_template(
        &state,
        "partials/quest_row.html",
        context! { q => quest, completed => completed },
    )
    .await?;

    let fresh_user = fetch_user(&state.db_pool, user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let level = level_info(fresh_user.xp_total);
    let streak_display = effective_streak_display(&fresh_user, now);

    let header_html = render_template(
        &state,
        "partials/header_stats.html",
        context! { user => fresh_user, level => level, streak_display => streak_display },
    )
    .await?;

    Ok(Html(format!("{row_html}\n{header_html}")))
}

