use sqlx::FromRow;
use sqlx::Type;
use time::{Date, OffsetDateTime, Time};
use uuid::Uuid;
use serde::Deserialize;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Type, Deserialize)]
#[sqlx(type_name = "quest_kind", rename_all = "lowercase")]
pub enum QuestKind {
    Once,
    Recurring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type, Deserialize)]
#[sqlx(type_name = "repeat_unit", rename_all = "lowercase")]
pub enum RepeatUnit {
    Day,
    Week,
}

#[derive(Debug, FromRow)]
pub struct Quest {
    pub id: Uuid,
    pub user_id: Uuid,

    pub title: String,
    pub description: String,

    pub kind: QuestKind,

    pub repeat_unit: Option<RepeatUnit>,
    pub repeat_interval: Option<i32>,
    pub anchor_date: Option<Date>,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,

    pub start_at: Option<OffsetDateTime>,
    pub due_at: Option<OffsetDateTime>,

    pub due_time: Option<Time>,
    pub timezone: String,

    pub is_active: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, FromRow)]
pub struct QuestCompletion {
    pub id: i64,
    pub quest_id: Uuid,
    pub period_start: Date,
    pub period_end: Date,
    pub completed_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateQuestInput {
    pub title: String,
    pub description: String,

    pub kind: QuestKind,

    pub repeat_unit: Option<RepeatUnit>,
    pub repeat_interval: Option<i32>,
    pub anchor_date: Option<Date>,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,

    pub start_at: Option<OffsetDateTime>,
    pub due_at: Option<OffsetDateTime>,

    pub due_time: Option<Time>,
    pub timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateQuestInput {
    pub title: Option<String>,
    pub description: Option<String>,

    pub kind: Option<QuestKind>,

    pub repeat_unit: Option<RepeatUnit>,
    pub repeat_interval: Option<i32>,
    pub anchor_date: Option<Date>,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,

    pub start_at: Option<OffsetDateTime>,
    pub due_at: Option<OffsetDateTime>,

    pub due_time: Option<Time>,
    pub timezone: Option<String>,
}
