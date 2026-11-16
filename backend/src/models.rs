use once_cell::sync::Lazy;
use regex::Regex;
use rocket::data::{Capped, ToByteUnit};
use rocket::form::{self, DataField, FromFormField};
use rocket::serde::{Deserialize, Serialize};

use uuid::Uuid;
use validator::Validate;

use chrono::{DateTime, NaiveDate, Utc};

#[derive(Serialize, Debug)]
pub struct ErrorResponse {
    pub error: String,
}

static ALPHA_NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9_ඞ]+$").unwrap());

#[derive(Deserialize, Validate)]
pub struct UserSignupInput<'r> {
    // email, username, password, confirmPassword
    #[validate(email)]
    pub email: &'r str,
    #[validate(length(min = 1, max = 15), regex(path = *ALPHA_NUM))]
    pub username: &'r str,
    #[validate(length(min = 1, max = 20))]
    pub password: &'r str,
    #[validate(length(min = 1, max = 20))]
    pub confirm_password: &'r str,
}

#[derive(Deserialize)]
pub struct UserLoginInput<'r> {
    pub username: &'r str,
    pub password: &'r str,
}

#[derive(Serialize)]
pub struct User {
    pub user_id: i32,
    pub display_username: String,
    pub s3_icon_key: Option<String>,
    pub date_joined: NaiveDate,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: i32,
    pub iat: usize, // Optional. Issued at (as UTC timestamp)
    pub exp: usize, // Required (validate_exp defaults to true in validation). Expiration time (as UTC timestamp)
}

// ponytail: old clients also send server_id, server_type, display_username and
// s3_icon_key; serde ignores unknown fields, so they still deserialize and are never trusted.
#[derive(Deserialize, Validate)]
pub struct SendMessageInput {
    pub channel_id: i32,
    #[validate(length(min = 1, max = 600))]
    pub message: String,
    // client-generated id echoed back in the websocket event for optimistic-send matching
    #[validate(length(max = 64))]
    pub nonce: Option<String>,
}

#[derive(Serialize, Clone, sqlx::FromRow)]
pub struct Message {
    pub message_id: i64,
    pub user_id: i32,
    pub channel_id: i32,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub display_username: String,
    pub s3_icon_key: Option<String>,
}

// row returned by the send_message CTE
#[derive(sqlx::FromRow)]
pub struct SentMessage {
    #[sqlx(flatten)]
    pub message: Message,
    pub server_id: i32,
    pub server_type: ServerType,
}

#[derive(Serialize, Clone)]
pub struct MessageWithServer {
    pub message: Message,
    pub server_type: ServerType,
    pub server_id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

#[derive(sqlx::Type, Deserialize, Serialize, FromFormField, Clone, Copy)]
#[sqlx(type_name = "server_type", rename_all = "lowercase")]
pub enum ServerType {
    Dm,
    GroupChat,
    Server,
}

// for sqlx queries
#[derive(Serialize, Clone)]
pub struct Server {
    pub server_id: i32,
    pub server_type: ServerType,
    pub members: i32,
    pub server_name: String,
    pub admins: Vec<i32>,
    pub s3_icon_key: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_id: Option<i64>,
}
#[derive(Serialize, Clone)]
pub struct ServerWithUnreadMessages {
    pub server_id: i32,
    pub server_type: ServerType,
    pub members: i32,
    pub server_name: String,
    pub admins: Vec<i32>,
    pub s3_icon_key: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_id: Option<i64>,
    pub unread_messages: i64,
}

#[derive(Serialize, Clone)]
pub struct Channel {
    pub channel_id: i32,
    pub server_id: i32,
    pub channel_name: String,
}

pub struct S3File {
    pub key: String,
    pub data: Capped<Vec<u8>>,
}

#[rocket::async_trait]
impl<'r> FromFormField<'r> for S3File {
    async fn from_data(field: DataField<'r, '_>) -> form::Result<'r, Self> {
        // will need to add folder in front of the name
        let file_name = format!("{}", Uuid::new_v4());
        let bytes = field.data.open(10.mebibytes()).into_bytes().await?;
        if !bytes.is_complete() {
            return Err(form::Error::validation("image larger than 10 MiB").into());
        }

        Ok(S3File {
            key: file_name,
            data: bytes,
        })
    }
}

#[derive(FromForm, Validate)]
pub struct CreateServerInput {
    pub recipient_ids: Vec<i32>,
    pub server_type: ServerType,
    #[validate(length(min = 1, max = 30))]
    pub server_name: String,
    pub server_icon: Option<S3File>,
}

#[derive(Serialize)]
pub struct ServerChannel {
    pub server: Server,
    // default channel
    pub channel: Channel,
}

#[derive(Serialize)]
pub struct OptionalServerChannel {
    pub server: Option<Server>,
    // default channel
    pub channel: Option<Channel>,
}

#[derive(FromForm, Validate)]
pub struct EditServerInput {
    #[validate(length(min = 1, max = 30))]
    pub server_name: Option<String>,
    pub server_icon: Option<S3File>,
}

#[derive(FromForm, Validate)]
pub struct EditUserInput {
    pub user_icon: Option<S3File>,
}

#[derive(Deserialize)]
pub struct ServerIds {
    pub server_ids: Vec<i32>,
}

#[derive(Deserialize, Validate)]
pub struct CreateChannelInput {
    pub server_id: i32,
    #[validate(length(min = 1, max = 30))]
    pub channel_name: String,
}
// websocket stuff

use rocket::futures::stream::SplitSink;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use ws::{stream::DuplexStream, Message as WsMessage};

// maps session_id to list of websocket send connections
pub type SessionIdWebsocketMap =
    Arc<Mutex<HashMap<Uuid, Arc<Mutex<SplitSink<DuplexStream, WsMessage>>>>>>;
pub fn new_sessionid_websocket_map() -> SessionIdWebsocketMap {
    Arc::new(Mutex::new(HashMap::new()))
}

// map server_id to list of session_ids
pub struct ServerSessionIdMap(pub Arc<Mutex<HashMap<i32, Arc<Mutex<Vec<Uuid>>>>>>);
pub fn new_server_sessionid_map() -> ServerSessionIdMap {
    ServerSessionIdMap(Arc::new(Mutex::new(HashMap::new())))
}
// map user_id to list of session_ids
pub struct UserSessionIdMap(pub Arc<Mutex<HashMap<i32, Arc<Mutex<Vec<Uuid>>>>>>);
pub fn new_user_sessionid_map() -> UserSessionIdMap {
    UserSessionIdMap(Arc::new(Mutex::new(HashMap::new())))
}

// websocket events
#[derive(Serialize)]
#[serde(tag = "event_type", content = "data")]
pub enum WebSocketEvent {
    Message(MessageWithServer),
    UserJoin(UserJoin),
    ServerCreated(ServerCreated),
}

// call signaling, client -> server
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "event_type", content = "data")]
pub enum CallClientEvent {
    JoinCall { channel_id: i32 },
    LeaveCall,
    SfuAnswer { sdp: String },
    SfuCandidate(IceCandidate),
}

// call signaling, server -> client
#[derive(Serialize)]
#[serde(tag = "event_type", content = "data")]
pub enum CallServerEvent {
    SfuOffer {
        sdp: String,
    },
    SfuCandidate(IceCandidate),
    CallParticipants {
        channel_id: i32,
        participants: Vec<CallParticipant>,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
}

#[derive(Serialize)]
pub struct CallParticipant {
    pub user_id: i32,
    // msid of this participant's tracks, so clients can label remote streams
    pub stream_id: String,
}

#[derive(Serialize)]
pub struct UserJoin {
    pub user: User,
    pub server_id: i32,
}

#[derive(Serialize)]
pub struct ServerCreated {
    pub server: Server,
    pub channel: Channel,
    pub users: Vec<User>,
}
