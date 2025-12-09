CREATE EXTENSION IF NOT EXISTS pgcrypto;             
CREATE TABLE IF NOT EXISTS users (
	  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),  
	  google_sub  text    NOT NULL UNIQUE,
	  email       text    NOT NULL,
	  name        text,
	  avatar_url  text,
	xp_total BIGINT NOT NULL DEFAULT 0,
	coins BIGINT NOT NULL DEFAULT 0,
	current_streak INTEGER NOT NULL DEFAULT 0,
	longest_streak INTEGER NOT NULL DEFAULT 0,
	last_active_date DATE,
	timezone TEXT NOT NULL DEFAULT 'UTC',
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);

