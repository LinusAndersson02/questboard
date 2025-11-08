CREATE EXTENSION IF NOT EXISTS pgcrypto;    

CREATE TABLE IF NOT EXISTS users (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),   
  google_sub  text UNIQUE NOT NULL,   
  email       text UNIQUE NOT NULL,
  name        text,
  avatar_url  text,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_users_google_sub ON users(google_sub);
CREATE INDEX IF NOT EXISTS idx_users_email      ON users(email);
