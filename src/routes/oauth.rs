use anyhow::Context;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::Host;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
    basic::BasicClient,
};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use std::{collections::HashMap, env};
use tracing::error;

use super::AppState;

use crate::auth::{AuthSession, User};

type GoogleOAuthClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

fn google_oauth_client(redirect_url: &str) -> anyhow::Result<GoogleOAuthClient> {
    let client_id = env::var("GOOGLE_CLIENT_ID").context("GOOGLE_CLIENT_ID must be set")?;
    let client_secret =
        env::var("GOOGLE_CLIENT_SECRET").context("GOOGLE_CLIENT_SECRET must be set")?;

    let client: GoogleOAuthClient = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_client_secret(ClientSecret::new(client_secret.to_string()))
        .set_auth_uri(AuthUrl::new(
            "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
        )?)
        .set_token_uri(TokenUrl::new(
            "https://oauth2.googleapis.com/token".to_string(),
        )?)
        .set_redirect_uri(RedirectUrl::new(redirect_url.to_string())?);

    Ok(client)
}

fn redirect_uri_from(hostname: &str) -> String {
    if let Ok(uri) = std::env::var("OAUTH_REDIRECT_URI") {
        return uri;
    }
    let proto = if hostname.starts_with("localhost") || hostname.starts_with("127.0.0.1") {
        "http"
    } else {
        "https"
    };
    format!("{proto}://{hostname}/oauth/google/callback")
}

pub async fn login_start(
    State(state): State<AppState>,
    Host(host): Host,
    Query(mut params): Query<HashMap<String, String>>,
) -> anyhow::Result<Redirect, (StatusCode, String)> {
    let return_url = params
        .remove("return_url")
        .unwrap_or_else(|| "/".to_string());
    let redirect_url = redirect_uri_from(&host);

    let client = google_oauth_client(&redirect_url)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (authorize_url, csrf_state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO oauth2_state_storage (csrf_state, pkce_code_verifier, return_url)
        VALUES ($1,$2,$3)
        "#,
    )
    .bind(csrf_state.secret())
    .bind(pkce_verifier.secret())
    .bind(&return_url)
    .execute(&state.db_pool)
    .await
    {
        error!(?e, "failed to save oauth state");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "oauth state error".into(),
        ));
    }

    Ok(Redirect::to(authorize_url.as_ref()))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: String,
    state: String,
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Host(host): Host,
    Query(query): Query<CallbackQuery>,
    mut auth: AuthSession,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let redirect_uri = redirect_uri_from(&host);
    let client = google_oauth_client(&redirect_uri)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (pkce_code_verifier, return_url): (String, String) = sqlx::query_as(
        r#"
        DELETE FROM oauth2_state_storage
        WHERE csrf_state = $1
        RETURNING pkce_code_verifier, return_url
        "#,
    )
    .bind(&query.state)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        error!(?e, "invalid or missing oauth state");
        (StatusCode::BAD_REQUEST, "invalid oauth state".into())
    })?;

    let token_resp = client
        .exchange_code(AuthorizationCode::new(query.code))
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_code_verifier))
        .request_async(&oauth2::reqwest::Client::new())
        .await
        .map_err(|e| {
            error!(?e, "token exchange failed");
            (StatusCode::BAD_REQUEST, "token exchange failed".into())
        })?;

    let access_token = token_resp.access_token().secret();

    let userinfo: serde_json::Value = HttpClient::new()
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| {
            error!(?e, "userinfo request failed");
            (StatusCode::BAD_GATEWAY, "userinfo request failed".into())
        })?
        .json()
        .await
        .map_err(|e| {
            error!(?e, "userinfo parse failed");
            (StatusCode::BAD_GATEWAY, "userinfo parse failed".into())
        })?;

    let google_sub = userinfo
        .get("sub")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let email = userinfo
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let email_verified = userinfo
        .get("email_verified")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let name = userinfo.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let picture = userinfo
        .get("picture")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if !email_verified {
        return Err((StatusCode::UNAUTHORIZED, "email not verified".into()));
    }

    use uuid::Uuid;

    let user_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO users (google_sub, email, name, avatar_url)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (google_sub)
          DO UPDATE SET email = EXCLUDED.email,
                        name = EXCLUDED.name,
                        avatar_url = EXCLUDED.avatar_url
        RETURNING id
        "#,
        google_sub,
        email,
        name,
        picture
    )
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        error!(?e, "failed to upsert user");
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "user upsert failed".into(),
        )
    })?;

    let user = User {
        id: user_id,
        email: email.to_string(),
        name: if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        },
        avatar: if picture.is_empty() {
            None
        } else {
            Some(picture.to_string())
        },
        session_key: google_sub.to_string(),
    };

    auth.login(&user).await.map_err(|e| {
        error!(?e, "auth.login failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "login failed".into())
    })?;

    auth.session.cycle_id().await.map_err(|e| {
        error!(?e, "session.cycle_id failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "session rotate failed".into(),
        )
    })?;

    Ok(Redirect::to(&return_url))
}

pub async fn logout(mut auth: AuthSession) -> impl IntoResponse {
    let _ = auth.logout().await;
    Redirect::to("/")
}
