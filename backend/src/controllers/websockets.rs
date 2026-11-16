use crate::database::HarmonyDb;
use crate::middleware::protect_route::JwtGuard;
use crate::models::CallClientEvent;
use crate::sfu::Sfu;
use crate::utils::hub::Hub;
use rocket::futures::{SinkExt, StreamExt};
use rocket::get;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::State;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, timeout, Instant};
use ws::{Channel, Message, WebSocket};

const PING_EVERY: Duration = Duration::from_secs(25);
// nothing received (pongs included) for this long, or a send stuck this long: drop the socket
const IDLE: Duration = Duration::from_secs(60);

// the name channel_id is referring to the channels in my servers, not the rocket channel
#[get("/chat_ws/<user_id>")]
pub async fn websocket_handler(
    ws: WebSocket,
    user_id: i32,
    _origin: SameHost,
    guard: JwtGuard,
    hub: &State<Arc<Hub>>,
    sfu: &State<Arc<Sfu>>,
    db: &State<HarmonyDb>,
) -> Result<Channel<'static>, Status> {
    if guard.0.sub != user_id {
        return Err(Status::Unauthorized);
    }
    let hub = Arc::clone(hub);
    let sfu = Arc::clone(sfu);
    let pool = PgPool::clone(db);

    // inbound messages are only call signaling; an SDP answer is a few KiB
    let limit = Some(256 << 10);
    let ws = ws.config(ws::Config {
        max_message_size: limit,
        max_frame_size: limit,
        ..Default::default()
    });
    Ok(ws.channel(move |mut stream| {
        Box::pin(async move {
            // registered before the server list is read, so no join or leave is missed
            let (session_id, mut outbox) = hub.connect(user_id);
            if subscribe(&hub, session_id, user_id, &pool).await.is_err() {
                hub.disconnect(session_id);
                return Ok(());
            }
            let mut ping = interval(PING_EVERY);
            let mut last_seen = Instant::now();

            loop {
                let out = tokio::select! {
                    out = outbox.recv() => match out {
                        Some(out) => out,
                        // the hub dropped this session
                        None => break,
                    },
                    _ = ping.tick() => {
                        if last_seen.elapsed() > IDLE {
                            break;
                        }
                        Message::Ping(Vec::new())
                    }
                    incoming = stream.next() => {
                        let Some(Ok(incoming)) = incoming else { break };
                        last_seen = Instant::now();
                        if let Message::Text(text) = incoming {
                            handle_call_event(&text, session_id, user_id, &hub, &sfu, &pool).await;
                        }
                        continue;
                    }
                };
                if !matches!(timeout(IDLE, stream.send(out)).await, Ok(Ok(()))) {
                    break;
                }
            }

            hub.disconnect(session_id);
            sfu.leave(session_id, None).await;
            Ok(())
        })
    }))
}

// read again when a leave lands between the read and subscribing, so an ex-member is never subscribed
async fn subscribe(
    hub: &Hub,
    session_id: uuid::Uuid,
    user_id: i32,
    pool: &PgPool,
) -> sqlx::Result<()> {
    loop {
        let leaves = hub.leaves();
        let server_ids =
            sqlx::query_scalar::<_, i32>("SELECT server_id FROM users_servers WHERE user_id = $1")
                .bind(user_id)
                .fetch_all(pool)
                .await?;
        if hub.subscribe(session_id, &server_ids, leaves) {
            return Ok(());
        }
    }
}

// The JWT cookie is SameSite=None, so any site's page could open this socket with it
// (cross-site websocket hijacking). Browsers always send Origin on an upgrade: it must be
// this host. The port is ignored because dev serves the page from vite on another port.
pub struct SameHost;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for SameHost {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let headers = req.headers();
        match headers.get_one("Origin") {
            // not a browser
            None => Outcome::Success(SameHost),
            Some(origin) if same_host(origin, headers.get_one("Host").unwrap_or("")) => {
                Outcome::Success(SameHost)
            }
            Some(_) => Outcome::Error((Status::Forbidden, ())),
        }
    }
}

fn same_host(origin: &str, host: &str) -> bool {
    let origin = hostname(origin.split_once("://").map_or("", |(_, rest)| rest));
    let host = hostname(host);
    let loopback = |h: &str| matches!(h, "localhost" | "127.0.0.1" | "::1");
    !origin.is_empty()
        && (origin.eq_ignore_ascii_case(host) || (loopback(origin) && loopback(host)))
}

// "host:port" or "[v6]:port" -> host
fn hostname(authority: &str) -> &str {
    match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.split(':').next().unwrap_or(""),
    }
}

// incoming messages are call signaling
async fn handle_call_event(
    text: &str,
    session_id: uuid::Uuid,
    user_id: i32,
    hub: &Arc<Hub>,
    sfu: &Arc<Sfu>,
    pool: &PgPool,
) {
    let Ok(event) = serde_json::from_str::<CallClientEvent>(text) else {
        return;
    };
    let result = match event {
        CallClientEvent::JoinCall { channel_id } => {
            if can_join(pool, user_id, channel_id).await {
                sfu.join(session_id, user_id, channel_id, Arc::clone(hub))
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

#[cfg(test)]
mod tests {
    use super::same_host;
    use crate::models::{CallClientEvent, IceCandidate};

    #[test]
    fn origin_must_be_this_host() {
        assert!(same_host("https://harmonyapp.xyz", "harmonyapp.xyz"));
        assert!(same_host("https://10.0.0.5", "10.0.0.5:443"));
        assert!(same_host("http://localhost:3000", "127.0.0.1:5500"));
        assert!(same_host("http://[::1]:3000", "[::1]:5500"));
        assert!(!same_host("https://evil.com", "harmonyapp.xyz"));
        assert!(!same_host(
            "https://harmonyapp.xyz.evil.com",
            "harmonyapp.xyz"
        ));
        assert!(!same_host("http://localhost:3000", "harmonyapp.xyz"));
        assert!(!same_host("null", ""));
    }

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
