use crate::models::{
    Quest,
    QuestCompletion,
    QuestKind,
    RepeatUnit,
    CreateQuestInput,
    UpdateQuestInput,
};
use sqlx::PgPool;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;


pub async fn list_quests_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<Quest>, sqlx::Error> {
    sqlx::query_as!(
        Quest,
        r#"
        SELECT
            id,
            user_id,
            title,
            description,
            kind as "kind: _",
            repeat_unit as "repeat_unit: _",
            repeat_interval,
            anchor_date,
            start_date,
            end_date,
            start_at,
            due_at,
            due_time,
            timezone,
            is_active,
            created_at,
            updated_at
        FROM quests
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
        user_id,
    )
    .fetch_all(pool)
    .await
}

pub async fn get_quest_by_id(
    pool: &PgPool,
    user_id: Uuid,
    quest_id: Uuid,
) -> Result<Option<Quest>, sqlx::Error> {
    sqlx::query_as!(
        Quest,
        r#"
        SELECT
            id,
            user_id,
            title,
            description,
            kind as "kind: _",
            repeat_unit as "repeat_unit: _",
            repeat_interval,
            anchor_date,
            start_date,
            end_date,
            start_at,
            due_at,
            due_time,
            timezone,
            is_active,
            created_at,
            updated_at
        FROM quests
        WHERE id = $1 AND user_id = $2
        "#,
        quest_id,
        user_id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn create_quest(
    pool: &PgPool,
    user_id: Uuid,
    input: CreateQuestInput,
) -> Result<Quest, sqlx::Error> {
    let timezone = input.timezone.unwrap_or_else(|| "UTC".to_string());

    sqlx::query_as!(
        Quest,
        r#"
        INSERT INTO quests (
            user_id,
            title,
            description,
            kind,
            repeat_unit,
            repeat_interval,
            anchor_date,
            start_date,
            end_date,
            start_at,
            due_at,
            due_time,
            timezone
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
        RETURNING
            id,
            user_id,
            title,
            description,
            kind as "kind: _",
            repeat_unit as "repeat_unit: _",
            repeat_interval,
            anchor_date,
            start_date,
            end_date,
            start_at,
            due_at,
            due_time,
            timezone,
            is_active,
            created_at,
            updated_at
        "#,
        user_id,
        input.title,
        input.description,
        input.kind as QuestKind,
        input.repeat_unit as Option<RepeatUnit>,
        input.repeat_interval,
        input.anchor_date,
        input.start_date,
        input.end_date,
        input.start_at,
        input.due_at,
        input.due_time,
        timezone,
    )
    .fetch_one(pool)
    .await
}

pub async fn update_quest(
    pool: &PgPool,
    user_id: Uuid,
    quest_id: Uuid,
    input: UpdateQuestInput,
) -> Result<Option<Quest>, sqlx::Error> {
    let mut quest = match get_quest_by_id(pool, user_id, quest_id).await? {
        Some(q) => q,
        None => return Ok(None),
    };

    if let Some(title) = input.title {
        quest.title = title;
    }
    if let Some(description) = input.description {
        quest.description = description;
    }
    if let Some(kind) = input.kind {
        quest.kind = kind;
    }
    if let Some(repeat_unit) = input.repeat_unit {
        quest.repeat_unit = Some(repeat_unit);
    }
    if let Some(repeat_interval) = input.repeat_interval {
        quest.repeat_interval = Some(repeat_interval);
    }
    if let Some(anchor_date) = input.anchor_date {
        quest.anchor_date = Some(anchor_date);
    }
    if let Some(start_date) = input.start_date {
        quest.start_date = Some(start_date);
    }
    if let Some(end_date) = input.end_date {
        quest.end_date = Some(end_date);
    }
    if let Some(start_at) = input.start_at {
        quest.start_at = Some(start_at);
    }
    if let Some(due_at) = input.due_at {
        quest.due_at = Some(due_at);
    }
    if let Some(due_time) = input.due_time {
        quest.due_time = Some(due_time);
    }
    if let Some(timezone) = input.timezone {
        quest.timezone = timezone;
    }

    let updated = sqlx::query_as!(
        Quest,
        r#"
        UPDATE quests
        SET
            title = $3,
            description = $4,
            kind = $5,
            repeat_unit = $6,
            repeat_interval = $7,
            anchor_date = $8,
            start_date = $9,
            end_date = $10,
            start_at = $11,
            due_at = $12,
            due_time = $13,
            timezone = $14,
            updated_at = now()
        WHERE id = $1 AND user_id = $2
        RETURNING
            id,
            user_id,
            title,
            description,
            kind as "kind: _",
            repeat_unit as "repeat_unit: _",
            repeat_interval,
            anchor_date,
            start_date,
            end_date,
            start_at,
            due_at,
            due_time,
            timezone,
            is_active,
            created_at,
            updated_at
        "#,
        quest.id,
        quest.user_id,
        quest.title,
        quest.description,
        quest.kind as QuestKind,
        quest.repeat_unit as Option<RepeatUnit>,
        quest.repeat_interval,
        quest.anchor_date,
        quest.start_date,
        quest.end_date,
        quest.start_at,
        quest.due_at,
        quest.due_time,
        quest.timezone,
    )
    .fetch_optional(pool)
    .await?;

    Ok(updated)
}

pub async fn delete_quest(
    pool: &PgPool,
    user_id: Uuid,
    quest_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query!(
        r#"
        DELETE FROM quests
        WHERE id = $1 AND user_id = $2
        "#,
        quest_id,
        user_id,
    )
    .execute(pool)
    .await?;

    Ok(res.rows_affected() == 1)
}


pub fn current_period_for_quest(
    quest: &Quest,
    now: OffsetDateTime,
) -> Option<(Date, Date)> {
    match quest.kind {
        QuestKind::Once => {
            let start = quest.start_at?.date();
            let end = quest.due_at?.date();
            if now.date() < start || now.date() > end {
                None
            } else {
                Some((start, end))
            }
        }
        QuestKind::Recurring => {
            let unit = quest.repeat_unit?;
            let interval = quest.repeat_interval.unwrap_or(1).max(1);
            let anchor = quest.anchor_date?;
            let today = now.date();

            if let Some(start_date) = quest.start_date {
                if today < start_date {
                    return None;
                }
            }
            if let Some(end_date) = quest.end_date {
                if today > end_date {
                    return None;
                }
            }

            let days_since_anchor = (today - anchor).whole_days();
            if days_since_anchor < 0 {
                return None;
            }

            let step_days = match unit {
                RepeatUnit::Day => interval,
                RepeatUnit::Week => interval * 7,
            };

            let n = (days_since_anchor / step_days as i64) as i64;
            let period_start = anchor + Duration::days(n * step_days as i64);
            let period_end = period_start + Duration::days(step_days as i64);

            if today < period_start || today >= period_end {
                None
            } else {
                Some((period_start, period_end))
            }
        }
    }
}

pub async fn complete_quest_for_current_period(
    pool: &PgPool,
    quest: &Quest,
    now: OffsetDateTime,
) -> Result<Option<QuestCompletion>, sqlx::Error> {
    let Some((period_start, period_end)) = current_period_for_quest(quest, now) else {
        return Ok(None);
    };

    let completion = sqlx::query_as!(
        QuestCompletion,
        r#"
        INSERT INTO quest_completions (quest_id, period_start, period_end)
        VALUES ($1, $2, $3)
        ON CONFLICT (quest_id, period_start, period_end)
        DO UPDATE SET completed_at = now()
        RETURNING
            id,
            quest_id,
            period_start,
            period_end,
            completed_at
        "#,
        quest.id,
        period_start,
        period_end,
    )
    .fetch_one(pool)
    .await?;

    Ok(Some(completion))
}

