use crate::models::{
    CreateQuestInput, Quest, QuestCompletion, QuestKind, QuestWithStatus, RepeatFreq,
    UpdateQuestInput,
};

use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::HashSet;
use time::{Date, Duration, Month, OffsetDateTime};
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
struct UserStatsRow {
    xp_total: i64,
    coins: i64,
    current_streak: i32,
    longest_streak: i32,
    last_active_date: Option<Date>,
}

// -------------------------
// Public queries
// -------------------------

pub async fn fetch_quests_for_user(
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

            xp_reward,
            coin_reward,

            start_at,
            due_at,

            repeat_freq as "repeat_freq: _",
            repeat_interval,
            anchor_date,
            start_date,
            end_date,

            repeat_weekdays,
            repeat_month_day,
            repeat_month_week,
            repeat_month_weekday,

            due_time,
            timezone,

            is_active,
            created_at,
            updated_at
        FROM quests
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
        user_id
    )
    .fetch_all(pool)
    .await
}

/// ✅ The ONE list you should use everywhere (API + UI): includes status.
pub async fn list_quests_for_user(
    pool: &PgPool,
    user_id: Uuid,
    now: OffsetDateTime,
) -> Result<Vec<QuestWithStatus>, sqlx::Error> {
    let quests = fetch_quests_for_user(pool, user_id).await?;
    add_status_bulk(pool, quests, now).await
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

            start_at,
            due_at,

            repeat_freq as "repeat_freq: _",
            repeat_interval,
            anchor_date,
            start_date,
            end_date,

            repeat_weekdays,
            repeat_month_day,
            repeat_month_week,
            repeat_month_weekday,

            due_time,
            timezone,

            is_active,
            created_at,
            updated_at
        FROM quests
        WHERE id = $1 AND user_id = $2
        "#,
        quest_id,
        user_id
    )
    .fetch_optional(pool)
    .await
}

pub async fn get_quest_by_id_with_status(
    pool: &PgPool,
    user_id: Uuid,
    quest_id: Uuid,
    now: OffsetDateTime,
) -> Result<Option<QuestWithStatus>, sqlx::Error> {
    let q = get_quest_by_id(pool, user_id, quest_id).await?;
    let Some(q) = q else {
        return Ok(None);
    };

    let p = current_period_for_quest(&q, now);
    let (is_due, is_completed, period_start, period_end) = if let Some((ps, pe)) = p {
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM quest_completions
                WHERE quest_id = $1 AND period_start = $2 AND period_end = $3
            ) AS "exists!"
            "#,
            q.id,
            ps,
            pe
        )
        .fetch_one(pool)
        .await?;

        (true, exists, Some(ps), Some(pe))
    } else {
        (false, false, None, None)
    };

    Ok(Some(QuestWithStatus {
        quest: q,
        is_due,
        is_completed,
        period_start,
        period_end,
    }))
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

    // Start with client-provided fields
    let mut kind = input.kind;

    let mut start_at = input.start_at;
    let mut due_at = input.due_at;

    let mut repeat_freq = input.repeat_freq;
    let mut repeat_interval = input.repeat_interval;
    let mut anchor_date = input.anchor_date;
    let mut start_date = input.start_date;
    let mut end_date = input.end_date;

    let mut repeat_weekdays = input.repeat_weekdays;
    let mut repeat_month_day = input.repeat_month_day;
    let mut repeat_month_week = input.repeat_month_week;
    let mut repeat_month_weekday = input.repeat_month_weekday;

    normalize_quest(
        &mut kind,
        &mut start_at,
        &mut due_at,
        &mut repeat_freq,
        &mut repeat_interval,
        &mut anchor_date,
        &mut start_date,
        &mut end_date,
        &mut repeat_weekdays,
        &mut repeat_month_day,
        &mut repeat_month_week,
        &mut repeat_month_weekday,
        now,
        today,
    );

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

            start_at,
            due_at,

            repeat_freq,
            repeat_interval,
            anchor_date,
            start_date,
            end_date,

            repeat_weekdays,
            repeat_month_day,
            repeat_month_week,
            repeat_month_weekday,

            due_time,
            timezone
        )
        VALUES (
            $1,$2,$3,$4,
            $5,$6,
            $7,$8,
            $9,$10,$11,$12,$13,
            $14,$15,$16,$17,
            $18,$19
        )
        RETURNING
            id,
            user_id,
            title,
            description,
            kind as "kind: _",

            xp_reward,
            coin_reward,

            start_at,
            due_at,

            repeat_freq as "repeat_freq: _",
            repeat_interval,
            anchor_date,
            start_date,
            end_date,

            repeat_weekdays,
            repeat_month_day,
            repeat_month_week,
            repeat_month_weekday,

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
        start_at,
        due_at,
        repeat_freq as Option<RepeatFreq>,
        repeat_interval,
        anchor_date,
        start_date,
        end_date,
        repeat_weekdays.as_deref(),
        repeat_month_day,
        repeat_month_week,
        repeat_month_weekday,
        input.due_time,
        timezone
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
    if let Some(xp_reward) = input.xp_reward {
        quest.xp_reward = xp_reward;
    }
    if let Some(coin_reward) = input.coin_reward {
        quest.coin_reward = coin_reward;
    }

    if let Some(start_at) = input.start_at {
        quest.start_at = Some(start_at);
    }
    if let Some(due_at) = input.due_at {
        quest.due_at = Some(due_at);
    }

    if let Some(repeat_freq) = input.repeat_freq {
        quest.repeat_freq = Some(repeat_freq);
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

    if let Some(repeat_weekdays) = input.repeat_weekdays {
        quest.repeat_weekdays = Some(repeat_weekdays);
    }
    if let Some(repeat_month_day) = input.repeat_month_day {
        quest.repeat_month_day = Some(repeat_month_day);
    }
    if let Some(repeat_month_week) = input.repeat_month_week {
        quest.repeat_month_week = Some(repeat_month_week);
    }
    if let Some(repeat_month_weekday) = input.repeat_month_weekday {
        quest.repeat_month_weekday = Some(repeat_month_weekday);
    }

    if let Some(due_time) = input.due_time {
        quest.due_time = Some(due_time);
    }
    if let Some(timezone) = input.timezone {
        quest.timezone = timezone;
    }

    // Normalize to satisfy constraints / keep data consistent
    let now = OffsetDateTime::now_utc();
    let today = now.date();

    let mut kind = quest.kind;
    let mut start_at = quest.start_at;
    let mut due_at = quest.due_at;

    let mut repeat_freq = quest.repeat_freq;
    let mut repeat_interval = quest.repeat_interval;
    let mut anchor_date = quest.anchor_date;
    let mut start_date = quest.start_date;
    let mut end_date = quest.end_date;

    let mut repeat_weekdays = quest.repeat_weekdays;
    let mut repeat_month_day = quest.repeat_month_day;
    let mut repeat_month_week = quest.repeat_month_week;
    let mut repeat_month_weekday = quest.repeat_month_weekday;

    normalize_quest(
        &mut kind,
        &mut start_at,
        &mut due_at,
        &mut repeat_freq,
        &mut repeat_interval,
        &mut anchor_date,
        &mut start_date,
        &mut end_date,
        &mut repeat_weekdays,
        &mut repeat_month_day,
        &mut repeat_month_week,
        &mut repeat_month_weekday,
        now,
        today,
    );

    // Write normalized values back into quest (so SQL uses them)
    quest.kind = kind;
    quest.start_at = start_at;
    quest.due_at = due_at;
    quest.repeat_freq = repeat_freq;
    quest.repeat_interval = repeat_interval;
    quest.anchor_date = anchor_date;
    quest.start_date = start_date;
    quest.end_date = end_date;
    quest.repeat_weekdays = repeat_weekdays;
    quest.repeat_month_day = repeat_month_day;
    quest.repeat_month_week = repeat_month_week;
    quest.repeat_month_weekday = repeat_month_weekday;

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

            start_at = $8,
            due_at = $9,

            repeat_freq = $10,
            repeat_interval = $11,
            anchor_date = $12,
            start_date = $13,
            end_date = $14,

            repeat_weekdays = $15,
            repeat_month_day = $16,
            repeat_month_week = $17,
            repeat_month_weekday = $18,

            due_time = $19,
            timezone = $20,

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

            start_at,
            due_at,

            repeat_freq as "repeat_freq: _",
            repeat_interval,
            anchor_date,
            start_date,
            end_date,

            repeat_weekdays,
            repeat_month_day,
            repeat_month_week,
            repeat_month_weekday,

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
        quest.start_at,
        quest.due_at,
        quest.repeat_freq as Option<RepeatFreq>,
        quest.repeat_interval,
        quest.anchor_date,
        quest.start_date,
        quest.end_date,
        quest.repeat_weekdays.as_deref(),
        quest.repeat_month_day,
        quest.repeat_month_week,
        quest.repeat_month_weekday,
        quest.due_time,
        quest.timezone
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
        user_id
    )
    .execute(pool)
    .await?;

    Ok(res.rows_affected() == 1)
}

// -------------------------
// Scheduling: current period
// -------------------------

pub fn current_period_for_quest(quest: &Quest, now: OffsetDateTime) -> Option<(Date, Date)> {
    if !quest.is_active {
        return None;
    }

    let today = now.date();

    match quest.kind {
        QuestKind::Once => {
            let start = quest.start_at?.date();
            let due = quest.due_at?.date();

            // Active in [start..=due]
            if today < start || today > due {
                return None;
            }

            // Period spans whole window so it can only be completed once.
            let period_start = start;
            let period_end = due + Duration::days(1);
            Some((period_start, period_end))
        }

        QuestKind::Recurring => {
            let freq = quest.repeat_freq?;
            let interval = quest.repeat_interval.unwrap_or(1).max(1);
            let anchor = quest.anchor_date?;

            if let Some(sd) = quest.start_date {
                if today < sd {
                    return None;
                }
            }
            if let Some(ed) = quest.end_date {
                if today > ed {
                    return None;
                }
            }

            match freq {
                RepeatFreq::Daily => {
                    let days_since = (today - anchor).whole_days();
                    if days_since < 0 {
                        return None;
                    }
                    if (days_since % interval as i64) != 0 {
                        return None;
                    }

                    Some((today, today + Duration::days(1)))
                }

                RepeatFreq::Weekly => {
                    let weekdays = quest.repeat_weekdays.as_ref()?;
                    if weekdays.is_empty() {
                        return None;
                    }

                    let today_week_start = start_of_week_monday(today);
                    let anchor_week_start = start_of_week_monday(anchor);

                    let weeks_since = (today_week_start - anchor_week_start).whole_days() / 7;
                    if weeks_since < 0 {
                        return None;
                    }
                    if (weeks_since % interval as i64) != 0 {
                        return None;
                    }

                    let isodow = iso_weekday(today);
                    if !weekdays.iter().any(|d| *d as i16 == isodow) {
                        return None;
                    }

                    Some((today, today + Duration::days(1)))
                }

                RepeatFreq::Monthly => {
                    // month-based interval
                    let m_today = month_index(today);
                    let m_anchor = month_index(anchor);
                    let months_since = m_today - m_anchor;
                    if months_since < 0 {
                        return None;
                    }
                    if (months_since % interval as i32) != 0 {
                        return None;
                    }

                    let y = today.year();
                    let m = today.month();

                    // Rule A: day-of-month
                    if let Some(day) = quest.repeat_month_day {
                        let dim = days_in_month(y, m) as i16;
                        let target_day = day.clamp(1, 31).min(dim);
                        if today.day() as i16 != target_day {
                            return None;
                        }
                        return Some((today, today + Duration::days(1)));
                    }

                    // Rule B: nth weekday (week=5 => last)
                    if let (Some(week), Some(weekday)) =
                        (quest.repeat_month_week, quest.repeat_month_weekday)
                    {
                        let target = nth_weekday_of_month(y, m, weekday as i16, week as i16)?;
                        if target != today {
                            return None;
                        }
                        return Some((today, today + Duration::days(1)));
                    }

                    None
                }
            }
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CompletionKeyRow {
    quest_id: Uuid,
    period_start: Date,
    period_end: Date,
}

async fn add_status_bulk(
    pool: &PgPool,
    quests: Vec<Quest>,
    now: OffsetDateTime,
) -> Result<Vec<QuestWithStatus>, sqlx::Error> {
    // Collect due periods we need to check in quest_completions
    let mut keys: Vec<(Uuid, Date, Date)> = Vec::new();
    let mut periods: Vec<Option<(Date, Date)>> = Vec::with_capacity(quests.len());

    for q in &quests {
        let p = current_period_for_quest(q, now);
        periods.push(p);
        if let Some((ps, pe)) = p {
            keys.push((q.id, ps, pe));
        }
    }

  // Query all existing completions for those keys in one go
let mut completed: HashSet<(Uuid, Date, Date)> = HashSet::new();

if !keys.is_empty() {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT quest_id, period_start, period_end \
         FROM quest_completions \
         WHERE "
    );

    let mut first = true;
    for (qid, ps, pe) in &keys {
        if !first {
            qb.push(" OR ");
        }
        first = false;

        qb.push("(quest_id = ")
            .push_bind(*qid)
            .push(" AND period_start = ")
            .push_bind(*ps)
            .push(" AND period_end = ")
            .push_bind(*pe)
            .push(")");
    }

    let rows: Vec<CompletionKeyRow> = qb.build_query_as().fetch_all(pool).await?;
    completed.extend(rows.into_iter().map(|r| (r.quest_id, r.period_start, r.period_end)));
}

    // Build the output
    let mut out = Vec::with_capacity(quests.len());

    for (q, p) in quests.into_iter().zip(periods.into_iter()) {
        let (is_due, is_completed, period_start, period_end) = match p {
            None => (false, false, None, None),
            Some((ps, pe)) => {
                let done = completed.contains(&(q.id, ps, pe));
                (true, done, Some(ps), Some(pe))
            }
        };

        out.push(QuestWithStatus {
            quest: q,
            is_due,
            is_completed,
            period_start,
            period_end,
        });
    }

    Ok(out)
}


// -------------------------
// Completion + rewards
// -------------------------

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

    // NOTE: streak is computed in UTC (you can switch to per-user timezone later).
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

// -------------------------
// Normalization helpers
// -------------------------

fn normalize_quest(
    kind: &mut QuestKind,

    start_at: &mut Option<OffsetDateTime>,
    due_at: &mut Option<OffsetDateTime>,

    repeat_freq: &mut Option<RepeatFreq>,
    repeat_interval: &mut Option<i32>,
    anchor_date: &mut Option<Date>,
    start_date: &mut Option<Date>,
    end_date: &mut Option<Date>,

    repeat_weekdays: &mut Option<Vec<i16>>,
    repeat_month_day: &mut Option<i16>,
    repeat_month_week: &mut Option<i16>,
    repeat_month_weekday: &mut Option<i16>,

    now: OffsetDateTime,
    today: Date,
) {
    match *kind {
        QuestKind::Once => {
            // Default window if missing
            match (*start_at, *due_at) {
                (None, None) => {
                    *start_at = Some(now);
                    *due_at = Some(now + Duration::days(7));
                }
                (Some(s), None) => {
                    *due_at = Some(s + Duration::days(7));
                }
                (None, Some(d)) => {
                    *start_at = Some(d - Duration::days(7));
                }
                (Some(_), Some(_)) => {}
            }

            if let (Some(s), Some(d)) = (*start_at, *due_at) {
                if d < s {
                    *due_at = Some(s + Duration::days(7));
                }
            }

            // Clear recurring fields
            *repeat_freq = None;
            *repeat_interval = None;
            *anchor_date = None;
            *start_date = None;
            *end_date = None;

            *repeat_weekdays = None;
            *repeat_month_day = None;
            *repeat_month_week = None;
            *repeat_month_weekday = None;
        }

        QuestKind::Recurring => {
            // Recurring quests don’t use start_at/due_at
            *start_at = None;
            *due_at = None;

            // Defaults
            if repeat_freq.is_none() {
                *repeat_freq = Some(RepeatFreq::Daily);
            }

            let interval = repeat_interval.unwrap_or(1).max(1);
            *repeat_interval = Some(interval);

            if anchor_date.is_none() {
                *anchor_date = Some(start_date.unwrap_or(today));
            }
            if start_date.is_none() {
                *start_date = *anchor_date;
            }

            // Clean up by frequency
            match repeat_freq.unwrap() {
                RepeatFreq::Daily => {
                    *repeat_weekdays = None;
                    *repeat_month_day = None;
                    *repeat_month_week = None;
                    *repeat_month_weekday = None;
                }

                RepeatFreq::Weekly => {
                    // Need at least one weekday
                    let wd = repeat_weekdays
                        .take()
                        .unwrap_or_else(|| vec![iso_weekday(anchor_date.unwrap_or(today)) as i16]);
                    let wd = if wd.is_empty() {
                        vec![iso_weekday(anchor_date.unwrap_or(today)) as i16]
                    } else {
                        wd
                    };
                    *repeat_weekdays = Some(wd);

                    // Clear monthly
                    *repeat_month_day = None;
                    *repeat_month_week = None;
                    *repeat_month_weekday = None;
                }

                RepeatFreq::Monthly => {
                    // Monthly rule: if none provided, default to day-of-month = today day
                    let has_dom = repeat_month_day.is_some();
                    let has_nth = repeat_month_week.is_some() && repeat_month_weekday.is_some();

                    if !has_dom && !has_nth {
                        *repeat_month_day = Some(today.day() as i16);
                    }

                    // If using day-of-month, clear nth-weekday; if using nth-weekday, clear dom
                    if repeat_month_day.is_some() {
                        *repeat_month_week = None;
                        *repeat_month_weekday = None;
                    } else if has_nth {
                        *repeat_month_day = None;
                    }

                    // Clear weekly
                    *repeat_weekdays = None;
                }
            }

            // end_date left as-is if provided
        }
    }
}

// -------------------------
// Date helpers
// -------------------------

fn iso_weekday(d: Date) -> i16 {
    d.weekday().number_from_monday() as i16 // 1..7
}

fn start_of_week_monday(d: Date) -> Date {
    let wd = iso_weekday(d) as i64; // 1..7
    d - Duration::days(wd - 1)
}

fn month_number(m: Month) -> i32 {
    match m {
        Month::January => 1,
        Month::February => 2,
        Month::March => 3,
        Month::April => 4,
        Month::May => 5,
        Month::June => 6,
        Month::July => 7,
        Month::August => 8,
        Month::September => 9,
        Month::October => 10,
        Month::November => 11,
        Month::December => 12,
    }
}

fn month_index(d: Date) -> i32 {
    let y = d.year() as i32;
    let m = month_number(d.month());
    y * 12 + (m - 1)
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn days_in_month(y: i32, m: Month) -> u8 {
    match m {
        Month::January => 31,
        Month::February => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        Month::March => 31,
        Month::April => 30,
        Month::May => 31,
        Month::June => 30,
        Month::July => 31,
        Month::August => 31,
        Month::September => 30,
        Month::October => 31,
        Month::November => 30,
        Month::December => 31,
    }
}

fn nth_weekday_of_month(y: i32, m: Month, weekday: i16, week: i16) -> Option<Date> {
    let weekday = weekday.clamp(1, 7) as i16;

    let dim = days_in_month(y, m) as i16;

    if week == 5 {
        for day in (1..=dim).rev() {
            let d = Date::from_calendar_date(y, m, day as u8).ok()?;
            if iso_weekday(d) == weekday {
                return Some(d);
            }
        }
        return None;
    }

    let week = week.clamp(1, 4) as i16;

    let first = Date::from_calendar_date(y, m, 1).ok()?;
    let first_wd = iso_weekday(first);

    let offset = (weekday - first_wd + 7) % 7;
    let first_occurrence_day = 1 + offset;

    let target_day = first_occurrence_day + (week - 1) * 7;
    if target_day < 1 || target_day > dim {
        return None;
    }

    Date::from_calendar_date(y, m, target_day as u8).ok()
}
