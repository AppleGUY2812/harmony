import { useState, useEffect, useRef } from "react";

import { useWebsocketContext } from "../../context/WebsocketContext";
import { useAuthContext } from "../../context/AuthContext";
import useServerStore from "../../zustand/useServerStore";
import useWebRTCStore from "../../zustand/useWebRTCStore";

const useListenWebRTC = () => {
  const { authUser } = useAuthContext(); // Authenticated user's info
  const { websocket } = useWebsocketContext(); // Access WebSocket
  const { createPeerConnection, peerConnection, endCall } = useWebRTCStore(
    (state) => ({
      createPeerConnection: state.createPeerConnection,
      peerConnection: state.peerConnection,
      endCall: state.endCall,
    }),
  );

  // const [localStream, setLocalStream] = useState(null);
  // const [remoteStream, setRemoteStream] = useState(null);
  // const localStreamRef = useRef(null);
  // const peerConnectionRef = useRef(null);

  const setActiveCall = useServerStore((state) => state.setActiveCall);
  const iceCandidateQueue = useRef([]);

  // const configuration = {
  //   iceServers: [
  //     {
  //       urls: "stun:stun.l.google.com:19302",
  //     },
  //   ],
  // };

  // useEffect(() => {
  //   const init = async () => {
  //     // console.log("init");
  //     // Get the user's local media (camera and microphone)
  //     const stream = await navigator.mediaDevices.getUserMedia({
  //       video: true,
  //       audio: true,
  //     });
  //     // console.log(stream);
  //     setLocalStream(stream);
  //     localStream = stream;
  //   };

  //   init();
  // }, []);

  // // console.log("111local stream?");
  // // console.log(localStream);
  // const createPeerConnection = async () => {
  //   // Get the user's local media (camera and microphone)
  //   const stream = await navigator.mediaDevices.getUserMedia({
  //     video: true,
  //     audio: true,
  //   });
  //   setLocalStream(stream);
  //   // localStreamRef.current = stream;

  //   const peerConnection = new RTCPeerConnection(configuration);

  //   // Add local stream to the peer connection
  //   // console.log("local stream?");
  //   // console.log(localStream);
  //   if (localStream) {
  //     // console.log("local stream added");
  //     localStream.getTracks().forEach((track) => {
  //       peerConnection.addTrack(track, localStream);
  //     });
  //   }

  //   // Handle remote stream
  //   peerConnection.ontrack = (event) => {
  //     // console.log(event);
  //     // console.log("rat");
  //     setRemoteStream(event.streams[0]); // Set the incoming remote stream
  //   };

  //   // Send ICE candidates  to the remote peer via WebSocket
  //   peerConnection.onicecandidate = (event) => {
  //     if (event.candidate && websocket) {
  //       const iceCandidate = {
  //         event_type: "IceCandidate",
  //         data: {
  //           candidate: event.candidate.candidate, // ICE candidate
  //           sdp_mid: event.candidate.sdpMid, // SDP mid
  //           sdp_mline_index: event.candidate.sdpMLineIndex, // SDP m-line index
  //           to: toUserId, // Receiver's user ID
  //           from: authUser.user_id, // Sender's user ID
  //         },
  //       };
  //       // console.log(iceCandidate);
  //       websocket.send(JSON.stringify(iceCandidate));
  //     }
  //   };

  //   return peerConnection;
  // };

  // // Initiate a WebRTC call (Offer)
  // const initiateCall = async () => {
  //   const peerConnection = await createPeerConnection();
  //   peerConnectionRef.current = peerConnection;

  //   const offer = await peerConnection.createOffer();
  //   await peerConnection.setLocalDescription(offer);

  //   // Send the offer to the remote peer via WebSocket
  //   const offerMessage = {
  //     event_type: "Offer",
  //     data: {
  //       sdp: offer.sdp,
  //       to: toUserId, // Send to the other user
  //       from: authUser.user_id, // Your user ID
  //     },
  //   };
  //   websocket.send(JSON.stringify(offerMessage));
  // };

  // const endCall = () => {
  //   if (peerConnectionRef.current) {
  //     peerConnectionRef.current.close();
  //     peerConnectionRef.current = null;
  //   }

  //   if (localStream) {
  //     localStream.getTracks().forEach((track) => track.stop());
  //     setLocalStream(null);
  //     localStream = null;
  //   }

  //   setRemoteStream(null);
  // };

  // Handle incoming WebSocket messages for signaling
  useEffect(() => {
    if (!websocket) return;

    const handleWebRTCMessage = async (event) => {
      // console.log(event);
      const message = JSON.parse(event.data);

      if (
        message.event_type === "Offer" &&
        message.data.to === authUser.user_id
      ) {
        // console.log("OFFFEERR");
        setActiveCall({ user_id: message.data.from });
        // peerCOnnection is set from createPeerConnection
        const peerConnectionTemp = await createPeerConnection(
          message.data.from,
          websocket,
          message.data.to,
        );
        // console.log("peerconnection");
        // console.log(peerConnectionTemp);
        // peerConnectionRef.current = peerConnection;

        await peerConnectionTemp.setRemoteDescription(
          new RTCSessionDescription({ type: "offer", sdp: message.data.sdp }),
        );

        const answer = await peerConnectionTemp.createAnswer();
        await peerConnectionTemp.setLocalDescription(answer);

        // Send the answer back to the remote peer via WebSocket
        const answerMessage = {
          event_type: "Answer",
          data: {
            sdp: answer.sdp,
            to: message.data.from, // Respond back to the caller
            from: authUser.user_id, // Your user ID
          },
        };
        websocket.send(JSON.stringify(answerMessage));
        // console.log("SENT ANSWER");
        // console.log(answerMessage);

        iceCandidateQueue.current.forEach((candidate) => {
          peerConnectionTemp.addIceCandidate(candidate);
          // console.log("added ice candidate");
        });
        iceCandidateQueue.current = [];
      } else if (
        message.event_type === "Answer" &&
        message.data.to === authUser.user_id
      ) {
        // console.log("RECEIVEDF ANSWER");
        // Set the remote description to complete the connection
        await peerConnection.setRemoteDescription(
          new RTCSessionDescription({ type: "answer", sdp: message.data.sdp }),
        );
      } else if (
        message.event_type === "IceCandidate" &&
        message.data.to === authUser.user_id
      ) {
        // console.log("Received ICE Candidate:", message.data);
        try {
          // Construct a full ICE candidate with candidate, sdpMid, and sdpMLineIndex
          const candidate = new RTCIceCandidate({
            candidate: message.data.candidate,
            sdpMid: message.data.sdp_mid,
            sdpMLineIndex: message.data.sdp_mline_index,
          });
          if (peerConnection?.remoteDescription) {
            // If remote description is set, add the candidate immediately
            await peerConnection.addIceCandidate(candidate);
          } else {
            // Queue the ICE candidate if the remote description is not set
            // console.log(
            //   "Queuing ICE Candidate because remote description is not set yet.",
            // );
            iceCandidateQueue.current.push(candidate);
          }
        } catch (error) {
          console.error("Failed to add ICE candidate", error);
        }
      } else if (
        message.event_type === "EndCall" &&
        message.data.to === authUser.user_id
      ) {
        console.log("END CALL RECEIVED");
        endCall();
      }
    };

    websocket.addEventListener("message", handleWebRTCMessage);
    return () => {
      websocket.removeEventListener("message", handleWebRTCMessage);
    };
  }, [
    websocket,
    authUser.user_id,
    createPeerConnection,
    peerConnection,
    setActiveCall,
    endCall,
  ]);

  // return { localStream, remoteStream, initiateCall, endCall };
};

export default useListenWebRTC;
