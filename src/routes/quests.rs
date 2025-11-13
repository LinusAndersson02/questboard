use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use reqwest::StatusCode;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    auth::AuthSession,
    models::{Quest, QuestCompletion, CreateQuestInput, UpdateQuestInput},
    routes::AppState,
    services::quest_service,
};
use axum_login::login_required;

pub fn quests_router() -> Router<AppState> {
    Router::new()
        .route("/quests", get(list_quests).post(create_quest))
        .route("/quests/:id", get(get_quest).put(update_quest).delete(delete_quest))
        .route("/quests/:id/complete", post(complete_quest))
        .route_layer(login_required!(
                crate::auth::DbBackend,
                login_url = "/auth/google/start"
                ))
}

pub async fn list_quests(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Vec<Quest>>, StatusCode> {
    let user = match auth.user {
        Some(ref u) => u,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let quests = quest_service::list_quests_for_user(&state.db_pool, user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(quests))
}

pub async fn get_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
) -> Result<Json<Quest>, StatusCode> {
    let user = match auth.user {
        Some(ref u) => u,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let quest = quest_service::get_quest_by_id(&state.db_pool, user.id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match quest {
        Some(q) => Ok(Json(q)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn create_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(input): Json<CreateQuestInput>,
) -> Result<(StatusCode, Json<Quest>), StatusCode> {
    let user = match auth.user {
        Some(ref u) => u,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let quest = quest_service::create_quest(&state.db_pool, user.id, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::CREATED, Json(quest)))
}

pub async fn update_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateQuestInput>,
) -> Result<Json<Quest>, StatusCode> {
    let user = match auth.user {
        Some(ref u) => u,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let quest = quest_service::update_quest(&state.db_pool, user.id, id, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match quest {
        Some(q) => Ok(Json(q)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let user = match auth.user {
        Some(ref u) => u,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let deleted = quest_service::delete_quest(&state.db_pool, user.id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn complete_quest(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<Uuid>,
) -> Result<Json<QuestCompletion>, StatusCode> {
    let user = match auth.user {
        Some(ref u) => u,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let quest = quest_service::get_quest_by_id(&state.db_pool, user.id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let quest = match quest {
        Some(q) => q,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let now = OffsetDateTime::now_utc();

    let completion =
        quest_service::complete_quest_for_current_period(&state.db_pool, &quest, now)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match completion {
        Some(c) => Ok(Json(c)),
        None => Err(StatusCode::BAD_REQUEST), // not active in current period
    }
}

