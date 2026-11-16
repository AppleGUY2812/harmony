use crate::database::HarmonyDb;
use crate::middleware::protect_route::JwtGuard;
use crate::models::CallClientEvent;
use crate::models::ErrorResponse;
use crate::models::Server;
use crate::models::ServerType;
use crate::models::UserSessionIdMap;
use crate::models::{ServerSessionIdMap, SessionIdWebsocketMap};
use crate::sfu::Sfu;
use crate::utils::json_error::json_error;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::State;
use rocket::{futures::StreamExt, get};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use ws::{Channel, WebSocket};

// the name channel_id is referring to the channels in my servers, not the rocket channel
#[get("/chat_ws/<user_id>")]
#[allow(clippy::too_many_arguments)]
pub async fn websocket_handler(
    ws: WebSocket,
    user_id: i32,
    guard: JwtGuard,
    websocket_map_state: &State<SessionIdWebsocketMap>,
    server_map_state: &State<ServerSessionIdMap>,
    user_map_state: &State<UserSessionIdMap>,
    sfu: &State<Arc<Sfu>>,
    db: &State<HarmonyDb>,
) -> Result<Channel<'static>, Status> {
    if guard.0.sub != user_id {
        return Err(Status::Forbidden);
    }
    let session_id = Uuid::new_v4();
    let websocket_map = websocket_map_state.inner().clone();
    let server_map = server_map_state.0.clone();
    let user_map = user_map_state.0.clone();
    let sfu = Arc::clone(sfu);
    let pool = PgPool::clone(db);
    let servers = get_all_servers(user_id, &pool)
        .await
        .map_err(|_| Status::InternalServerError)?;

    // inbound messages are only call signaling; an SDP answer is a few KiB
    let limit = Some(256 << 10);
    let ws = ws.config(ws::Config {
        max_message_size: limit,
        max_frame_size: limit,
        ..Default::default()
    });
    Ok(ws.channel(move |stream| {
        let (sender, mut receiver) = stream.split();
        let sender = Arc::new(Mutex::new(sender));

        Box::pin(async move {
            // map session_id to websocket sender
            {
                let mut websocket_map = websocket_map.lock().await;
                websocket_map.insert(session_id, Arc::clone(&sender));
            }
            // add the websocket sender to every server the user is in
            {
                let mut server_map = server_map.lock().await;
                for server in servers.iter() {
                    let entry = server_map
                        .entry(server.server_id)
                        .or_insert_with(|| Arc::new(Mutex::new(Vec::new())));
                    let mut vec = entry.lock().await;
                    vec.push(session_id);
                }
            }
            // add session_id to user bucket
            {
                let mut user_map = user_map.lock().await;
                let entry = user_map
                    .entry(user_id)
                    .or_insert_with(|| Arc::new(Mutex::new(Vec::new())));
                let mut vec = entry.lock().await;
                vec.push(session_id);
            }

            // incoming messages are call signaling
            tokio::spawn(async move {
                while let Some(message) = receiver.next().await {
                    let text = match message {
                        Ok(ws::Message::Text(text)) => text,
                        Ok(_) => continue,
                        Err(e) => {
                            println!("Error in channel {:?}", e);
                            break;
                        }
                    };
                    let Ok(event) = serde_json::from_str::<CallClientEvent>(&text) else {
                        continue;
                    };
                    let result = match event {
                        CallClientEvent::JoinCall { channel_id } => {
                            if can_join(&pool, user_id, channel_id).await {
                                sfu.join(session_id, user_id, channel_id, Arc::clone(&sender))
                                    .await
                            } else {
                                println!("user {user_id} denied call in channel {channel_id}");
                                Ok(())
                            }
                        }
                        CallClientEvent::LeaveCall => {
                            sfu.leave(session_id, None).await;
                            Ok(())
                        }
                        CallClientEvent::SfuAnswer { sdp } => sfu.answer(session_id, sdp).await,
                        CallClientEvent::SfuCandidate(c) => sfu.candidate(session_id, c).await,
                    };
                    if let Err(e) = result {
                        eprintln!("call signaling error: {e}");
                    }
                }

                // Handle disconnection
                sfu.leave(session_id, None).await;
                // Remove session_id from websocket map
                {
                    let mut websocket_map = websocket_map.lock().await;
                    websocket_map.remove(&session_id);
                }
                // remove session_id from server map
                {
                    for server in servers.iter() {
                        let arc_mutex_vec = {
                            let server_map = server_map.lock().await;
                            server_map.get(&server.server_id).cloned()
                        };

                        if let Some(arc_mutex_vec) = arc_mutex_vec {
                            let mut vec = arc_mutex_vec.lock().await;
                            vec.retain(|id| *id != session_id);

                            if vec.is_empty() {
                                let mut server_map = server_map.lock().await;
                                server_map.remove(&server.server_id);
                            }
                        }
                    }
                }
                // remove session_id from user map
                {
                    let arc_mutex_vec = {
                        let user_map = user_map.lock().await;
                        user_map.get(&user_id).cloned()
                    };

                    if let Some(arc_mutex_vec) = arc_mutex_vec {
                        let mut vec = arc_mutex_vec.lock().await;
                        vec.retain(|id| *id != session_id);
                        if vec.is_empty() {
                            let mut user_map = user_map.lock().await;
                            user_map.remove(&user_id);
                        }
                    }
                }
            });

            Ok(())
        })
    }))
}

// user is a member of the server that owns the channel
async fn can_join(pool: &PgPool, user_id: i32, channel_id: i32) -> bool {
    sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM channels c
        JOIN users_servers us ON us.server_id = c.server_id
        WHERE c.channel_id = $1 AND us.user_id = $2",
    )
    .bind(channel_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .is_ok_and(|row| row.is_some())
}

// add user_id's session_ids to the server websocket map
pub async fn add_user_to_server_map(
    user_id: i32,
    server_id: i32,
    server_map_state: &rocket::State<ServerSessionIdMap>,
    user_map_state: &rocket::State<UserSessionIdMap>,
) {
    let server_map_state = server_map_state.0.clone();
    let user_map_state = user_map_state.0.clone();

    // add every session_id of user_id to the server map
    {
        let mut server_map = server_map_state.lock().await;
        let mut user_map = user_map_state.lock().await;
        let entry = user_map
            .entry(user_id)
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())));
        let user_session_ids = entry.lock().await;
        for user_session_id in user_session_ids.iter() {
            let entry = server_map
                .entry(server_id)
                .or_insert_with(|| Arc::new(Mutex::new(Vec::new())));
            let mut server_session_ids = entry.lock().await;
            server_session_ids.push(*user_session_id);
        }
    }
}

// maybe? remove user_id's session_ids to the server websocket map

// todo: move this somewhere else
async fn get_all_servers(
    user_id: i32,
    pool: &PgPool,
) -> Result<Vec<Server>, (Status, Json<ErrorResponse>)> {
    let servers = sqlx::query_as!(
        Server,
        r#"SELECT 
        s.server_id, s.server_type AS "server_type!: ServerType", members, server_name, admins, s3_icon_key, last_message_at, last_message_id
        FROM servers s
        JOIN users_servers us ON us.server_id = s.server_id
        WHERE us.user_id = $1"#,
        user_id,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| (Status::BadRequest, json_error("Database error")))?;

    Ok(servers)
}

#[cfg(test)]
mod tests {
    use crate::models::{CallClientEvent, IceCandidate};

    fn parse(json: &str) -> Option<CallClientEvent> {
        serde_json::from_str(json).ok()
    }

    #[test]
    fn parses_call_events() {
        assert_eq!(
            parse(r#"{"event_type":"JoinCall","data":{"channel_id":7}}"#),
            Some(CallClientEvent::JoinCall { channel_id: 7 })
        );
        assert_eq!(
            parse(r#"{"event_type":"LeaveCall"}"#),
            Some(CallClientEvent::LeaveCall)
        );
        assert_eq!(
            parse(r#"{"event_type":"SfuAnswer","data":{"sdp":"v=0"}}"#),
            Some(CallClientEvent::SfuAnswer { sdp: "v=0".into() })
        );
        assert_eq!(
            parse(
                r#"{"event_type":"SfuCandidate","data":{"candidate":"c","sdp_mid":"0","sdp_mline_index":0}}"#
            ),
            Some(CallClientEvent::SfuCandidate(IceCandidate {
                candidate: "c".into(),
                sdp_mid: Some("0".into()),
                sdp_mline_index: Some(0),
            }))
        );
        // old p2p events and garbage are rejected
        assert_eq!(
            parse(r#"{"event_type":"Offer","data":{"sdp":"x","to":1,"from":2}}"#),
            None
        );
        assert_eq!(
            parse(r#"{"event_type":"JoinCall","data":{"channel_id":"7"}}"#),
            None
        );
        assert_eq!(parse("not json"), None);
    }
}
