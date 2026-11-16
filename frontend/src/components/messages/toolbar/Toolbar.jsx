import { useAuthContext } from "../../../context/AuthContext";
import { useWebsocketContext } from "../../../context/WebsocketContext";
import useServerStore from "../../../zustand/useServerStore";
import useWebRTCStore from "../../../zustand/useWebRTCStore";
import { TbBrandAmongUs } from "react-icons/tb";
import { IoCall } from "react-icons/io5";
import ProfilePicture from "../../../utils/ProfilePicture";

// toolbar contains information about the selected channel
const Toolbar = () => {
  const selectedServer = useServerStore((state) => state.selectedServer);
  const selectedChannel = useServerStore((state) => state.selectedChannel);
  const selectedConversation = useServerStore(
    (state) => state.selectedConversation,
  );
  const activeCall = useWebRTCStore((state) => state.activeCall);
  const joinCall = useWebRTCStore((state) => state.joinCall);
  const { websocket } = useWebsocketContext();
  const { authUser } = useAuthContext();
  const otherUser = selectedConversation?.users.filter(
    (user) => user.user_id !== authUser.user_id,
  )[0];
  const inThisCall = activeCall?.channel_id === selectedChannel.channel_id;

  return (
    <div className="flex select-none items-center justify-between border-b border-base-400 bg-base-100 px-4 py-2 text-content-header">
      <div className="flex items-center gap-2">
        {selectedConversation ? (
          <ProfilePicture size={32} user={otherUser} />
        ) : (
          <TbBrandAmongUs className="flex-shrink-0" size={20} />
        )}
        <p className="truncate font-semibold">
          {selectedConversation
            ? otherUser.display_username
            : selectedChannel.channel_name}
        </p>
      </div>
      {inThisCall ? null : (
        <button
          title="Start or join call"
          onClick={() =>
            joinCall(
              selectedServer.server_id,
              selectedChannel.channel_id,
              websocket,
            )
          }
        >
          <IoCall className="text-button" size="24" />
        </button>
      )}
    </div>
  );
};

export default Toolbar;
