import { useEffect } from "react";
import { useWebsocketContext } from "../../context/WebsocketContext";
import useWebRTCStore from "../../zustand/useWebRTCStore";

// routes SFU signaling events from the websocket into the call store
const useListenWebRTC = () => {
  const { websocket } = useWebsocketContext();

  useEffect(() => {
    if (!websocket) return;
    const handleMessage = (event) => {
      let message;
      try {
        message = JSON.parse(event.data);
      } catch {
        return;
      }
      const call = useWebRTCStore.getState();
      if (message.event_type === "SfuOffer") call.handleOffer(message.data.sdp);
      else if (message.event_type === "SfuCandidate")
        call.handleCandidate(message.data);
      else if (message.event_type === "CallParticipants")
        call.handleParticipants(message.data);
    };
    // the server ends the call when the websocket drops
    const handleClose = () => {
      if (useWebRTCStore.getState().activeCall)
        useWebRTCStore.getState().leaveCall();
    };
    websocket.addEventListener("message", handleMessage);
    websocket.addEventListener("close", handleClose);
    return () => {
      websocket.removeEventListener("message", handleMessage);
      websocket.removeEventListener("close", handleClose);
    };
  }, [websocket]);
};

export default useListenWebRTC;
