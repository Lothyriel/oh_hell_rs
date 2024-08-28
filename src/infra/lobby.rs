use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing, Extension, Json, Router,
};
use mongodb::bson::oid::ObjectId;
use reqwest::StatusCode;
use serde_json::Value;

use crate::{
    models::GameError,
    services::manager::{LobbyError, Manager},
};

use super::{
    auth::UserClaims,
    game::{GetLobbyDto, JoinLobbyDto},
};

pub fn router() -> Router<Manager> {
    Router::new()
        .route("/", routing::get(get_lobbies))
        .route("/", routing::post(create_lobby))
        .route("/:id", routing::put(join_lobby))
}

async fn get_lobbies(State(manager): State<Manager>) -> Json<Vec<GetLobbyDto>> {
    Json(manager.get_lobbies().await)
}

async fn join_lobby(
    State(manager): State<Manager>,
    Extension(user_claims): Extension<UserClaims>,
    Path(id): Path<ObjectId>,
) -> Result<Json<JoinLobbyDto>, LobbyError> {
    let players = manager.join_lobby(id, user_claims).await?;

    Ok(Json(JoinLobbyDto { players, id }))
}

async fn create_lobby(
    State(manager): State<Manager>,
    Extension(user_claims): Extension<UserClaims>,
) -> Json<Value> {
    let id = manager.create_lobby(user_claims).await;

    Json(serde_json::json!({"lobby_id": id}))
}

impl IntoResponse for LobbyError {
    fn into_response(self) -> axum::response::Response {
        let code = match &self {
            LobbyError::InvalidLobby => StatusCode::NOT_FOUND,
            LobbyError::GameAlreadyStarted => StatusCode::BAD_REQUEST,
            LobbyError::GameNotStarted => StatusCode::BAD_REQUEST,
            LobbyError::WrongLobby => StatusCode::BAD_REQUEST,
            LobbyError::GameError(e) => match e {
                GameError::NotEnoughPlayers => StatusCode::BAD_REQUEST,
                GameError::TooManyPlayers => StatusCode::BAD_REQUEST,
                GameError::InvalidTurn(_) => StatusCode::BAD_REQUEST,
                GameError::InvalidBid(_) => StatusCode::BAD_REQUEST,
            },
        };

        (code, Json(serde_json::json!({"error": self.to_string()}))).into_response()
    }
}
