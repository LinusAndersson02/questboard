CREATE TYPE quest_kind AS ENUM ('once', 'recurring');
CREATE TYPE repeat_unit AS ENUM ('day', 'week');

CREATE TABLE quests (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id),

    title           TEXT NOT NULL,
    description     TEXT NOT NULL,

    kind            quest_kind NOT NULL,

   repeat_unit     repeat_unit,        
    repeat_interval INTEGER,      
    anchor_date     DATE,          
    start_date      DATE,           
    end_date        DATE,            

    start_at        TIMESTAMPTZ,      
    due_at          TIMESTAMPTZ,       

    due_time        TIME,               
    timezone        TEXT NOT NULL DEFAULT 'UTC',

    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

