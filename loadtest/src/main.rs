// Load test for the harmony SFU: N bots sign up through the real HTTP API, join one
// server, open the real websocket, JoinCall on the server's channel, publish one
// synthetic audio + one synthetic VP8 track and subscribe to everyone else.
//
//   cargo run --release -- --n 50 --duration-secs 60 --api http://localhost:5500 --ws ws://localhost:5500

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use std::error::Error;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_OPUS, MIME_TYPE_VP8};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::{APIBuilder, API};
use webrtc::ice::mdns::MulticastDnsMode;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

type BoxError = Box<dyn Error + Send + Sync>;

const AUDIO_INTERVAL: Duration = Duration::from_millis(20); // 50 pps
const AUDIO_BYTES: usize = 100;
const VIDEO_INTERVAL: Duration = Duration::from_micros(33_333); // 30 fps
const VIDEO_BYTES: usize = 1150; // one RTP packet per frame at the default 1200 MTU

struct Args {
    n: usize,
    duration_secs: u64,
    api: String,
    ws: String,
}

fn parse_args() -> Args {
    let mut args = Args {
        n: 50,
        duration_secs: 60,
        api: "http://localhost:5500".into(),
        ws: "ws://localhost:5500".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().unwrap_or_else(|| usage(&flag));
        match flag.as_str() {
            "--n" => args.n = value.parse().unwrap_or_else(|_| usage(&flag)),
            "--duration-secs" => {
                args.duration_secs = value.parse().unwrap_or_else(|_| usage(&flag))
            }
            "--api" => args.api = value,
            "--ws" => args.ws = value,
            _ => usage(&flag),
        }
    }
    if args.n < 2 {
        usage("--n");
    }
    args
}

fn usage(flag: &str) -> ! {
    eprintln!("bad flag {flag}\nusage: loadtest [--n 50] [--duration-secs 60] [--api http://localhost:5500] [--ws ws://localhost:5500]");
    std::process::exit(2)
}

// RTP sequence tracking for one received track; loss = gaps in the sequence numbers
#[derive(Default)]
struct Seq {
    first: u64,
    highest: u64,
    received: u64,
}

impl Seq {
    fn push(&mut self, seq: u16) {
        if self.received == 0 {
            // start high so a wrap backwards never goes negative
            self.first = (1 << 32) + seq as u64;
            self.highest = self.first;
        } else {
            let delta = seq.wrapping_sub(self.highest as u16) as i16 as i64;
            self.highest = self.highest.max((self.highest as i64 + delta) as u64);
        }
        self.received += 1;
    }

    fn expected(&self) -> u64 {
        if self.received == 0 {
            0
        } else {
            self.highest - self.first + 1
        }
    }

    fn lost(&self) -> u64 {
        self.expected().saturating_sub(self.received)
    }
}

// one-way latency in 1 ms buckets (publisher and subscriber share this process's clock)
struct Latency {
    from: Duration,       // only packets received after this count (steady state)
    hist: Vec<AtomicU64>, // one bucket per ms of the run, so nothing real is capped
}

impl Latency {
    fn record(&self, start: Instant, payload: &[u8]) {
        let now = start.elapsed();
        let Some(sent) = payload.len().checked_sub(8).map(|i| &payload[i..]) else {
            return;
        };
        if now < self.from {
            return;
        }
        let sent = Duration::from_micros(u64::from_be_bytes(sent.try_into().unwrap()));
        let ms = now.saturating_sub(sent).as_millis() as usize;
        self.hist[ms.min(self.hist.len() - 1)].fetch_add(1, Ordering::Relaxed);
    }

    fn percentile(&self, q: f64) -> Option<f64> {
        let counts: Vec<u64> = self
            .hist
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect();
        let total: u64 = counts.iter().sum();
        if total == 0 {
            return None;
        }
        let rank = ((total - 1) as f64 * q).round() as u64;
        let mut seen = 0;
        counts
            .iter()
            .position(|&c| {
                seen += c;
                seen > rank
            })
            .map(|ms| ms as f64)
    }
}

#[derive(Default)]
struct BotStats {
    join: Mutex<Option<Duration>>,       // JoinCall sent -> pc connected
    all_tracks: Mutex<Option<Duration>>, // call start -> saw every remote track
    tracks: AtomicUsize,
    packets: AtomicU64,
    sent: AtomicU64, // RTP packets this bot published
    seqs: Mutex<Vec<Arc<Mutex<Seq>>>>,
}

struct Account {
    user_id: i64,
    jwt: String,
}

#[derive(Serialize)]
struct BotReport {
    bot: usize,
    join_ms: Option<f64>,
    tracks: usize,
    all_tracks_ms: Option<f64>,
    packets: u64,
    lost: u64,
    expected: u64,
    error: Option<String>,
}

#[derive(Serialize)]
struct Report {
    n: usize,
    duration_secs: u64,
    bots_connected: usize,
    join_ms_p50: Option<f64>,
    join_ms_p95: Option<f64>,
    join_ms_max: Option<f64>,
    expected_tracks_per_bot: usize,
    bots_with_all_tracks: usize,
    all_bots_all_tracks_ms: Option<f64>,
    packets_per_sec: f64,
    expected_packets_per_sec: f64,
    sent_packets_per_sec: f64, // all publishers together, over the whole run
    loss_pct: f64,
    // one-way latency of packets received in the second half of the run
    latency_ms_p50: Option<f64>,
    latency_ms_p95: Option<f64>,
    latency_ms_p99: Option<f64>,
    latency_ms_max: Option<f64>,
    // total packets received across bots, sampled every second since call start
    packets_timeline: Vec<u64>,
    bots: Vec<BotReport>,
}

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    let args = parse_args();
    let http = reqwest::Client::new();
    let run = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    // 1. accounts, one server, everyone in it
    eprintln!("signing up {} bots", args.n);
    let mut signups = Vec::new();
    for i in 0..args.n {
        // username max 15 chars: "lt" + 6 hex + index
        let username = format!("lt{:06x}{i}", run & 0xff_ffff);
        signups.push(tokio::spawn(signup(
            http.clone(),
            args.api.clone(),
            username,
        )));
    }
    let mut accounts = Vec::new();
    for s in signups {
        accounts.push(s.await??);
    }

    let created: Value = authed(
        http.post(format!("{}/api/servers/create", args.api)),
        &accounts[0],
    )
    .form(&[("server_type", "Server"), ("server_name", "loadtest")])
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
    let server_id = created["server"]["server_id"]
        .as_i64()
        .ok_or("no server_id")?;
    let channel_id = created["channel"]["channel_id"]
        .as_i64()
        .ok_or("no channel_id")?;
    for account in &accounts[1..] {
        authed(
            http.post(format!("{}/api/servers/join/{server_id}", args.api)),
            account,
        )
        .send()
        .await?
        .error_for_status()?;
    }
    eprintln!("server {server_id} channel {channel_id}: starting call");

    // 2. the call
    let api = Arc::new(bot_api()?);
    let expected_tracks = 2 * (args.n - 1);
    let (stop_tx, stop_rx) = watch::channel(false);
    let start = Instant::now();
    let stats: Vec<Arc<BotStats>> = (0..args.n).map(|_| Arc::default()).collect();
    let latency = Arc::new(Latency {
        from: Duration::from_secs(args.duration_secs / 2),
        hist: (0..=args.duration_secs * 1000)
            .map(|_| AtomicU64::new(0))
            .collect(),
    });
    let mut bots = Vec::new();
    for (i, account) in accounts.into_iter().enumerate() {
        bots.push(tokio::spawn(run_bot(
            Arc::clone(&api),
            args.ws.clone(),
            account,
            channel_id,
            start,
            expected_tracks,
            Arc::clone(&stats[i]),
            Arc::clone(&latency),
            stop_rx.clone(),
        )));
    }

    let mut timeline = Vec::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.tick().await;
    for _ in 0..args.duration_secs {
        tick.tick().await;
        let total: u64 = stats
            .iter()
            .map(|s| s.packets.load(Ordering::Relaxed))
            .sum();
        let tracks: usize = stats.iter().map(|s| s.tracks.load(Ordering::Relaxed)).sum();
        eprintln!(
            "t={:>3}s tracks {tracks}/{} packets {total}",
            timeline.len() + 1,
            expected_tracks * args.n
        );
        timeline.push(total);
    }
    let _ = stop_tx.send(true);
    let mut errors = Vec::new();
    for bot in bots {
        errors.push(bot.await?.err().map(|e| e.to_string()));
    }

    // 3. report
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let bot_reports: Vec<BotReport> = stats
        .iter()
        .zip(errors)
        .enumerate()
        .map(|(i, (s, error))| {
            let seqs = s.seqs.lock().unwrap();
            BotReport {
                bot: i,
                join_ms: s.join.lock().unwrap().map(ms),
                tracks: s.tracks.load(Ordering::Relaxed),
                all_tracks_ms: s.all_tracks.lock().unwrap().map(ms),
                packets: s.packets.load(Ordering::Relaxed),
                lost: seqs.iter().map(|q| q.lock().unwrap().lost()).sum(),
                expected: seqs.iter().map(|q| q.lock().unwrap().expected()).sum(),
                error,
            }
        })
        .collect();

    let mut joins: Vec<f64> = bot_reports.iter().filter_map(|b| b.join_ms).collect();
    joins.sort_by(f64::total_cmp);
    let pct = |q: f64| {
        (!joins.is_empty()).then(|| joins[((joins.len() - 1) as f64 * q).round() as usize])
    };
    let with_all = bot_reports
        .iter()
        .filter(|b| b.all_tracks_ms.is_some())
        .count();
    let all_bots_all_tracks_ms = (with_all == args.n).then(|| {
        bot_reports
            .iter()
            .filter_map(|b| b.all_tracks_ms)
            .fold(0.0, f64::max)
    });
    // steady state: from the second after everyone had every track (or the second half)
    let from = all_bots_all_tracks_ms
        .map(|t| (t / 1000.0).ceil() as usize)
        .unwrap_or(timeline.len() / 2)
        .min(timeline.len().saturating_sub(1));
    let packets_per_sec = match (timeline.get(from), timeline.last()) {
        (Some(a), Some(b)) if timeline.len() - 1 > from => {
            (b - a) as f64 / (timeline.len() - 1 - from) as f64
        }
        _ => 0.0,
    };
    let per_publisher_pps = 1.0 / AUDIO_INTERVAL.as_secs_f64() + 1.0 / VIDEO_INTERVAL.as_secs_f64();
    let sent: u64 = stats.iter().map(|s| s.sent.load(Ordering::Relaxed)).sum();
    let lost: u64 = bot_reports.iter().map(|b| b.lost).sum();
    let expected: u64 = bot_reports.iter().map(|b| b.expected).sum();

    let report = Report {
        n: args.n,
        duration_secs: args.duration_secs,
        bots_connected: joins.len(),
        join_ms_p50: pct(0.5),
        join_ms_p95: pct(0.95),
        join_ms_max: joins.last().copied(),
        expected_tracks_per_bot: expected_tracks,
        bots_with_all_tracks: with_all,
        all_bots_all_tracks_ms,
        packets_per_sec,
        expected_packets_per_sec: (args.n * (args.n - 1)) as f64 * per_publisher_pps,
        sent_packets_per_sec: sent as f64 / args.duration_secs as f64,
        loss_pct: if expected == 0 {
            0.0
        } else {
            100.0 * lost as f64 / expected as f64
        },
        latency_ms_p50: latency.percentile(0.5),
        latency_ms_p95: latency.percentile(0.95),
        latency_ms_p99: latency.percentile(0.99),
        latency_ms_max: latency.percentile(1.0),
        packets_timeline: timeline,
        bots: bot_reports,
    };

    print_summary(&report);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("results");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{run}-n{}.json", args.n));
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

fn print_summary(r: &Report) {
    let f = |v: Option<f64>| v.map_or("-".to_string(), |v| format!("{v:.0}"));
    let track_counts: Vec<usize> = r.bots.iter().map(|b| b.tracks).collect();
    println!("## SFU load test: {} bots, {}s\n", r.n, r.duration_secs);
    println!("| metric | value |");
    println!("|---|---|");
    println!("| bots connected | {}/{} |", r.bots_connected, r.n);
    println!(
        "| join ms (JoinCall -> connected) p50 / p95 / max | {} / {} / {} |",
        f(r.join_ms_p50),
        f(r.join_ms_p95),
        f(r.join_ms_max)
    );
    println!(
        "| bots with all {} remote tracks | {}/{} |",
        r.expected_tracks_per_bot, r.bots_with_all_tracks, r.n
    );
    println!(
        "| remote tracks per bot min / max | {} / {} |",
        track_counts.iter().min().unwrap_or(&0),
        track_counts.iter().max().unwrap_or(&0)
    );
    println!(
        "| time until every bot saw all tracks (ms) | {} |",
        f(r.all_bots_all_tracks_ms)
    );
    println!(
        "| RTP packets received/s (steady) | {:.0} (expected {:.0}) |",
        r.packets_per_sec, r.expected_packets_per_sec
    );
    println!(
        "| RTP packets published/s (all bots) | {:.0} (expected {:.0}) |",
        r.sent_packets_per_sec,
        r.n as f64 * (1.0 / AUDIO_INTERVAL.as_secs_f64() + 1.0 / VIDEO_INTERVAL.as_secs_f64())
    );
    println!("| loss from sequence gaps | {:.2}% |", r.loss_pct);
    println!(
        "| one-way latency ms, 2nd half, p50 / p95 / p99 / max | {} / {} / {} / {} |",
        f(r.latency_ms_p50),
        f(r.latency_ms_p95),
        f(r.latency_ms_p99),
        f(r.latency_ms_max)
    );
    let failed: Vec<_> = r
        .bots
        .iter()
        .filter_map(|b| b.error.as_ref().map(|e| format!("bot {}: {e}", b.bot)))
        .collect();
    if !failed.is_empty() {
        println!("\nerrors:\n- {}", failed.join("\n- "));
    }
}

fn authed(req: reqwest::RequestBuilder, account: &Account) -> reqwest::RequestBuilder {
    req.header("Cookie", format!("JWT={}", account.jwt))
}

async fn signup(http: reqwest::Client, api: String, username: String) -> Result<Account, BoxError> {
    let res = http
        .post(format!("{api}/api/auth/signup"))
        .json(&json!({
            "email": format!("{username}@loadtest.example"),
            "username": username,
            "password": "loadtest-pw",
            "confirm_password": "loadtest-pw",
        }))
        .send()
        .await?;
    // the cookie is Secure, so read it by hand instead of using a cookie jar over http
    let jwt = res
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|c| c.strip_prefix("JWT="))
        .and_then(|c| c.split(';').next())
        .map(str::to_owned);
    let status = res.status();
    let body: Value = res.json().await?;
    match (jwt, body["user_id"].as_i64()) {
        (Some(jwt), Some(user_id)) => Ok(Account { user_id, jwt }),
        _ => Err(format!("signup {username}: {status} {body}").into()),
    }
}

fn bot_api() -> Result<API, BoxError> {
    let mut media = MediaEngine::default();
    media.register_default_codecs()?;
    let registry = register_default_interceptors(Registry::new(), &mut media)?;
    let mut settings = SettingEngine::default();
    settings.set_ice_multicast_dns_mode(MulticastDnsMode::Disabled);
    Ok(APIBuilder::new()
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_setting_engine(settings)
        .build())
}

#[allow(clippy::too_many_arguments)]
async fn run_bot(
    api: Arc<API>,
    ws_base: String,
    account: Account,
    channel_id: i64,
    start: Instant,
    expected_tracks: usize,
    stats: Arc<BotStats>,
    latency: Arc<Latency>,
    mut stop: watch::Receiver<bool>,
) -> Result<(), BoxError> {
    let mut request = format!("{ws_base}/ws/chat_ws/{}", account.user_id).into_client_request()?;
    request
        .headers_mut()
        .insert("Cookie", format!("JWT={}", account.jwt).parse()?);
    let (socket, _) = tokio_tungstenite::connect_async(request).await?;
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if ws_tx
                .send(Message::Text(event.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
        let _ = ws_tx.close().await;
    });

    let pc = Arc::new(api.new_peer_connection(RTCConfiguration::default()).await?);
    let stream_id = format!("bot{}", account.user_id);
    let audio = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: 48000,
            channels: 2,
            ..Default::default()
        },
        "audio".into(),
        stream_id.clone(),
    ));
    let video = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_VP8.to_owned(),
            clock_rate: 90000,
            ..Default::default()
        },
        "video".into(),
        stream_id,
    ));
    pc.add_track(audio.clone()).await?;
    pc.add_track(video.clone()).await?;

    let candidate_tx = tx.clone();
    pc.on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
        if let Some(Ok(c)) = c.map(|c| c.to_json()) {
            let _ = candidate_tx.send(json!({
                "event_type": "SfuCandidate",
                "data": {"candidate": c.candidate, "sdp_mid": c.sdp_mid, "sdp_mline_index": c.sdp_mline_index},
            }));
        }
        Box::pin(async {})
    }));

    let joined = Instant::now();
    let join_stats = Arc::clone(&stats);
    pc.on_peer_connection_state_change(Box::new(move |s| {
        if s == RTCPeerConnectionState::Connected {
            join_stats
                .join
                .lock()
                .unwrap()
                .get_or_insert(joined.elapsed());
        }
        Box::pin(async {})
    }));

    let track_stats = Arc::clone(&stats);
    pc.on_track(Box::new(move |track, _, _| {
        let stats = Arc::clone(&track_stats);
        if stats.tracks.fetch_add(1, Ordering::Relaxed) + 1 == expected_tracks {
            *stats.all_tracks.lock().unwrap() = Some(start.elapsed());
        }
        let seq = Arc::new(Mutex::new(Seq::default()));
        stats.seqs.lock().unwrap().push(Arc::clone(&seq));
        let latency = Arc::clone(&latency);
        tokio::spawn(async move {
            while let Ok((packet, _)) = track.read_rtp().await {
                stats.packets.fetch_add(1, Ordering::Relaxed);
                seq.lock().unwrap().push(packet.header.sequence_number);
                latency.record(start, &packet.payload);
            }
        });
        Box::pin(async {})
    }));

    tx.send(json!({"event_type": "JoinCall", "data": {"channel_id": channel_id}}))?;
    for (track, interval, size) in [
        (audio, AUDIO_INTERVAL, AUDIO_BYTES),
        (video, VIDEO_INTERVAL, VIDEO_BYTES),
    ] {
        let stats = Arc::clone(&stats);
        tokio::spawn(publish(track, interval, size, start, stats, stop.clone()));
    }

    let mut pending = Vec::new();
    let result = loop {
        let msg = tokio::select! {
            msg = ws_rx.next() => msg,
            _ = stop.changed() => break Ok(()),
        };
        let Some(Ok(Message::Text(text))) = msg else {
            if msg.is_none() {
                break Err("websocket closed".into());
            }
            continue;
        };
        let Ok(event) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let data = &event["data"];
        if let Err(e) = match event["event_type"].as_str() {
            Some("SfuOffer") => answer(&pc, data, &tx, &mut pending).await,
            Some("SfuCandidate") => {
                let init = RTCIceCandidateInit {
                    candidate: data["candidate"].as_str().unwrap_or_default().to_owned(),
                    sdp_mid: data["sdp_mid"].as_str().map(str::to_owned),
                    sdp_mline_index: data["sdp_mline_index"].as_u64().map(|i| i as u16),
                    username_fragment: None,
                };
                if pc.remote_description().await.is_none() {
                    pending.push(init);
                    Ok(())
                } else {
                    pc.add_ice_candidate(init).await.map_err(Into::into)
                }
            }
            _ => Ok(()),
        } {
            break Err(e);
        }
    };

    let _ = tx.send(json!({"event_type": "LeaveCall"}));
    let _ = pc.close().await;
    result
}

async fn answer(
    pc: &RTCPeerConnection,
    data: &Value,
    tx: &mpsc::UnboundedSender<Value>,
    pending: &mut Vec<RTCIceCandidateInit>,
) -> Result<(), BoxError> {
    let sdp = data["sdp"].as_str().ok_or("offer without sdp")?.to_owned();
    pc.set_remote_description(RTCSessionDescription::offer(sdp)?)
        .await?;
    let answer = pc.create_answer(None).await?;
    let sdp = answer.sdp.clone();
    pc.set_local_description(answer).await?;
    tx.send(json!({"event_type": "SfuAnswer", "data": {"sdp": sdp}}))?;
    for candidate in pending.drain(..) {
        pc.add_ice_candidate(candidate).await?;
    }
    Ok(())
}

// synthetic media: the SFU never decodes, so the payload is zeros of the right size
// ending in the send time (µs since start), which stays the payload's last 8 bytes
// because every sample fits one RTP packet
async fn publish(
    track: Arc<TrackLocalStaticSample>,
    interval: Duration,
    size: usize,
    start: Instant,
    stats: Arc<BotStats>,
    stop: watch::Receiver<bool>,
) {
    let mut data = vec![0u8; size];
    let mut tick = tokio::time::interval(interval);
    while !*stop.borrow() {
        tick.tick().await;
        let sent = start.elapsed().as_micros() as u64;
        data[size - 8..].copy_from_slice(&sent.to_be_bytes());
        let sample = Sample {
            data: Bytes::copy_from_slice(&data),
            duration: interval,
            ..Default::default()
        };
        if track.write_sample(&sample).await.is_err() {
            break;
        }
        stats.sent.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{Latency, Seq};
    use std::sync::atomic::AtomicU64;
    use std::time::{Duration, Instant};

    #[test]
    fn latency_percentiles() {
        let start = Instant::now() - Duration::from_secs(10);
        let l = Latency {
            from: Duration::ZERO,
            hist: (0..5000).map(|_| AtomicU64::new(0)).collect(),
        };
        assert_eq!(l.percentile(0.5), None);
        // sent 9.9 s, 9 s and 0 s after start, received ~10 s after start
        for sent_us in [9_900_000u64, 9_000_000, 0] {
            l.record(
                start,
                &[[0u8; 4].as_slice(), &sent_us.to_be_bytes()].concat(),
            );
        }
        l.record(start, &[1, 2, 3]); // too short, ignored
        assert_eq!(l.percentile(0.0), Some(100.0));
        assert_eq!(l.percentile(0.5), Some(1000.0));
        assert_eq!(l.percentile(1.0), Some(4999.0)); // capped at the last bucket
    }

    #[test]
    fn seq_counts_gaps_across_wraparound() {
        let mut s = Seq::default();
        for seq in [65533u16, 65534, 65535, 1, 2, 4] {
            s.push(seq);
        }
        // 65533..=4 is 8 packets, 0 and 3 missing
        assert_eq!(s.expected(), 8);
        assert_eq!(s.lost(), 2);
        s.push(3); // late arrival
        assert_eq!(s.lost(), 1);
    }
}
