import { create } from "zustand";

const useWebRTCStore = create((set, get) => ({
  localStream: null,
  remoteStream: null,
  peerConnection: null,
  setPeerConnection: (peerConnection) => set({ peerConnection }),

  // Action to set streams
  setLocalStream: (stream) => set({ localStream: stream }),
  setRemoteStream: (stream) => set({ remoteStream: stream }),

  // Method to create the PeerConnection
  createPeerConnection: async (toUserId, websocket, fromUserId) => {
    const { localStream, setLocalStream, setRemoteStream } = get();

    // Get the user's local media (camera and microphone)
    const stream = await navigator.mediaDevices.getUserMedia({
      video: {
        aspectRatio: 16 / 9, // Enforce a 16:9 aspect ratio
        width: { ideal: 1280 }, // Ideal width in pixels
        height: { ideal: 720 }, // Ideal height in pixels
      },
      audio: true,
    });
    setLocalStream(stream);

    const configuration = {
      iceServers: [
        {
          urls: "stun:stun.l.google.com:19302",
        },
      ],
    };
    const peerConnection = new RTCPeerConnection(configuration);

    // Add local stream to the peer connection
    // console.log(localStream);
    // console.log(stream);
    if (stream) {
      // console.log("local stream added");
      stream.getTracks().forEach((track) => {
        // console.log("track added");
        peerConnection.addTrack(track, stream);
      });
    }

    // Handle remote stream
    peerConnection.ontrack = (event) => {
      // console.log("RSJADIOFDSOJFO REMOTE STERAM MMMMMMMMMMM ");
      setRemoteStream(event.streams[0]); // Set the incoming remote stream
    };

    // Send ICE candidates to the remote peer via WebSocket
    peerConnection.onicecandidate = (event) => {
      if (event.candidate && websocket) {
        const iceCandidate = {
          event_type: "IceCandidate",
          data: {
            candidate: event.candidate.candidate, // ICE candidate
            sdp_mid: event.candidate.sdpMid, // SDP mid
            sdp_mline_index: event.candidate.sdpMLineIndex, // SDP m-line index
            to: toUserId, // Receiver's user ID
            from: fromUserId, // Sender's user ID
          },
        };
        websocket.send(JSON.stringify(iceCandidate));
      }
    };

    peerConnection.addEventListener("connectionstatechange", () => {
      const state = peerConnection.connectionState;
      console.log(`Connection state changed: ${state}`);

      if (
        state === "disconnected" ||
        state === "failed" ||
        state === "closed"
      ) {
        get().endCall(); // Handle disconnection
      }
    });

    get().setPeerConnection(peerConnection);
    // console.log("set peer connection CREATE PEERCONNECTION");
    return peerConnection;
  },
  // Method to initiate a WebRTC call
  initiateCall: async (toUserId, websocket, fromUserId) => {
    // console.log(2388934234891);
    // console.log(toUserId);
    // console.log(fromUserId);
    await get().createPeerConnection(toUserId, websocket, fromUserId);
    const peerConnection = get().peerConnection;
    const offer = await peerConnection.createOffer();
    await peerConnection.setLocalDescription(offer);

    const offerMessage = {
      event_type: "Offer",
      data: {
        sdp: offer.sdp,
        to: toUserId,
        from: fromUserId,
      },
    };
    websocket.send(JSON.stringify(offerMessage));

    set({ peerConnection });
  },

  // Method to end the WebRTC call
  endCall: () => {
    const { peerConnection, localStream } = get();

    if (peerConnection) {
      peerConnection.close();
      set({ peerConnection: null });
    }

    if (localStream) {
      localStream.getTracks().forEach((track) => track.stop());
      set({ localStream: null });
    }

    set({ remoteStream: null });
  },
}));

export default useWebRTCStore;
