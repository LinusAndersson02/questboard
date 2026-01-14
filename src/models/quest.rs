use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use time::{Date, OffsetDateTime, Time};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "quest_kind", rename_all = "lowercase")]
pub enum QuestKind {
    Once,
    Recurring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "repeat_freq", rename_all = "lowercase")]
pub enum RepeatFreq {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, FromRow, Deserialize, Serialize)]
pub struct Quest {
    pub id: Uuid,
    pub user_id: Uuid,

    pub title: String,
    pub description: String,

    pub kind: QuestKind,

    pub xp_reward: i32,
    pub coin_reward: i32,

    pub start_at: Option<OffsetDateTime>,
    pub due_at: Option<OffsetDateTime>,

    pub repeat_freq: Option<RepeatFreq>,
    pub repeat_interval: Option<i32>,
    pub anchor_date: Option<Date>,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,

    pub repeat_weekdays: Option<Vec<i16>>,

    pub repeat_month_day: Option<i16>,
    pub repeat_month_week: Option<i16>,
    pub repeat_month_weekday: Option<i16>,

    // Optional: due time/timezone for “occurrence due time”
    pub due_time: Option<Time>,
    pub timezone: String,

    pub is_active: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

use std::option::Option;

#[derive(Debug, Serialize)]
pub struct QuestWithStatus {
    // Flatten makes `title`, `kind`, etc. appear at top-level in JSON/templates.
    #[serde(flatten)]
    pub quest: Quest,

    // Derived fields
    pub is_due: bool,
    pub is_completed: bool,

    // Useful for UI/debugging and future filtering
    pub period_start: Option<Date>,
    pub period_end: Option<Date>,
}


#[derive(Debug, FromRow, Deserialize, Serialize)]
pub struct QuestCompletion {
    pub id: i64,
    pub quest_id: Uuid,
    pub period_start: Date,
    pub period_end: Date,
    pub xp_reward: i32,
    pub coin_reward: i32,
    pub completed_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateQuestInput {
    pub title: String,
    pub description: String,

    pub kind: QuestKind,
    pub xp_reward: Option<i32>,
    pub coin_reward: Option<i32>,

    // Once-only scheduling
    pub start_at: Option<OffsetDateTime>,
    pub due_at: Option<OffsetDateTime>,

    // Recurring scheduling
    pub repeat_freq: Option<RepeatFreq>,
    pub repeat_interval: Option<i32>,
    pub anchor_date: Option<Date>,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,

    pub repeat_weekdays: Option<Vec<i16>>,
    pub repeat_month_day: Option<i16>,
    pub repeat_month_week: Option<i16>,
    pub repeat_month_weekday: Option<i16>,

    pub due_time: Option<Time>,
    pub timezone: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateQuestInput {
    pub title: Option<String>,
    pub description: Option<String>,

    pub kind: Option<QuestKind>,
    pub xp_reward: Option<i32>,
    pub coin_reward: Option<i32>,

    // Once-only scheduling
    pub start_at: Option<OffsetDateTime>,
    pub due_at: Option<OffsetDateTime>,

    // Recurring scheduling
    pub repeat_freq: Option<RepeatFreq>,
    pub repeat_interval: Option<i32>,
    pub anchor_date: Option<Date>,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,

    pub repeat_weekdays: Option<Vec<i16>>,
    pub repeat_month_day: Option<i16>,
    pub repeat_month_week: Option<i16>,
    pub repeat_month_weekday: Option<i16>,

    pub due_time: Option<Time>,
    pub timezone: Option<String>,
}

