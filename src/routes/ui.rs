use axum::{
    Router,
    extract::{Form, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post, put},
};
use minijinja::context;
use time::{Date, Duration, OffsetDateTime, PrimitiveDateTime, Time};
use uuid::Uuid;

use crate::{
    auth::{AuthSession, User},
    models::{CreateQuestInput, QuestKind, RepeatFreq, UpdateQuestInput},
    routes::{AppState, level_info},
    services::quest_service,
};

use axum_login::login_required;

pub fn ui_router() -> Router<AppState> {
    Router::new()
        .route("/ui/quests/new", get(ui_new_quest_modal))
        .route("/ui/quests/{id}/edit", get(ui_edit_quest_modal))
        .route("/ui/modal/close", get(ui_close_modal))
        .route("/ui/quests", post(ui_create_quest))
        .route("/ui/quests/list", get(ui_list_quests))
        .route(
            "/ui/quests/{id}",
            put(ui_update_quest).delete(ui_delete_quest),
        )
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

    let html = render_template(
        &state,
        "partials/edit_quest_modal.html",
        context! { q => q },
    )
    .await?;
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

    // Once (date inputs)
    pub once_start_date: Option<String>,
    pub once_due_date: Option<String>,

    // Recurring
    pub repeat_freq: Option<RepeatFreq>,
    pub repeat_interval: Option<i32>,
    pub anchor_date: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,

    // Weekly
    pub repeat_weekdays: Option<Vec<i16>>,

    // Monthly
    pub month_rule: Option<String>, // "dom" | "nth"
    pub repeat_month_day: Option<i16>,
    pub repeat_month_week: Option<i16>,
    pub repeat_month_weekday: Option<i16>,
}

pub async fn ui_create_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Form(form): Form<CreateQuestForm>,
) -> Result<Html<String>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;
    let now = OffsetDateTime::now_utc();

    // once dates -> start_at/due_at
    let (start_at, due_at) = if form.kind == QuestKind::Once {
        let sd = parse_date_opt(&form.once_start_date)?;
        let dd = parse_date_opt(&form.once_due_date)?;
        (sd.map(start_of_day_utc), dd.map(end_of_day_utc))
    } else {
        (None, None)
    };

    // recurring dates
    let anchor_date = parse_date_opt(&form.anchor_date)?;
    let start_date = parse_date_opt(&form.start_date)?;
    let end_date = parse_date_opt(&form.end_date)?;

    // monthly rule sanitize
    let (repeat_month_day, repeat_month_week, repeat_month_weekday) =
        match form.month_rule.as_deref() {
            Some("nth") => (None, form.repeat_month_week, form.repeat_month_weekday),
            _ => (form.repeat_month_day, None, None),
        };

    let input = CreateQuestInput {
        title: form.title,
        description: form.description,
        kind: form.kind,
        xp_reward: form.xp_reward,
        coin_reward: form.coin_reward,

        start_at,
        due_at,

        repeat_freq: form.repeat_freq,
        repeat_interval: form.repeat_interval,
        anchor_date,
        start_date,
        end_date,

        repeat_weekdays: form.repeat_weekdays,

        repeat_month_day,
        repeat_month_week,
        repeat_month_weekday,

        due_time: None,
        timezone: None,
    };

    let created = quest_service::create_quest(&state.db_pool, user.id, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item = quest_service::get_quest_by_id_with_status(&state.db_pool, user.id, created.id, now)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let row_html = render_template(
        &state,
        "partials/quest_row.html",
        context! {
            q => item.quest,
            due => item.is_due,
            completed => item.is_completed
        },
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

    pub once_start_date: Option<String>,
    pub once_due_date: Option<String>,

    pub repeat_freq: Option<RepeatFreq>,
    pub repeat_interval: Option<i32>,
    pub anchor_date: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,

    pub repeat_weekdays: Option<Vec<i16>>,

    pub month_rule: Option<String>,
    pub repeat_month_day: Option<i16>,
    pub repeat_month_week: Option<i16>,
    pub repeat_month_weekday: Option<i16>,
}

pub async fn ui_update_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
    Form(form): Form<UpdateQuestForm>,
) -> Result<Html<String>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;
    let now = OffsetDateTime::now_utc();

    let (start_at, due_at) = if form.kind == QuestKind::Once {
        let sd = parse_date_opt(&form.once_start_date)?;
        let dd = parse_date_opt(&form.once_due_date)?;
        (sd.map(start_of_day_utc), dd.map(end_of_day_utc))
    } else {
        (None, None)
    };

    let anchor_date = parse_date_opt(&form.anchor_date)?;
    let start_date = parse_date_opt(&form.start_date)?;
    let end_date = parse_date_opt(&form.end_date)?;

    let (repeat_month_day, repeat_month_week, repeat_month_weekday) =
        match form.month_rule.as_deref() {
            Some("nth") => (None, form.repeat_month_week, form.repeat_month_weekday),
            _ => (form.repeat_month_day, None, None),
        };

    let input = UpdateQuestInput {
        title: Some(form.title),
        description: Some(form.description),
        kind: Some(form.kind),
        xp_reward: Some(form.xp_reward),
        coin_reward: Some(form.coin_reward),

        start_at,
        due_at,

        repeat_freq: form.repeat_freq,
        repeat_interval: form.repeat_interval,
        anchor_date,
        start_date,
        end_date,

        repeat_weekdays: form.repeat_weekdays,

        repeat_month_day,
        repeat_month_week,
        repeat_month_weekday,

        due_time: None,
        timezone: None,
    };

    let updated = quest_service::update_quest(&state.db_pool, user.id, id, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let item = quest_service::get_quest_by_id_with_status(&state.db_pool, user.id, updated.id, now)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let row_html = render_template(
        &state,
        "partials/quest_row.html",
        context! {
            q => item.quest,
            due => item.is_due,
            completed => item.is_completed
        },
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

#[derive(serde::Deserialize)]
pub struct QuestListQuery {
    pub filter: Option<String>,
}

pub async fn ui_list_quests(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(q): Query<QuestListQuery>,
) -> Result<Html<String>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;
    let now = OffsetDateTime::now_utc();

    let mut quests = quest_service::list_quests_for_user(&state.db_pool, user.id, now)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Filter
    let filter = q.filter.unwrap_or_else(|| "all".to_string());
    quests = apply_filter(quests, &filter, now);

    // Sort: due & not completed first
    quests.sort_by_key(|it| {
        let due_open = it.is_due && !it.is_completed;
        (!due_open, it.is_completed, it.quest.created_at)
    });

    let html = render_template(
        &state,
        "partials/quests_list.html",
        context! { quests => quests },
    )
    .await?;

    Ok(Html(html))
}

fn apply_filter(
    quests: Vec<crate::models::QuestWithStatus>,
    filter: &str,
    now: OffsetDateTime,
) -> Vec<crate::models::QuestWithStatus> {
    let today = now.date();

    match filter {
        "today" => quests.into_iter().filter(|q| q.is_due).collect(),

        // "week" / "month" are “due at least once within range”
        "week" => quests
            .into_iter()
            .filter(|q| is_due_in_range(&q.quest, today, today + Duration::days(6)))
            .collect(),

        "month" => quests
            .into_iter()
            .filter(|q| is_due_in_range(&q.quest, today, today + Duration::days(30)))
            .collect(),

        "once" => quests
            .into_iter()
            .filter(|q| q.quest.kind == QuestKind::Once)
            .collect(),

        "recurring" => quests
            .into_iter()
            .filter(|q| q.quest.kind == QuestKind::Recurring)
            .collect(),

        _ => quests,
    }
}

// brute-force range check (fine for small ranges + personal app)
// checks “is there any day in [start..=end] where current_period_for_quest is Some”
fn is_due_in_range(quest: &crate::models::Quest, start: Date, end: Date) -> bool {
    let mut d = start;
    while d <= end {
        // pretend “now” is noon UTC for that day
        let noon = PrimitiveDateTime::new(d, Time::from_hms(12, 0, 0).unwrap()).assume_utc();
        if quest_service::current_period_for_quest(quest, noon).is_some() {
            return true;
        }
        d = d + Duration::days(1);
    }
    false
}

fn parse_date_opt(s: &Option<String>) -> Result<Option<Date>, StatusCode> {
    let Some(raw) = s.as_ref() else {
        return Ok(None);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_date(raw)?))
}

fn parse_date(s: &str) -> Result<Date, StatusCode> {
    // YYYY-MM-DD
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let y: i32 = parts[0].parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let m: u8 = parts[1].parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let d: u8 = parts[2].parse().map_err(|_| StatusCode::BAD_REQUEST)?;

    let month = time::Month::try_from(m).map_err(|_| StatusCode::BAD_REQUEST)?;
    Date::from_calendar_date(y, month, d).map_err(|_| StatusCode::BAD_REQUEST)
}

fn start_of_day_utc(d: Date) -> OffsetDateTime {
    PrimitiveDateTime::new(d, Time::from_hms(0, 0, 0).unwrap()).assume_utc()
}

fn end_of_day_utc(d: Date) -> OffsetDateTime {
    PrimitiveDateTime::new(d, Time::from_hms(23, 59, 59).unwrap()).assume_utc()
}
