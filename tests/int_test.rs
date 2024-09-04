const URL: &str = "http://localhost:3000/";

#[cfg(test)]
mod tests {
    use futures::{SinkExt, StreamExt};
    use mongodb::bson::oid::ObjectId;
    use oh_hell::{
        infra::{
            auth::{LoginParams, TokenResponse},
            lobby::CreateLobbyResponse,
            ClientGameMessage, ClientMessage, JoinLobbyDto, ServerGameMessage, ServerMessage,
        },
        models::Card,
    };
    use reqwest::Client;
    use tokio::{net::TcpStream, task};
    use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

    use crate::URL;

    #[tokio::test]
    async fn test_example() {
        let start_app = oh_hell::start_app();
        let app = start_app;

        task::spawn(app);

        let mut client = reqwest::Client::new();

        let p1_t = login(&mut client).await;

        let lobby_id = create_lobby(&mut client, &p1_t).await;

        let p2_t = login(&mut client).await;

        let lobby = join_lobby(&mut client, &p2_t, lobby_id).await;

        assert!(lobby.players.len() == 2);

        let mut p1_s = connect_ws(p1_t).await;

        let mut p2_s = connect_ws(p2_t).await;

        send_msg(&mut p1_s, ClientGameMessage::Ready).await;

        send_msg(&mut p2_s, ClientGameMessage::Ready).await;

        let player_ready_predicate =
            |m: &ServerGameMessage| matches!(m, ServerGameMessage::PlayerReady { player_id: _ });

        recv_game_msg(&mut p1_s, player_ready_predicate).await;
        recv_game_msg(&mut p2_s, player_ready_predicate).await;

        let p1_deck = get_deck(&mut p1_s).await;
        let p2_deck = get_deck(&mut p2_s).await;

        assert!(p1_deck.len() == 1);
        assert!(p2_deck.len() == 1);

        println!("{:?}", p1_deck);
        println!("{:?}", p2_deck);

        panic!("erro");
    }

    async fn get_deck(stream: &mut WebSocket) -> Vec<Card> {
        match recv_game_msg(stream, |_| true).await {
            ServerGameMessage::PlayerDeck(c) => c,
            _ => panic!("Message not expected"),
        }
    }

    async fn recv_game_msg<F>(stream: &mut WebSocket, predicate: F) -> ServerGameMessage
    where
        F: Fn(&ServerGameMessage) -> bool,
    {
        let msg = recv_msg(stream).await;

        match msg {
            ServerMessage::Authorized(_) => panic!("Not expected"),
            ServerMessage::Game(g) => match predicate(&g) {
                true => g,
                false => panic!("Message not expected"),
            },
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

        let msg = recv_msg(&mut stream).await;

        let _ = match msg {
            ServerMessage::Authorized(a) => a,
            _ => panic!("Unexpected message"),
        };

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
        let params = LoginParams {
            picture_index: 0,
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
