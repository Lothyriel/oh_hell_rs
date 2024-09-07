const URL: &str = "http://localhost:3000/";

#[cfg(test)]
mod tests {
    use futures::{stream::FusedStream, SinkExt, StreamExt};
    use mongodb::bson::oid::ObjectId;
    use oh_hell::{
        infra::{
            auth::{get_claims_from_token, ProfileParams, TokenResponse},
            lobby::CreateLobbyResponse,
            ClientGameMessage, ClientMessage, JoinLobbyDto, ServerMessage,
        },
        models::Card,
    };
    use reqwest::Client;
    use tokio::{net::TcpStream, task};
    use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

    use crate::URL;

    #[tokio::test]
    async fn test_example() {
        task::spawn(oh_hell::start_app());

        let mut client = reqwest::Client::new();

        let p1_token = login(&mut client).await;
        let p2_token = login(&mut client).await;

        let p1_claims = get_claims_from_token(&p1_token).await.unwrap();
        let p2_claims = get_claims_from_token(&p2_token).await.unwrap();

        let lobby_id = create_lobby(&mut client, &p1_token).await;

        let lobby = join_lobby(&mut client, &p1_token, lobby_id).await;
        assert!(lobby.players.len() == 1);

        let lobby = join_lobby(&mut client, &p2_token, lobby_id).await;
        assert!(lobby.players.len() == 2);

        let mut p1 = connect_ws(p1_token).await;
        let mut p2 = connect_ws(p2_token).await;

        let msg = ClientGameMessage::PlayerStatusChange { ready: true };
        send_msg(&mut p1, msg).await;
        send_msg(&mut p2, msg).await;

        //p2 receives himself and p1 ready
        assert_game_msg(&mut p1, validate_player_status_change).await;
        assert_game_msg(&mut p2, validate_player_status_change).await;
        assert_game_msg(&mut p2, validate_player_status_change).await;

        let p1_deck = get_deck(&mut p1).await;
        let p2_deck = get_deck(&mut p2).await;
        assert!(p1_deck.len() == 1);
        assert!(p2_deck.len() == 1);

        assert_game_msg(&mut p1, get_bidding_turn_predicate(p1_claims.id())).await;
        let msg = ClientGameMessage::PutBid { bid: 0 };
        send_msg(&mut p1, msg).await;

        assert_game_msg(&mut p2, get_bidding_turn_predicate(p2_claims.id())).await;
        send_msg(&mut p2, msg).await;

        assert_game_msg(&mut p1, validate_player_turn).await;
        assert_game_msg(&mut p2, validate_player_turn).await;

        let msg = ClientGameMessage::PlayTurn { card: p1_deck[0] };
        send_msg(&mut p1, msg).await;

        assert_game_msg(&mut p1, validate_turn_played).await;
        assert_game_msg(&mut p2, validate_turn_played).await;
    }

    fn validate_turn_played(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::TurnPlayed { turn: _ })
    }

    fn validate_player_turn(m: &ServerMessage) -> bool {
        matches!(m, ServerMessage::PlayerTurn { player_id: _ })
    }

    fn get_bidding_turn_predicate(id: String) -> impl FnOnce(&ServerMessage) -> bool {
        |m: &ServerMessage| m == &ServerMessage::PlayerBiddingTurn { player_id: id }
    }

    fn validate_player_status_change(m: &ServerMessage) -> bool {
        matches!(
            m,
            ServerMessage::PlayerStatusChange {
                player_id: _,
                ready: _
            }
        )
    }

    async fn get_deck(stream: &mut WebSocket) -> Vec<Card> {
        match assert_game_msg(stream, |m| matches!(m, ServerMessage::PlayerDeck(_))).await {
            ServerMessage::PlayerDeck(c) => c,
            _ => panic!("Should be a deck message"),
        }
    }

    async fn assert_game_msg<F>(stream: &mut WebSocket, predicate: F) -> ServerMessage
    where
        F: FnOnce(&ServerMessage) -> bool,
    {
        let msg = recv_msg(stream).await;

        match predicate(&msg) {
            true => {
                println!("Asserted game msg {msg:?}");
                msg
            }
            false => panic!("Message not expected {msg:?}"),
        }
    }

    async fn send_msg(stream: &mut WebSocket, msg: ClientGameMessage) {
        let msg = ClientMessage::Game(msg);

        let msg = serde_json::to_string(&msg).unwrap();

        stream.send(Message::Text(msg)).await.unwrap();
    }

    type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    async fn connect_ws(token: String) -> WebSocket {
        let (mut stream, _) = connect_async("ws://localhost:3000/game").await.unwrap();

        let msg = ClientMessage::Auth(token);

        let json = serde_json::to_string(&msg).unwrap();

        stream.send(Message::Text(json)).await.unwrap();

        assert!(!stream.is_terminated());

        stream
    }

    async fn recv_msg(stream: &mut WebSocket) -> ServerMessage {
        let msg = stream.next().await.unwrap().unwrap();

        let msg: ServerMessage = match msg {
            Message::Text(t) => serde_json::from_str(&t).unwrap(),
            _ => panic!("Wrong format"),
        };
        msg
    }

    async fn join_lobby(client: &mut Client, token: &str, lobby_id: ObjectId) -> JoinLobbyDto {
        let res = client
            .put(format!("{URL}lobby/{lobby_id}"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();

        res.json().await.unwrap()
    }

    async fn create_lobby(client: &mut Client, token: &str) -> ObjectId {
        let res = client
            .post(format!("{URL}lobby"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();

        let res: CreateLobbyResponse = res.json().await.unwrap();

        res.lobby_id
    }

    async fn login(client: &mut Client) -> String {
        let params = ProfileParams {
            picture: "picture.jpg".to_string(),
            nickname: "JX".to_string(),
        };

        let res = client
            .post(format!("{URL}auth/login"))
            .json(&params)
            .send()
            .await
            .unwrap();

        let res: TokenResponse = res.json().await.unwrap();

        res.token
    }
}
