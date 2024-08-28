use chrono::{DateTime, Utc};
use mongodb::{error::Result, Collection, Database};

use crate::infra::auth::UserClaims;

#[derive(Clone)]
pub struct AuthRepository {
    logins: Collection<LoginDto>,
}

impl AuthRepository {
    pub fn new(database: &Database) -> Self {
        Self {
            logins: database.collection("Logins"),
        }
    }

    pub async fn insert_login(&self, game: &LoginDto) -> Result<()> {
        self.logins.insert_one(game).await?;

        Ok(())
    }
}

#[derive(serde::Serialize)]
pub struct LoginDto {
    claims: UserClaims,
    ip: String,
    time: DateTime<Utc>,
}

impl LoginDto {
    pub fn new(claims: UserClaims, ip: String) -> Self {
        Self {
            claims,
            ip,
            time: Utc::now(),
        }
    }
}
