use oauth2::{basic::BasicClient, AuthUrl, ClientId, ClientSecret, EndpointNotSet, EndpointSet, RedirectUrl, TokenUrl};
use axum::extract::State;
use axum::extract::Query;
use axum_extra::extract::Host;
use anyhow::Context;  
use std::{collections::HashMap, env};
use axum::response::Redirect;
use oauth2::PkceCodeChallenge;
use oauth2::CsrfToken;
use oauth2::Scope;
use tracing::{info,error};
use reqwest::header;
use axum::response::IntoResponse;
use axum::http::HeaderMap;
use axum_extra::headers;
use serde::Deserialize;
use oauth2::AuthorizationCode;
use oauth2::PkceCodeVerifier;
use oauth2::TokenResponse;
use reqwest::Client as HttpClient;
use uuid::Uuid;

use super::AppState;


type GoogleOAuthClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

fn google_oauth_client(redirect_url: &str) -> anyhow::Result<GoogleOAuthClient> {
    let client_id = env::var("GOOGLE_CLIENT_ID")
        .context("GOOGLE_CLIENT_ID must be set")?;
    let client_secret = env::var("GOOGLE_CLIENT_SECRET")
        .context("GOOGLE_CLIENT_SECRET must be set")?;

    
    let client : GoogleOAuthClient = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_client_secret(ClientSecret::new(client_secret.to_string()))
        .set_auth_uri(AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())?)
        .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".to_string())?)
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



pub async fn login_start( State(state) : State<AppState>,
Host(host): Host,
Query(mut params) : Query<HashMap<String,String>>,
) -> anyhow::Result<Redirect, (axum::http::StatusCode, String)> {

    let return_url = params.remove("return_url").unwrap_or_else(|| "/".to_string()) ;
    let redirect_url = redirect_uri_from(&host);

    let client = google_oauth_client(&redirect_url).map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;


    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (authorize_url, csrf_state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    tracing::info!(%redirect_url, auth_url=%authorize_url, "oauth start");


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
        return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, "oauth state error".into()))
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
) -> Result<impl IntoResponse, (axum::http::StatusCode, String)> {
    let redirect_uri = redirect_uri_from(&host);
    let client = google_oauth_client(&redirect_uri)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
        (axum::http::StatusCode::BAD_REQUEST, "invalid oauth state".into())
    })?;

    let token_resp = client
        .exchange_code(AuthorizationCode::new(query.code))
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_code_verifier))
        .request_async(&oauth2::reqwest::Client::new())
        .await
        .map_err(|e| {
            error!(?e, "token exchange failed");
            (axum::http::StatusCode::BAD_REQUEST, "token exchange failed".into())
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
            (axum::http::StatusCode::BAD_GATEWAY, "userinfo request failed".into())
        })?
        .json()
        .await
        .map_err(|e| {
            error!(?e, "userinfo parse failed");
            (axum::http::StatusCode::BAD_GATEWAY, "userinfo parse failed".into())
        })?;

    let google_sub = userinfo.get("sub").and_then(|v| v.as_str()).unwrap_or_default();
    let email = userinfo.get("email").and_then(|v| v.as_str()).unwrap_or_default();
    let email_verified = userinfo.get("email_verified").and_then(|v| v.as_bool()).unwrap_or(false);
    let name = userinfo.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let picture = userinfo.get("picture").and_then(|v| v.as_str()).unwrap_or("");

    if !email_verified {
        return Err((axum::http::StatusCode::UNAUTHORIZED, "email not verified".into()));
    }

    let (user_id,): (String,) = sqlx::query_as(
        r#"
        INSERT INTO users (google_sub, email, name, avatar_url)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (google_sub)
        DO UPDATE SET email = EXCLUDED.email,
                      name = EXCLUDED.name,
                      avatar_url = EXCLUDED.avatar_url
        RETURNING id::text
        "#,
    )
    .bind(google_sub)
    .bind(email)
    .bind(name)
    .bind(picture)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        error!(?e, "failed to upsert user");
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "user upsert failed".into())
    })?;

    let p1 = Uuid::new_v4().to_string();
    let p2 = Uuid::new_v4().to_string();
    let session_token = format!("{p1}_{p2}");

    let now = chrono::Utc::now();
    let exp = now + chrono::Duration::hours(24);

    sqlx::query(
        r#"
        INSERT INTO user_sessions (session_token_p1, session_token_p2, user_id, created_at, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(&p1)
    .bind(&p2)
    .bind(&user_id)
    .bind(now)
    .bind(exp)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        error!(?e, "failed to create session");
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "session create failed".into())
    })?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!(
            "session_token={}; Path=/; HttpOnly; SameSite=Strict{}",
            session_token,
            if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
                "" 
            } else {
                "; Secure"
            }
        ).parse().unwrap(),
    );

    info!("user {} logged in", user_id);
    Ok((headers, Redirect::to(&return_url)))
}

pub async fn logout(
    State(state): State<AppState>,
    cookie_header: Option<axum_extra::TypedHeader<headers::Cookie>>,
) -> impl IntoResponse {
    if let Some(axum_extra::TypedHeader(cookies)) = cookie_header {
        if let Some(token) = cookies.get("session_token") {
            if let Some((p1, _)) = token.split_once('_') {
                let _ = sqlx::query("DELETE FROM user_sessions WHERE session_token_p1 = $1")
                    .bind(p1)
                    .execute(&state.db_pool)
                    .await;
            }
        }
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        "session_token=deleted; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
            .parse()
            .unwrap(),
    );
    (headers, Redirect::to("/"))
}
