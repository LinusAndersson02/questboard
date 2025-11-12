use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub avatar: Option<String>,

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
