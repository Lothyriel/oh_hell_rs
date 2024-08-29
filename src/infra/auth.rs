use std::{net::SocketAddr, sync::OnceLock};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::IntoResponse,
    routing, Json, Router,
};
use jsonwebtoken::{
    errors::Error,
    jwk::{Jwk, JwkSet},
    DecodingKey, EncodingKey, Header, TokenData,
};
use mongodb::bson::oid::ObjectId;
use serde_json::{json, Value};

use crate::services::{manager::Manager, repositories::auth::LoginDto};

pub fn router() -> Router<Manager> {
    Router::new().route("/login", routing::post(login))
}

pub static JWT_KEY: OnceLock<String> = OnceLock::new();

pub async fn middleware(
    State(manager): State<Manager>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, AuthError> {
    let who = *req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .expect("We should get the IP here");

    let token = get_token_from_req(&mut req)
        .await
        .ok_or(AuthError::TokenNotPresent)?;

    let claims = get_claims_from_token(token).await?;

    req.extensions_mut().insert(claims.clone());

    let insert = manager
        .auth_repo
        .insert_login(&LoginDto::new(claims, who.to_string()))
        .await;

    if let Err(e) = insert {
        tracing::error!("Error while saving login info | {e}")
    }

    Ok(next.run(req).await)
}

#[derive(serde::Deserialize)]
struct LoginParams {
    nickname: String,
    picture_index: usize,
}

async fn login(Json(params): Json<LoginParams>) -> Json<Value> {
    let claims = AnonymousUserClaims {
        id: ObjectId::new(),
        picture_index: params.picture_index,
        name: params.nickname,
    };

    let token = jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_KEY.get().expect("JWT_KEY should be set").as_bytes()),
    )
    .expect("Should encode JWT");

    Json(serde_json::json!({"token": token}))
}

pub async fn get_claims_from_token(token: &str) -> Result<UserClaims, AuthError> {
    get_google_claims(token)
        .await
        .or_else(|_| get_session_claims(token))
}

async fn get_token_from_req(req: &mut Request) -> Option<&str> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.starts_with("Bearer ").then(|| &value[7..]))
}

fn get_session_claims(_token: &str) -> Result<UserClaims, AuthError> {
    let claims = AnonymousUserClaims {
        id: ObjectId::new(),
        picture_index: 0,
        name: "João Xavier".to_string(),
    };

    Ok(UserClaims::Anonymous(claims))
}

async fn get_google_claims(token: &str) -> Result<UserClaims, AuthError> {
    let header = jsonwebtoken::decode_header(token)?;
    let kid = header.kid.ok_or(AuthError::InvalidKid)?;
    let jwks = get_google_jwks().await?;
    let jwk = jwks.find(&kid).ok_or(AuthError::InvalidKid)?;
    let token_data = decode_google_claims(token, jwk)?;
    let claims = UserClaims::Google(token_data.claims);

    Ok(claims)
}

fn decode_google_claims(token: &str, jwk: &Jwk) -> Result<TokenData<GoogleUserClaims>, Error> {
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);

    validation.set_issuer(&["https://accounts.google.com"]);

    // TODO set google audience
    // TODO set /.well-known
    validation.set_audience(&[
        "824653628296-ahr9jr3aqgr367mul4p359dj4plsl67a.apps.googleusercontent.com",
    ]);

    jsonwebtoken::decode::<GoogleUserClaims>(token, &DecodingKey::from_jwk(jwk)?, &validation)
}

async fn get_google_jwks() -> Result<JwkSet, reqwest::Error> {
    let response = reqwest::get("https://www.googleapis.com/oauth2/v3/certs").await?;

    response.json().await
}

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Auth token not found on the request")]
    TokenNotPresent,
    #[error("Invalid KeyId ('kid') on token")]
    InvalidKid,
    #[error("Invalid token: ({0})")]
    JwtValidation(#[from] jsonwebtoken::errors::Error),
    #[error("Error during certificate retrieval: ({0})")]
    IO(#[from] reqwest::Error),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let body = Json(json!({"error": self.to_string() }));

        (StatusCode::UNAUTHORIZED, body).into_response()
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum UserClaims {
    Anonymous(AnonymousUserClaims),
    Google(GoogleUserClaims),
}

impl UserClaims {
    pub fn id(&self) -> String {
        match self {
            UserClaims::Anonymous(a) => a.id.to_string(),
            UserClaims::Google(g) => g.email.clone(),
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct AnonymousUserClaims {
    id: ObjectId,
    picture_index: usize,
    name: String,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct GoogleUserClaims {
    pub email: String,
    pub name: String,
    pub picture: String,
}
