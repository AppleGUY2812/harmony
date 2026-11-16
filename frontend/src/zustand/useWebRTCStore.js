import { create } from "zustand";
import toast from "react-hot-toast";

// One RTCPeerConnection to the SFU per call. The server makes every offer
// (on join and whenever participants' tracks change); we only answer.

const getLocalMedia = async () => {
  try {
    return await navigator.mediaDevices.getUserMedia({
      audio: true,
      video: { width: { ideal: 1280 }, height: { ideal: 720 } },
    });
  } catch {
    // no camera (or camera denied): audio only
    return navigator.mediaDevices.getUserMedia({ audio: true });
  }
};

const fetchUsers = async (serverId) => {
  const res = await fetch(
    `${import.meta.env.VITE_API_URL}/api/servers/users/${serverId}`,
    { credentials: "include" },
  );
  const data = await res.json();
  if (!res.ok) throw new Error(data.error);
  return Object.fromEntries(data.map((user) => [user.user_id, user]));
};

const send = (websocket, event_type, data) =>
  websocket?.send(JSON.stringify({ event_type, data }));

// stream id -> <audio> playing it; not in the DOM, so remote audio keeps
// playing whichever page is shown
const audioSinks = new Map();
const removeAudio = (streamId) => {
  const audio = audioSinks.get(streamId);
  if (audio) audio.srcObject = null;
  audioSinks.delete(streamId);
};

const useWebRTCStore = create((set, get) => ({
  // {server_id, channel_id} of the call we're in
  activeCall: null,
  peerConnection: null,
  websocket: null,
  localStream: null,
  micOn: true,
  cameraOn: true,
  // stream id -> MediaStream
  remoteStreams: {},
  // [{user_id, stream_id}], includes us
  participants: [],
  // user_id -> user, members of the call's server (for names/pictures)
  users: {},
  pendingCandidates: [],
  // offers are applied one at a time
  signaling: Promise.resolve(),

  joinCall: async (serverId, channelId, websocket) => {
    if (get().activeCall) get().leaveCall();
    if (websocket?.readyState !== WebSocket.OPEN) {
      toast.error("Not connected, try again in a moment");
      return;
    }
    // identifies this join, so a stale one (left and rejoined during the prompt) bails
    const join = {};
    set({ activeCall: { server_id: serverId, channel_id: channelId, join } });

    let localStream;
    try {
      localStream = await getLocalMedia();
    } catch (error) {
      toast.error(`Can't access microphone: ${error.message}`);
      if (get().activeCall?.join === join) set({ activeCall: null });
      return;
    }
    // left (or switched calls) while waiting for permission
    if (get().activeCall?.join !== join) {
      localStream.getTracks().forEach((track) => track.stop());
      return;
    }

    const pc = new RTCPeerConnection({
      iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
    });
    // matched to the server's recvonly audio/video m-lines when its offer arrives
    localStream.getTracks().forEach((track) => pc.addTrack(track, localStream));

    pc.ontrack = ({ streams }) => {
      const stream = streams[0];
      if (!stream) return;
      set((state) => ({
        remoteStreams: { ...state.remoteStreams, [stream.id]: stream },
      }));
      if (!audioSinks.has(stream.id)) {
        const audio = new Audio();
        audio.srcObject = stream;
        audio.play().catch((error) => console.error("Remote audio", error));
        audioSinks.set(stream.id, audio);
      }
      // the server removed this participant's tracks (they left)
      stream.onremovetrack = () => {
        if (stream.getTracks().length) return;
        removeAudio(stream.id);
        set((state) => {
          const remoteStreams = { ...state.remoteStreams };
          delete remoteStreams[stream.id];
          return { remoteStreams };
        });
      };
    };
    pc.onicecandidate = ({ candidate }) => {
      if (!candidate?.candidate) return; // end of candidates
      send(websocket, "SfuCandidate", {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdpMid,
        sdp_mline_index: candidate.sdpMLineIndex,
      });
    };
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === "failed") {
        toast.error("Call connection failed");
        get().leaveCall();
      }
    };

    set({
      peerConnection: pc,
      websocket,
      localStream,
      micOn: true,
      cameraOn: localStream.getVideoTracks().length > 0,
    });
    send(websocket, "JoinCall", { channel_id: channelId });

    fetchUsers(serverId)
      .then((users) => set({ users }))
      .catch((error) => toast.error(error.message));
  },

  leaveCall: () => {
    const { peerConnection, localStream, websocket, activeCall } = get();
    if (activeCall) send(websocket, "LeaveCall");
    peerConnection?.close();
    localStream?.getTracks().forEach((track) => track.stop());
    [...audioSinks.keys()].forEach(removeAudio);
    set({
      activeCall: null,
      peerConnection: null,
      localStream: null,
      remoteStreams: {},
      participants: [],
      users: {},
      pendingCandidates: [],
    });
  },

  toggleMic: () => {
    const micOn = !get().micOn;
    get()
      .localStream?.getAudioTracks()
      .forEach((track) => (track.enabled = micOn));
    set({ micOn });
  },

  toggleCamera: () => {
    const tracks = get().localStream?.getVideoTracks() ?? [];
    if (!tracks.length) return;
    const cameraOn = !get().cameraOn;
    tracks.forEach((track) => (track.enabled = cameraOn));
    set({ cameraOn });
  },

  // websocket events --------------------------------------------------------

  handleOffer: (sdp) => {
    const signaling = get().signaling.then(async () => {
      const { peerConnection: pc, websocket } = get();
      if (!pc) return;
      try {
        await pc.setRemoteDescription({ type: "offer", sdp });
        const answer = await pc.createAnswer();
        await pc.setLocalDescription(answer);
        send(websocket, "SfuAnswer", { sdp: answer.sdp });
        const queued = get().pendingCandidates;
        set({ pendingCandidates: [] });
        for (const candidate of queued) await pc.addIceCandidate(candidate);
      } catch (error) {
        console.error("Failed to answer SFU offer", error);
      }
    });
    set({ signaling });
  },

  handleCandidate: async ({ candidate, sdp_mid, sdp_mline_index }) => {
    const pc = get().peerConnection;
    if (!pc) return;
    const init = {
      candidate,
      sdpMid: sdp_mid,
      sdpMLineIndex: sdp_mline_index,
    };
    if (!pc.remoteDescription) {
      set((state) => ({
        pendingCandidates: [...state.pendingCandidates, init],
      }));
      return;
    }
    try {
      await pc.addIceCandidate(init);
    } catch (error) {
      console.error("Failed to add ICE candidate", error);
    }
  },

  handleParticipants: async ({ channel_id, participants }) => {
    const { activeCall, users } = get();
    if (activeCall?.channel_id !== channel_id) return;
    set({ participants });
    // someone joined the server after we fetched its members
    if (participants.some((p) => !users[p.user_id])) {
      try {
        set({ users: await fetchUsers(activeCall.server_id) });
      } catch (error) {
        toast.error(error.message);
      }
    }
  },
}));

export default useWebRTCStore;
