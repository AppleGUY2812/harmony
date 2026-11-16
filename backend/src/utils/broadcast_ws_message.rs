use crate::models::{ServerSessionIdMap, SessionIdWebsocketMap, UserSessionIdMap};
use rocket::futures::SinkExt;

// websocket: broadcasts given message to everyone in the server
pub async fn broadcast_to_server(
    message: &str,
    server_id: i32,
    websocket_map_state: SessionIdWebsocketMap,
    server_map_state: ServerSessionIdMap,
) {
    let websocket_map = websocket_map_state.lock().await;
    let server_map = server_map_state.0.lock().await;

    if let Some(arc_mutex_vec) = server_map.get(&server_id) {
        let senders_session_id = arc_mutex_vec.lock().await;
        for sender_session_id in senders_session_id.iter() {
            if let Some(sender_arc_mutex) = websocket_map.get(sender_session_id) {
                let mut sender = sender_arc_mutex.lock().await;
                if let Err(e) = sender.send(message.into()).await {
                    eprintln!("Error sending message: {:?}", e);
                }
            }
        }
    }
}

// for dmcreated event
pub async fn broadcast_to_users(
    message: &str,
    user_ids: &[i32],
    websocket_map_state: SessionIdWebsocketMap,
    user_map_state: UserSessionIdMap,
) {
    let websocket_map = websocket_map_state.lock().await;
    let user_map = user_map_state.0.lock().await;

    for user_id in user_ids.iter() {
        if let Some(arc_mutex_vec) = user_map.get(user_id) {
            let senders_session_id = arc_mutex_vec.lock().await;
            for sender_session_id in senders_session_id.iter() {
                if let Some(sender_arc_mutex) = websocket_map.get(sender_session_id) {
                    let mut sender = sender_arc_mutex.lock().await;
                    if let Err(e) = sender.send(message.into()).await {
                        eprintln!("Error sending message: {:?}", e);
                    }
                }
            }
        }
    }
}
