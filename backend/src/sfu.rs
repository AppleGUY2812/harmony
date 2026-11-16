// Selective forwarding unit: every call participant has one RTCPeerConnection to the
// server, the server forwards each published RTP track to every other participant.
// Same shape as Pion's sfu-ws example. The server always makes the offers.

use crate::models::{CallParticipant, CallServerEvent, IceCandidate};
use rocket::futures::stream::SplitSink;
use rocket::futures::SinkExt;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;
use webrtc::api::interceptor_registry::{configure_rtcp_reports, configure_twcc_receiver_only};
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::{APIBuilder, API};
use webrtc::ice::mdns::MulticastDnsMode;
use webrtc::ice::network_type::NetworkType;
use webrtc::ice::udp_mux::{UDPMuxDefault, UDPMuxParams};
use webrtc::ice::udp_network::UDPNetwork;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::ice_transport::ice_candidate_type::RTCIceCandidateType;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::nack::{generator::Generator, responder::Responder};
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::signaling_state::RTCSignalingState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::{RTCPFeedback, RTCRtpTransceiverInit};
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::{TrackLocal, TrackLocalWriter};
use webrtc::track::track_remote::TrackRemote;
use ws::{stream::DuplexStream, Message as WsMessage};

pub type WsSender = Arc<Mutex<SplitSink<DuplexStream, WsMessage>>>;

const STUN_SERVER: &str = "stun:stun.l.google.com:19302";

struct Peer {
    user_id: i32,
    // msid of this peer's published tracks; also identifies this join (a session can rejoin)
    stream_id: Uuid,
    pc: Arc<RTCPeerConnection>,
    // drained into the websocket by a writer task, so nothing awaits network I/O under the state lock
    tx: mpsc::Sender<String>,
    // something changed while an offer was in flight, offer again after the answer
    renegotiate: bool,
}

struct Forward {
    owner: Uuid, // session id of the publisher
    local: Arc<TrackLocalStaticRTP>,
    remote: Arc<TrackRemote>,
}

#[derive(Default)]
struct Room {
    peers: HashMap<Uuid, Peer>,       // session id -> peer
    tracks: HashMap<String, Forward>, // local track id -> forwarded track
}

#[derive(Default)]
struct State {
    rooms: HashMap<i32, Room>,    // channel id -> room
    sessions: HashMap<Uuid, i32>, // session id -> channel id
}

pub struct Sfu {
    api: API,
    // ponytail: one global lock like Pion's listLock; per-room locks if many busy rooms contend
    state: Mutex<State>,
}

impl Sfu {
    // WEBRTC_UDP_PORT (default 50000): the single UDP port all calls share.
    // WEBRTC_PUBLIC_IP: advertise this IP instead of the local ones (1:1 NAT, docker).
    pub async fn from_env() -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let port: u16 = std::env::var("WEBRTC_UDP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(50000);
        let socket = UdpSocket::bind(("0.0.0.0", port)).await?;

        let mut settings = SettingEngine::default();
        settings.set_udp_network(UDPNetwork::Muxed(UDPMuxDefault::new(UDPMuxParams::new(
            socket,
        ))));
        settings.set_ice_multicast_dns_mode(MulticastDnsMode::Disabled);
        // all host candidates share the mux socket; an udp6 candidate would read and
        // drop the udp4 packets, so stick to udp4
        settings.set_network_types(vec![NetworkType::Udp4]);
        if let Some(ip) = std::env::var("WEBRTC_PUBLIC_IP")
            .ok()
            .filter(|ip| !ip.is_empty())
        {
            settings.set_nat_1to1_ips(vec![ip], RTCIceCandidateType::Host);
        }

        let mut media = MediaEngine::default();
        media.register_default_codecs()?;
        let mut registry = Registry::new();
        registry = configure_rtcp_reports(registry);
        registry = configure_twcc_receiver_only(registry, &mut media)?;
        // same as configure_nack, but the default 8192-packet resend buffer per
        // outgoing stream is ~1 GB of slots at 50 participants
        for parameter in ["", "pli"] {
            media.register_feedback(
                RTCPFeedback {
                    typ: "nack".to_owned(),
                    parameter: parameter.to_owned(),
                },
                RTPCodecType::Video,
            );
        }
        registry.add(Box::new(Responder::builder().with_log2_size(9)));
        registry.add(Box::new(Generator::builder()));

        let api = APIBuilder::new()
            .with_media_engine(media)
            .with_interceptor_registry(registry)
            .with_setting_engine(settings)
            .build();

        let sfu = Arc::new(Sfu {
            api,
            state: Mutex::default(),
        });

        // keyframe every 3s so late or lossy subscribers recover
        let ticker = Arc::clone(&sfu);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            loop {
                interval.tick().await;
                let state = ticker.state.lock().await;
                for room in state.rooms.values() {
                    request_keyframes(room).await;
                }
            }
        });

        Ok(sfu)
    }

    pub async fn join(
        self: &Arc<Self>,
        session: Uuid,
        user_id: i32,
        channel_id: i32,
        ws: WsSender,
    ) -> Result<(), webrtc::Error> {
        // one call per session
        self.leave(session, None).await;

        let pc = Arc::new(
            self.api
                .new_peer_connection(RTCConfiguration {
                    ice_servers: vec![RTCIceServer {
                        urls: vec![STUN_SERVER.to_owned()],
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .await?,
        );
        // one audio + one video slot for the client to publish into
        for kind in [RTPCodecType::Audio, RTPCodecType::Video] {
            pc.add_transceiver_from_kind(
                kind,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Recvonly,
                    send_encodings: vec![],
                }),
            )
            .await?;
        }

        let stream_id = Uuid::new_v4();

        let (tx, mut rx) = mpsc::channel::<String>(64);
        tokio::spawn(async move {
            while let Some(json) = rx.recv().await {
                if let Err(e) = ws.lock().await.send(json.into()).await {
                    eprintln!("sfu: ws send: {e}");
                    break;
                }
            }
        });

        let candidate_tx = tx.clone();
        pc.on_ice_candidate(Box::new(move |candidate: Option<RTCIceCandidate>| {
            let tx = candidate_tx.clone();
            Box::pin(async move {
                let Some(Ok(init)) = candidate.map(|c| c.to_json()) else {
                    return;
                };
                let event = CallServerEvent::SfuCandidate(IceCandidate {
                    candidate: init.candidate,
                    // bundled: every candidate belongs to the first m-line
                    sdp_mid: None,
                    sdp_mline_index: Some(0),
                });
                send(&tx, &event);
            })
        }));

        // handlers spawn instead of awaiting: they may fire while the state lock is held
        let sfu = Arc::clone(self);
        pc.on_track(Box::new(move |remote, _, _| {
            tokio::spawn(Arc::clone(&sfu).forward(session, channel_id, stream_id, remote));
            Box::pin(async {})
        }));

        let sfu = Arc::clone(self);
        pc.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            if matches!(
                s,
                RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
            ) {
                let sfu = Arc::clone(&sfu);
                tokio::spawn(async move { sfu.leave(session, Some(stream_id)).await });
            }
            Box::pin(async {})
        }));

        let mut state = self.state.lock().await;
        let State { rooms, sessions } = &mut *state;
        sessions.insert(session, channel_id);
        let room = rooms.entry(channel_id).or_default();
        room.peers.insert(
            session,
            Peer {
                user_id,
                stream_id,
                pc,
                tx,
                renegotiate: true, // first offer
            },
        );
        signal(room).await;
        send_participants(room, channel_id).await;
        Ok(())
    }

    // Leave the session's call. With `only`, leave only if it is still that join.
    pub async fn leave(&self, session: Uuid, only: Option<Uuid>) {
        let pc = {
            let mut state = self.state.lock().await;
            let State { rooms, sessions } = &mut *state;
            let Some(&channel_id) = sessions.get(&session) else {
                return;
            };
            let Some(room) = rooms.get_mut(&channel_id) else {
                return;
            };
            if only.is_some_and(|id| room.peers.get(&session).map(|p| p.stream_id) != Some(id)) {
                return;
            }
            sessions.remove(&session);
            let Some(peer) = room.peers.remove(&session) else {
                return;
            };
            room.tracks.retain(|_, f| f.owner != session);
            if room.peers.is_empty() {
                rooms.remove(&channel_id);
            } else {
                signal(room).await;
                send_participants(room, channel_id).await;
            }
            peer.pc
        };
        let _ = pc.close().await;
    }

    pub async fn answer(&self, session: Uuid, sdp: String) -> Result<(), webrtc::Error> {
        let pc = self
            .pc(session)
            .await
            .ok_or(webrtc::Error::ErrConnectionClosed)?;
        pc.set_remote_description(RTCSessionDescription::answer(sdp)?)
            .await?;

        // send the offer that was held back while this one was in flight
        let mut state = self.state.lock().await;
        let State { rooms, sessions } = &mut *state;
        if let Some(room) = sessions.get(&session).and_then(|c| rooms.get_mut(c)) {
            if room.peers.get(&session).is_some_and(|p| p.renegotiate) {
                signal(room).await;
            }
        }
        Ok(())
    }

    pub async fn candidate(&self, session: Uuid, c: IceCandidate) -> Result<(), webrtc::Error> {
        let pc = self
            .pc(session)
            .await
            .ok_or(webrtc::Error::ErrConnectionClosed)?;
        pc.add_ice_candidate(RTCIceCandidateInit {
            candidate: c.candidate,
            sdp_mid: c.sdp_mid,
            sdp_mline_index: c.sdp_mline_index,
            username_fragment: None,
        })
        .await
    }

    async fn pc(&self, session: Uuid) -> Option<Arc<RTCPeerConnection>> {
        let state = self.state.lock().await;
        let room = state.rooms.get(state.sessions.get(&session)?)?;
        Some(Arc::clone(&room.peers.get(&session)?.pc))
    }

    // Publish a remote track to the room and pump its RTP until the publisher goes away.
    async fn forward(
        self: Arc<Self>,
        session: Uuid,
        channel_id: i32,
        stream_id: Uuid,
        remote: Arc<TrackRemote>,
    ) {
        let local = Arc::new(TrackLocalStaticRTP::new(
            remote.codec().capability,
            Uuid::new_v4().to_string(),
            stream_id.to_string(),
        ));
        let track_id = local.id().to_owned();
        {
            let mut state = self.state.lock().await;
            let Some(room) = state.rooms.get_mut(&channel_id) else {
                return;
            };
            if room.peers.get(&session).map(|p| p.stream_id) != Some(stream_id) {
                return;
            }
            room.tracks.insert(
                track_id.clone(),
                Forward {
                    owner: session,
                    local: Arc::clone(&local),
                    remote: Arc::clone(&remote),
                },
            );
            signal(room).await;
        }

        // no decoding, just rewrite ssrc/payload type per subscriber (done by write_rtp)
        while let Ok((packet, _)) = remote.read_rtp().await {
            let _ = local.write_rtp(&packet).await;
        }

        let mut state = self.state.lock().await;
        if let Some(room) = state.rooms.get_mut(&channel_id) {
            if room.tracks.remove(&track_id).is_some() {
                signal(room).await;
            }
        }
    }
}

// Bring every peer's senders in line with the room's tracks and offer the changes.
// At most one offer in flight per peer: a peer mid-negotiation is marked and
// re-signaled after its answer arrives.
async fn signal(room: &mut Room) {
    let Room { peers, tracks } = room;
    let mut added_any = false;
    for (session, peer) in peers.iter_mut() {
        let mut have = HashSet::new();
        let mut stale = Vec::new();
        for sender in peer.pc.get_senders().await {
            if let Some(track) = sender.track().await {
                if tracks.contains_key(track.id()) {
                    have.insert(track.id().to_owned());
                } else {
                    stale.push(sender);
                }
            }
        }
        let missing: Vec<_> = tracks
            .iter()
            .filter(|(id, f)| f.owner != *session && !have.contains(*id))
            .map(|(_, f)| Arc::clone(&f.local))
            .collect();
        let changed = !stale.is_empty() || !missing.is_empty();

        if peer.pc.signaling_state() != RTCSignalingState::Stable {
            peer.renegotiate |= changed;
            continue;
        }
        if !changed && !peer.renegotiate {
            continue;
        }
        for sender in stale {
            let _ = peer.pc.remove_track(&sender).await;
        }
        added_any |= !missing.is_empty();
        for track in missing {
            match peer.pc.add_track(track).await {
                // RTCP (NACKs for the nack responder) only reaches the interceptors while
                // someone reads; ends when the sender is removed or the pc closes
                Ok(sender) => {
                    tokio::spawn(async move {
                        let mut b = vec![0u8; 1500];
                        while sender.read(&mut b).await.is_ok() {}
                    });
                }
                Err(e) => eprintln!("sfu: add_track: {e}"),
            }
        }
        peer.renegotiate = false;
        if let Err(e) = offer(peer).await {
            eprintln!("sfu: offer: {e}");
        }
    }
    if added_any {
        request_keyframes(room).await;
    }
}

async fn offer(peer: &Peer) -> Result<(), webrtc::Error> {
    let offer = peer.pc.create_offer(None).await?;
    let sdp = offer.sdp.clone();
    peer.pc.set_local_description(offer).await?;
    send(&peer.tx, &CallServerEvent::SfuOffer { sdp });
    Ok(())
}

// PLI to every video publisher in the room
async fn request_keyframes(room: &Room) {
    for f in room.tracks.values() {
        if f.remote.kind() != RTPCodecType::Video {
            continue;
        }
        if let Some(owner) = room.peers.get(&f.owner) {
            let _ = owner
                .pc
                .write_rtcp(&[Box::new(PictureLossIndication {
                    sender_ssrc: 0,
                    media_ssrc: f.remote.ssrc(),
                })])
                .await;
        }
    }
}

async fn send_participants(room: &Room, channel_id: i32) {
    let event = CallServerEvent::CallParticipants {
        channel_id,
        participants: room
            .peers
            .values()
            .map(|p| CallParticipant {
                user_id: p.user_id,
                stream_id: p.stream_id.to_string(),
            })
            .collect(),
    };
    for peer in room.peers.values() {
        send(&peer.tx, &event);
    }
}

// A client that stops reading fills its queue and loses signaling; only it is affected.
fn send(tx: &mpsc::Sender<String>, event: &CallServerEvent) {
    let json = serde_json::to_string(event).expect("serializable");
    if let Err(e) = tx.try_send(json) {
        eprintln!("sfu: signaling queue: {e}");
    }
}
