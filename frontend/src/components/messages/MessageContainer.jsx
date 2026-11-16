import useServerStore from "../../zustand/useServerStore";
import useWebRTCStore from "../../zustand/useWebRTCStore";
import CallToolbar from "./toolbar/CallToolbar";
import MessageInput from "./MessageInput";
import Messages from "./Messages";
import Toolbar from "./toolbar/Toolbar";
const MessageContainer = () => {
  const selectedServer = useServerStore((state) => state.selectedServer);
  const selectedChannel = useServerStore((state) => state.selectedChannel);
  const activeCall = useWebRTCStore((state) => state.activeCall);

  return (
    <div className="flex h-screen flex-1 flex-col gap-3 bg-base-100">
      {/* outside the channel guard: the call stays up while browsing servers */}
      {activeCall ? <CallToolbar /> : null}
      {!selectedChannel || !selectedServer ? null : (
        <>
          <Toolbar />
          <Messages />
          <MessageInput />
        </>
      )}
    </div>
  );
};

export default MessageContainer;
