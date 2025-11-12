CREATE TABLE IF NOT EXISTS oauth2_state_storage (
  csrf_state         text PRIMARY KEY,
  pkce_code_verifier text NOT NULL,
  return_url         text NOT NULL,
  created_at         timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_oauth2_state_created_at
  ON oauth2_state_storage (created_at);

