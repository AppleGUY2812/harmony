use crate::middleware::protect_route::JwtGuard;
use crate::models::{MessageWithServer, SentMessage, WebSocketEvent};
use crate::utils::hub::Hub;
use crate::{
    database,
    models::{self, ErrorResponse},
    utils::{self, json_error::json_error},
};

use rocket::http::Status;

use rocket::serde::json::Json;

use rocket_db_pools::Connection;
use std::sync::Arc;
use utils::user_membership::{ByChannel, UserMembershipChecker};
use validator::Validate;

// One statement (its own implicit transaction): membership check, server_id/server_type
// derived from the channel, author name/icon from users, insert, and one servers UPDATE.
// No row back = channel missing or user not a member.
// GREATEST keeps last_message_* monotonic when concurrent sends commit out of order.
const SEND_MESSAGE_SQL: &str = "
WITH ch AS (
    SELECT c.channel_id, s.server_id, s.server_type
    FROM channels c
    JOIN users_servers us ON us.server_id = c.server_id AND us.user_id = $1
    JOIN servers s ON s.server_id = c.server_id
    WHERE c.channel_id = $2
), m AS (
    INSERT INTO messages (user_id, channel_id, message, display_username, s3_icon_key)
    SELECT $1, ch.channel_id, $3, u.display_username, u.s3_icon_key
    FROM ch JOIN users u ON u.user_id = $1
    RETURNING *
), upd AS (
    UPDATE servers
    SET last_message_at = GREATEST(servers.last_message_at, m.timestamp),
        last_message_id = GREATEST(servers.last_message_id, m.message_id)
    FROM m, ch
    WHERE servers.server_id = ch.server_id
)
SELECT m.*, ch.server_id, ch.server_type FROM m, ch";

#[post("/send", format = "json", data = "<message_json>")]
pub async fn send_message(
    guard: JwtGuard,
    message_json: Json<models::SendMessageInput>,
    mut db: Connection<database::HarmonyDb>,
    hub: &rocket::State<Arc<Hub>>,
) -> Result<(Status, Json<models::Message>), (Status, Json<ErrorResponse>)> {
    message_json
        .validate()
        .map_err(|_| (Status::BadRequest, json_error("invalid message")))?;

    let sent = sqlx::query_as::<_, SentMessage>(SEND_MESSAGE_SQL)
        .bind(guard.0.sub)
        .bind(message_json.channel_id)
        .bind(&message_json.message)
        .fetch_optional(&mut **db)
        .await
        .map_err(|_| (Status::InternalServerError, json_error("database error")))?
        .ok_or((Status::Forbidden, json_error("user not in server")))?;

    let server_id = sent.server_id;
    let event = WebSocketEvent::Message(MessageWithServer {
        message: sent.message.clone(),
        server_type: sent.server_type,
        server_id,
        nonce: message_json.into_inner().nonce,
    });
    let event_json = serde_json::to_string(&event).map_err(|_| {
        (
            Status::InternalServerError,
            json_error("serialization error"),
        )
    })?;

    // broadcast message to everyone in the server
    hub.send_to_server(server_id, &event_json);

    Ok((Status::Created, Json(sent.message)))
}

// newest page (or the page older than `before`), returned in ascending order.
// Served by idx_messages_channel_message: backward index scan, stops after `limit` rows.
#[get("/get/<channel_id>?<before>&<limit>")]
pub async fn get_messages(
    guard: JwtGuard,
    channel_id: i32,
    before: Option<i64>,
    limit: Option<i64>,
    mut db: Connection<database::HarmonyDb>,
) -> Result<Json<Vec<models::Message>>, (Status, Json<ErrorResponse>)> {
    let user_id = &guard.0.sub;

    let channel_checker = ByChannel { channel_id };
    channel_checker.user_in_server(&mut db, *user_id).await?;

    let messages = sqlx::query_as::<_, models::Message>(
        "SELECT * FROM (
            SELECT * FROM messages
            WHERE channel_id = $1 AND message_id < $2
            ORDER BY message_id DESC
            LIMIT $3
        ) page ORDER BY message_id ASC",
    )
    .bind(channel_id)
    .bind(before.unwrap_or(i64::MAX))
    .bind(limit.unwrap_or(50).clamp(1, 100))
    .fetch_all(&mut **db)
    .await
    .map_err(|_| (Status::InternalServerError, json_error("database error")))?;

    Ok(Json(messages))
}
