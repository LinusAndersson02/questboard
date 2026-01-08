use crate::models::{
    CreateQuestInput, Quest, QuestCompletion, QuestKind, RepeatUnit, UpdateQuestInput,
};
use sqlx::{PgPool, Postgres, Transaction};
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug, serde::Serialize)]
pub struct CompleteQuestOutcome {
    pub completion: QuestCompletion,

    pub xp_gained: i32,
    pub coins_gained: i32,
    pub streak_bonus_coins: i32,

    pub new_xp_total: i64,
    pub new_coins: i64,
    pub current_streak: i32,
    pub longest_streak: i32,
}

#[derive(Debug)]
pub enum CompleteQuestResult {
    NotActive,
    AlreadyCompleted,
    Completed(CompleteQuestOutcome),
}

#[derive(Debug, sqlx::FromRow)]
pub struct QuestWithStatus {
    #[sqlx(flatten)]
    pub quest: Quest,
    pub is_completed: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct UserStatsRow {
    xp_total: i64,
    coins: i64,
    current_streak: i32,
    longest_streak: i32,
    last_active_date: Option<Date>,
}
pub async fn list_quests_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<Quest>, sqlx::Error> {
    sqlx::query_as!(
        Quest,
        r#"
        SELECT
            id,
            user_id,
            title,
            description,
            kind as "kind: _",
            xp_reward,
            coin_reward,
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


pub async fn list_quests_for_user_with_status(
    pool: &PgPool,
    user_id: Uuid,
    now: OffsetDateTime,
) -> Result<Vec<(Quest, bool)>, sqlx::Error> {
    let quests = list_quests_for_user(pool, user_id).await?;

    let mut out = Vec::with_capacity(quests.len());
    for q in quests {
        let completed = if let Some((period_start, period_end)) = current_period_for_quest(&q, now) {
            let exists = sqlx::query_scalar!(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM quest_completions
                    WHERE quest_id = $1
                      AND period_start = $2
                      AND period_end = $3
                ) AS "exists!"
                "#,
                q.id,
                period_start,
                period_end
            )
            .fetch_one(pool)
            .await?;
            exists
        } else {
            false
        };

        out.push((q, completed));
    }

    Ok(out)
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
            xp_reward,
            coin_reward,
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

    let xp_reward = input.xp_reward.unwrap_or(10);
    let coin_reward = input.coin_reward.unwrap_or(1);

    let now = OffsetDateTime::now_utc();
    let today = now.date();

    // Start with whatever the client sent
    let mut kind = input.kind;
    let mut repeat_unit = input.repeat_unit;
    let mut repeat_interval = input.repeat_interval;
    let mut anchor_date = input.anchor_date;
    let mut start_date = input.start_date;
    let mut end_date = input.end_date;

    let mut start_at = input.start_at;
    let mut due_at = input.due_at;

    match kind {
        QuestKind::Once => {
            match (start_at, due_at) {
                (None, None) => {
                    start_at = Some(now);
                    due_at = Some(now + Duration::days(7));
                }
                (Some(s), None) => {
                    start_at = Some(s);
                    due_at = Some(s + Duration::days(7));
                }
                (None, Some(d)) => {
                    let s = d - Duration::days(7);
                    start_at = Some(s);
                    due_at = Some(d);
                }
                (Some(s), Some(d)) => {
                    start_at = Some(s);
                    due_at = Some(d);
                }
            }

            if let (Some(s), Some(d)) = (start_at, due_at) {
                if d < s {
                    due_at = Some(s + Duration::days(7));
                }
            }

            repeat_unit = None;
            repeat_interval = None;
            anchor_date = None;
            start_date = None;
            end_date = None;
        }

        QuestKind::Recurring => {
            if repeat_unit.is_none() {
                repeat_unit = Some(RepeatUnit::Day);
            }

            let interval = repeat_interval.unwrap_or(1).max(1);
            repeat_interval = Some(interval);

            let mut anchor = anchor_date.unwrap_or_else(|| start_date.unwrap_or(today));

            if anchor > today {
                anchor = today;
            }
            anchor_date = Some(anchor);

            if start_date.is_none() {
                start_date = Some(anchor);
            }

            start_at = None;
            due_at = None;
        }
    }

    sqlx::query_as!(
        Quest,
        r#"
        INSERT INTO quests (
            user_id,
            title,
            description,
            kind,
            xp_reward,
            coin_reward,
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
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
        RETURNING
            id,
            user_id,
            title,
            description,
            kind as "kind: _",
            xp_reward,
            coin_reward,
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
        kind as QuestKind,
        xp_reward,
        coin_reward,
        repeat_unit as Option<RepeatUnit>,
        repeat_interval,
        anchor_date,
        start_date,
        end_date,
        start_at,
        due_at,
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
    if let Some(xp_reward) = input.xp_reward {
        quest.xp_reward = xp_reward;
    }
    if let Some(coin_reward) = input.coin_reward {
        quest.coin_reward = coin_reward;
    }

    let updated = sqlx::query_as!(
        Quest,
        r#"
    UPDATE quests
    SET
        title = $3,
        description = $4,
        kind = $5,
        xp_reward = $6,
        coin_reward = $7,
        repeat_unit = $8,
        repeat_interval = $9,
        anchor_date = $10,
        start_date = $11,
        end_date = $12,
        start_at = $13,
        due_at = $14,
        due_time = $15,
        timezone = $16,
        updated_at = now()
    WHERE id = $1 AND user_id = $2
    RETURNING
        id,
        user_id,
        title,
        description,
        kind as "kind: _",
        xp_reward,
        coin_reward,
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
        quest.xp_reward,
        quest.coin_reward,
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

pub fn current_period_for_quest(quest: &Quest, now: OffsetDateTime) -> Option<(Date, Date)> {
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

            let n = days_since_anchor / step_days as i64;
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

async fn insert_completion_for_period(
    tx: &mut Transaction<'_, Postgres>,
    quest: &Quest,
    period_start: Date,
    period_end: Date,
) -> Result<Option<QuestCompletion>, sqlx::Error> {
    sqlx::query_as!(
        QuestCompletion,
        r#"
        INSERT INTO quest_completions (
            quest_id, period_start, period_end, xp_reward, coin_reward
        )
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (quest_id, period_start, period_end)
        DO NOTHING
        RETURNING
            id,
            quest_id,
            period_start,
            period_end,
            xp_reward,
            coin_reward,
            completed_at
        "#,
        quest.id,
        period_start,
        period_end,
        quest.xp_reward,
        quest.coin_reward,
    )
    .fetch_optional(&mut **tx)
    .await
}

pub async fn complete_quest_and_reward(
    pool: &PgPool,
    user_id: Uuid,
    quest: &Quest,
    now: OffsetDateTime,
) -> Result<CompleteQuestResult, sqlx::Error> {
    let Some((period_start, period_end)) = current_period_for_quest(quest, now) else {
        return Ok(CompleteQuestResult::NotActive);
    };

    let mut tx: Transaction<'_, Postgres> = pool.begin().await?;

    let inserted = insert_completion_for_period(&mut tx, quest, period_start, period_end).await?;
    let Some(completion) = inserted else {
        tx.commit().await?;
        return Ok(CompleteQuestResult::AlreadyCompleted);
    };

    let mut user_stats = sqlx::query_as!(
        UserStatsRow,
        r#"
        SELECT
            xp_total,
            coins,
            current_streak,
            longest_streak,
            last_active_date
        FROM users
        WHERE id = $1
        FOR UPDATE
        "#,
        user_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // NOTE: timezone handling: for now streak is computed in UTC.
    let today = now.date();

    let mut streak_bonus_coins = 0;

    let new_current_streak = match user_stats.last_active_date {
        None => {
            streak_bonus_coins = 1;
            1
        }
        Some(last) if last == today => 0,
        Some(last) if last == today - Duration::days(1) => {
            streak_bonus_coins = 1;
            user_stats.current_streak + 1
        }
        Some(_) => {
            streak_bonus_coins = 1;
            1
        }
    };

    if new_current_streak != 0 {
        user_stats.current_streak = new_current_streak;
        if user_stats.current_streak > user_stats.longest_streak {
            user_stats.longest_streak = user_stats.current_streak;
        }
        user_stats.last_active_date = Some(today);
    }

    let xp_gained = quest.xp_reward;
    let coins_gained = quest.coin_reward + streak_bonus_coins;

    user_stats.xp_total += xp_gained as i64;
    user_stats.coins += coins_gained as i64;

    let updated = sqlx::query!(
        r#"
        UPDATE users
        SET
            xp_total = $2,
            coins = $3,
            current_streak = $4,
            longest_streak = $5,
            last_active_date = $6,
            updated_at = now()
        WHERE id = $1
        RETURNING
            xp_total,
            coins,
            current_streak,
            longest_streak
        "#,
        user_id,
        user_stats.xp_total,
        user_stats.coins,
        user_stats.current_streak,
        user_stats.longest_streak,
        user_stats.last_active_date,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(CompleteQuestResult::Completed(CompleteQuestOutcome {
        completion,
        xp_gained,
        coins_gained,
        streak_bonus_coins,
        new_xp_total: updated.xp_total,
        new_coins: updated.coins,
        current_streak: updated.current_streak,
        longest_streak: updated.longest_streak,
    }))
}
