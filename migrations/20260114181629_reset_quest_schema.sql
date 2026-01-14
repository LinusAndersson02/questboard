BEGIN;

-- Drop dependent table first
DROP TABLE IF EXISTS quest_completions;

-- Drop main table
DROP TABLE IF EXISTS quests;

-- Drop legacy types (and new type too, since we recreate cleanly)
DROP TYPE IF EXISTS repeat_unit;
DROP TYPE IF EXISTS repeat_freq;
DROP TYPE IF EXISTS quest_kind;

-- Recreate enums
CREATE TYPE quest_kind AS ENUM ('once', 'recurring');
CREATE TYPE repeat_freq AS ENUM ('daily', 'weekly', 'monthly');

-- Recreate quests table (NEW model only)
CREATE TABLE quests (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    title           TEXT NOT NULL,
    description     TEXT NOT NULL,

    kind            quest_kind NOT NULL,

    xp_reward       INTEGER NOT NULL DEFAULT 0,
    coin_reward     INTEGER NOT NULL DEFAULT 0,

    -- Once-only scheduling
    start_at        TIMESTAMPTZ,
    due_at          TIMESTAMPTZ,

    -- Recurring scheduling
    repeat_freq         repeat_freq,
    repeat_interval     INTEGER,
    anchor_date         DATE,
    start_date          DATE,
    end_date            DATE,

    -- Weekly rules: ISO weekdays 1=Mon..7=Sun
    repeat_weekdays     SMALLINT[],

    -- Monthly rules (choose exactly one rule):
    --  A) day-of-month: repeat_month_day = 1..31
    --  B) nth weekday: repeat_month_week = 1..5 (5 may represent "last" in your app)
    --                 repeat_month_weekday = 1..7
    repeat_month_day        SMALLINT,
    repeat_month_week       SMALLINT,
    repeat_month_weekday    SMALLINT,

    -- Optional: “due time” for recurring occurrences (and/or once if you want)
    due_time        TIME,
    timezone        TEXT NOT NULL DEFAULT 'UTC',

    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- =========================
-- Constraints (the "clean" part)
-- =========================

-- once must have due_at
ALTER TABLE quests
  ADD CONSTRAINT quests_once_requires_due_at
  CHECK (kind <> 'once' OR due_at IS NOT NULL);

-- once must NOT have recurring fields
ALTER TABLE quests
  ADD CONSTRAINT quests_once_no_repeat_fields
  CHECK (
    kind <> 'once'
    OR (
      repeat_freq IS NULL
      AND repeat_interval IS NULL
      AND anchor_date IS NULL
      AND start_date IS NULL
      AND end_date IS NULL
      AND repeat_weekdays IS NULL
      AND repeat_month_day IS NULL
      AND repeat_month_week IS NULL
      AND repeat_month_weekday IS NULL
    )
  );

-- recurring must NOT have once timestamps
ALTER TABLE quests
  ADD CONSTRAINT quests_recurring_no_due_at
  CHECK (kind <> 'recurring' OR (start_at IS NULL AND due_at IS NULL));

-- recurring must have core recurrence fields
ALTER TABLE quests
  ADD CONSTRAINT quests_recurring_core_required
  CHECK (
    kind <> 'recurring'
    OR (
      repeat_freq IS NOT NULL
      AND repeat_interval IS NOT NULL AND repeat_interval >= 1
      AND anchor_date IS NOT NULL
      AND start_date IS NOT NULL
    )
  );

-- weekly requires weekdays
ALTER TABLE quests
  ADD CONSTRAINT quests_weekly_requires_weekdays
  CHECK (
    kind <> 'recurring'
    OR repeat_freq <> 'weekly'
    OR (repeat_weekdays IS NOT NULL AND array_length(repeat_weekdays, 1) >= 1)
  );

-- weekdays values must be 1..7 (NO subquery in CHECK)
ALTER TABLE quests
  ADD CONSTRAINT quests_weekday_values
  CHECK (
    repeat_weekdays IS NULL
    OR repeat_weekdays <@ ARRAY[1,2,3,4,5,6,7]::SMALLINT[]
  );

-- monthly must choose exactly one rule:
-- either month_day OR (month_week + month_weekday)
ALTER TABLE quests
  ADD CONSTRAINT quests_monthly_rule_valid
  CHECK (
    kind <> 'recurring'
    OR repeat_freq <> 'monthly'
    OR (
      (repeat_month_day IS NOT NULL AND repeat_month_week IS NULL AND repeat_month_weekday IS NULL)
      OR
      (repeat_month_day IS NULL AND repeat_month_week IS NOT NULL AND repeat_month_weekday IS NOT NULL)
    )
  );

-- range checks
ALTER TABLE quests
  ADD CONSTRAINT quests_month_day_range
  CHECK (repeat_month_day IS NULL OR (repeat_month_day BETWEEN 1 AND 31));

ALTER TABLE quests
  ADD CONSTRAINT quests_month_week_range
  CHECK (repeat_month_week IS NULL OR (repeat_month_week BETWEEN 1 AND 5));

ALTER TABLE quests
  ADD CONSTRAINT quests_month_weekday_range
  CHECK (repeat_month_weekday IS NULL OR (repeat_month_weekday BETWEEN 1 AND 7));

-- =========================
-- Indexes
-- =========================

CREATE INDEX IF NOT EXISTS idx_quests_user_kind_active
  ON quests (user_id, kind, is_active);

CREATE INDEX IF NOT EXISTS idx_quests_user_due_at
  ON quests (user_id, due_at);

CREATE INDEX IF NOT EXISTS idx_quests_user_repeat_freq
  ON quests (user_id, repeat_freq);

-- =========================
-- Completions table stays the same conceptually
-- =========================

CREATE TABLE quest_completions (
    id              BIGSERIAL PRIMARY KEY,
    quest_id        UUID NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    period_start    DATE NOT NULL,
    period_end      DATE NOT NULL,
    completed_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    xp_reward       INTEGER NOT NULL DEFAULT 0,
    coin_reward     INTEGER NOT NULL DEFAULT 0,

    CONSTRAINT uq_quest_period UNIQUE (quest_id, period_start, period_end)
);

CREATE INDEX IF NOT EXISTS idx_quest_completions_quest_period
  ON quest_completions (quest_id, period_start, period_end);

COMMIT;

