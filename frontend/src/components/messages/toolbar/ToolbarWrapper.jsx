import Toolbar from "./Toolbar";
import CallToolbar from "./CallToolbar";
import useListenWebRTC from "../../../hooks/webRTC/useListenWebRTC";
import { useEffect, useRef } from "react";
import useServerStore from "../../../zustand/useServerStore";
import { useAuthContext } from "../../../context/AuthContext";
import useWebRTCStore from "../../../zustand/useWebRTCStore";
import { useWebsocketContext } from "../../../context/WebsocketContext";

const ToolbarWrapper = () => {
  const selectedConversation = useServerStore(
    (state) => state.selectedConversation,
  );
  const activeCall = useServerStore((state) => state.activeCall);
  const setActiveCall = useServerStore((state) => state.setActiveCall);
  const localVideoRef = useRef(null);
  const remoteVideoRef = useRef(null);
  const { initiateCall, endCall, localStream, remoteStream } = useWebRTCStore(
    (state) => ({
      initiateCall: state.initiateCall,
      endCall: state.endCall,
      localStream: state.localStream,
      remoteStream: state.remoteStream,
    }),
  );
  // Attach local stream to local/remote video element
  useEffect(() => {
    if (localStream && localVideoRef.current) {
      localVideoRef.current.srcObject = localStream;
    }
    if (remoteStream && remoteVideoRef.current) {
      remoteVideoRef.current.srcObject = null; // Reset it first

      remoteVideoRef.current.srcObject = remoteStream;
    }
  }, [localStream, remoteStream]);

  const { websocket } = useWebsocketContext();

  // handle groupchat later, only handling dm rn
  const { authUser } = useAuthContext();
  const otherUser = selectedConversation?.users.filter(
    (user) => user.user_id !== authUser.user_id,
  )[0];
  //   const { localStream, remoteStream, initiateCall, endCall } = useListenWebRTC(
  //     otherUser?.user_id,
  //   );

  const callButtonClick = () => {
    if (!activeCall) {
      setActiveCall({ user_id: otherUser.user_id });
      initiateCall(otherUser.user_id, websocket, authUser.user_id);
    }
  };
  const endCallButtonClick = () => {
    setActiveCall(null);
    endCall();
    const endCallMessage = {
      event_type: "EndCall",
      data: {
        to: otherUser.user_id,
        from: authUser.user_id,
      },
    };
    websocket.send(JSON.stringify(endCallMessage));
  };
  console.log(activeCall?.user_id);
  console.log(otherUser?.user_id);
  return (
    <>
      {activeCall?.user_id === otherUser?.user_id ? (
        <CallToolbar
          localVideoRef={localVideoRef}
          remoteVideoRef={remoteVideoRef}
          endCallButtonClick={endCallButtonClick}
        />
      ) : (
        <Toolbar callButtonClick={callButtonClick} />
      )}
      {/* <div className="bg-green-500">
        ffsdfsd
        <video
          className="border-8 border-solid border-red-500"
          ref={localVideoRef}
          autoPlay
          playsInline
          muted
        />
        <video ref={remoteVideoRef} autoPlay playsInline />
      </div> */}
    </>
  );
};

export default ToolbarWrapper;
