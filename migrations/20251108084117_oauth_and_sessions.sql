CREATE TABLE IF NOT EXISTS oauth2_state_storage (
  csrf_state          text PRIMARY KEY,
  pkce_code_verifier  text NOT NULL,
  return_url          text NOT NULL,
  created_at          timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS user_sessions (
  session_token_p1  text NOT NULL,
  session_token_p2  text NOT NULL,
  user_id           text NOT NULL,      
  created_at        timestamptz NOT NULL DEFAULT now(),
  expires_at        timestamptz NOT NULL,
  PRIMARY KEY (session_token_p1)           
);

CREATE INDEX IF NOT EXISTS idx_user_sessions_user ON user_sessions(user_id);
