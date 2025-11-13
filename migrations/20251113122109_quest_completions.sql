CREATE TABLE quest_completions (
    id                  BIGSERIAL PRIMARY KEY,
    quest_id            UUID NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    period_start        DATE NOT NULL,
    period_end          DATE NOT NULL,
    completed_at        TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT uq_quest_period UNIQUE (quest_id, period_start, period_end)
);

