use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::Date;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub avatar: Option<String>,

    pub xp_total: i64,
    pub coins: i64,
    pub current_streak: i32,
    pub longest_streak: i32,
    pub last_active_date: Option<Date>,
    pub timezone: String,

    pub session_key: String,
}

impl AuthUser for User {
    type Id = Uuid;
    fn id(&self) -> Self::Id {
        self.id
    }
    fn session_auth_hash(&self) -> &[u8] {
        self.session_key.as_bytes()
    }
}

#[derive(Clone)]
pub struct DbBackend {
    pub pool: PgPool,
}

#[derive(Clone)]
pub struct NoCredentials;

impl AuthnBackend for DbBackend {
    type User = User;
    type Credentials = NoCredentials;
    type Error = sqlx::Error;

    async fn authenticate(
        &self,
        _creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        sqlx::query_as!(
            User,
            r#"
        SELECT
            id              AS "id!: Uuid",
            email           AS "email!",
            name,
            avatar_url      AS "avatar?",
            xp_total        AS "xp_total!",
            coins           AS "coins!",
            current_streak  AS "current_streak!",
            longest_streak  AS "longest_streak!",
            last_active_date AS "last_active_date?",
            timezone        AS "timezone!",
            google_sub      AS "session_key!"
        FROM users
        WHERE id = $1
        "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
    }
}

pub type AuthSession = axum_login::AuthSession<DbBackend>;
