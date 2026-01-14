
-- 1) New enum for frequency
DO $$ BEGIN
  CREATE TYPE repeat_freq AS ENUM ('daily', 'weekly', 'monthly');
EXCEPTION
  WHEN duplicate_object THEN NULL;
END $$;

-- 2) Add new recurring rule columns
ALTER TABLE quests
  ADD COLUMN IF NOT EXISTS repeat_freq repeat_freq,
  ADD COLUMN IF NOT EXISTS repeat_weekdays SMALLINT[],          -- ISO weekday: 1=Mon .. 7=Sun
  ADD COLUMN IF NOT EXISTS repeat_month_day SMALLINT,           -- 1..31
  ADD COLUMN IF NOT EXISTS repeat_month_week SMALLINT,          -- 1..5 (you may treat 5 as "last")
  ADD COLUMN IF NOT EXISTS repeat_month_weekday SMALLINT;       -- 1..7 (Mon..Sun)

UPDATE quests
SET
  repeat_freq = CASE
    WHEN kind = 'recurring' AND repeat_unit = 'day' THEN 'daily'::repeat_freq
    WHEN kind = 'recurring' AND repeat_unit = 'week' THEN 'weekly'::repeat_freq
    ELSE repeat_freq
  END
WHERE repeat_freq IS NULL;

-- weekly weekday backfill: use anchor_date weekday if present; else Monday (1)
-- Postgres extract(isodow) returns 1..7.
UPDATE quests
SET repeat_weekdays = ARRAY[
  COALESCE(EXTRACT(ISODOW FROM anchor_date)::SMALLINT, 1::SMALLINT)
]
WHERE kind = 'recurring'
  AND repeat_freq = 'weekly'
  AND (repeat_weekdays IS NULL OR array_length(repeat_weekdays, 1) IS NULL);

-- Ensure repeat_interval exists for recurring (you already have repeat_interval column)
UPDATE quests
SET repeat_interval = 1
WHERE kind = 'recurring'
  AND (repeat_interval IS NULL OR repeat_interval < 1);

-- Ensure anchor_date exists for recurring
UPDATE quests
SET anchor_date = COALESCE(anchor_date, start_date, CURRENT_DATE)
WHERE kind = 'recurring'
  AND anchor_date IS NULL;

-- 4) Constraints (recommended)

-- repeat_freq must be set for recurring, and NULL for once
ALTER TABLE quests
  DROP CONSTRAINT IF EXISTS quests_repeat_freq_presence;
ALTER TABLE quests
  ADD CONSTRAINT quests_repeat_freq_presence
  CHECK (
    (kind = 'once' AND repeat_freq IS NULL)
    OR
    (kind = 'recurring' AND repeat_freq IS NOT NULL)
  );

-- recurring must have repeat_interval >= 1
ALTER TABLE quests
  DROP CONSTRAINT IF EXISTS quests_repeat_interval_valid;
ALTER TABLE quests
  ADD CONSTRAINT quests_repeat_interval_valid
  CHECK (
    kind = 'once'
    OR
    (repeat_interval IS NOT NULL AND repeat_interval >= 1)
  );

-- weekly requires weekdays
ALTER TABLE quests
  DROP CONSTRAINT IF EXISTS quests_weekly_requires_weekdays;
ALTER TABLE quests
  ADD CONSTRAINT quests_weekly_requires_weekdays
  CHECK (
    kind = 'once'
    OR repeat_freq <> 'weekly'
    OR (repeat_weekdays IS NOT NULL AND array_length(repeat_weekdays, 1) >= 1)
  );

-- monthly must choose exactly one rule:
-- either month_day OR (month_week + month_weekday)
ALTER TABLE quests
  DROP CONSTRAINT IF EXISTS quests_monthly_rule_valid;
ALTER TABLE quests
  ADD CONSTRAINT quests_monthly_rule_valid
  CHECK (
    kind = 'once'
    OR repeat_freq <> 'monthly'
    OR (
      (repeat_month_day IS NOT NULL AND repeat_month_week IS NULL AND repeat_month_weekday IS NULL)
      OR
      (repeat_month_day IS NULL AND repeat_month_week IS NOT NULL AND repeat_month_weekday IS NOT NULL)
    )
  );

-- validate ranges
ALTER TABLE quests
  DROP CONSTRAINT IF EXISTS quests_month_day_range;
ALTER TABLE quests
  ADD CONSTRAINT quests_month_day_range
  CHECK (repeat_month_day IS NULL OR (repeat_month_day >= 1 AND repeat_month_day <= 31));

ALTER TABLE quests
  DROP CONSTRAINT IF EXISTS quests_month_week_range;
ALTER TABLE quests
  ADD CONSTRAINT quests_month_week_range
  CHECK (repeat_month_week IS NULL OR (repeat_month_week >= 1 AND repeat_month_week <= 5));

-- validate weekday values WITHOUT subqueries (Postgres doesn't allow unnest() in CHECK)
ALTER TABLE quests
  DROP CONSTRAINT IF EXISTS quests_weekday_values;
ALTER TABLE quests
  ADD CONSTRAINT quests_weekday_values
  CHECK (
    repeat_weekdays IS NULL
    OR repeat_weekdays <@ ARRAY[1,2,3,4,5,6,7]::SMALLINT[]
  );

-- one-time quests should have a due date
ALTER TABLE quests
  DROP CONSTRAINT IF EXISTS quests_once_requires_due_at;
ALTER TABLE quests
  ADD CONSTRAINT quests_once_requires_due_at
  CHECK (kind <> 'once' OR due_at IS NOT NULL);

-- 5) Indexes for upcoming filtering
CREATE INDEX IF NOT EXISTS idx_quests_user_kind_active
  ON quests (user_id, kind, is_active);

CREATE INDEX IF NOT EXISTS idx_quests_user_due_at
  ON quests (user_id, due_at);

CREATE INDEX IF NOT EXISTS idx_quests_user_repeat_freq
  ON quests (user_id, repeat_freq);

