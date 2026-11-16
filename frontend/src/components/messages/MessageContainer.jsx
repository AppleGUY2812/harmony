import useServerStore from "../../zustand/useServerStore";
import CallToolbar from "./toolbar/CallToolbar";
import MessageInput from "./MessageInput";
import Messages from "./Messages";
import Toolbar from "./toolbar/Toolbar";
import ToolbarWrapper from "./toolbar/ToolbarWrapper";
const MessageContainer = () => {
  const selectedServer = useServerStore((state) => state.selectedServer);
  const selectedChannel = useServerStore((state) => state.selectedChannel);

  return (
    <div className="flex h-screen flex-1 flex-col gap-3 bg-base-100">
      {!selectedChannel || !selectedServer ? null : (
        <>
          <ToolbarWrapper />
          <Messages />
          <MessageInput />
        </>
      )}
    </div>
  );
};

export default MessageContainer;
