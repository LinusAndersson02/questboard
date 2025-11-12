CREATE EXTENSION IF NOT EXISTS pgcrypto;             
CREATE TABLE IF NOT EXISTS users (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),  
  google_sub  text    NOT NULL UNIQUE,
  email       text    NOT NULL,
  name        text,
  avatar_url  text,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);

