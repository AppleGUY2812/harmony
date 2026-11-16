import { useAuthContext } from "../../../context/AuthContext";
import useServerStore from "../../../zustand/useServerStore";
import { TbBrandAmongUs } from "react-icons/tb";
import CallButton from "./CallButton";
import { IoCall } from "react-icons/io5";
import ProfilePicture from "../../../utils/ProfilePicture";

// toolbar contains information about the selected channel
const Toolbar = ({ callButtonClick }) => {
  const selectedChannel = useServerStore((state) => state.selectedChannel);
  const selectedConversation = useServerStore(
    (state) => state.selectedConversation,
  );
  // handle groupchat later, only handling dm rn
  const { authUser } = useAuthContext();
  const otherUser = selectedConversation?.users.filter(
    (user) => user.user_id !== authUser.user_id,
  )[0];

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
      {selectedConversation ? (
        <button onClick={callButtonClick}>
          <IoCall className="text-button" size="24" />
        </button>
      ) : null}
    </div>
  );
};

export default Toolbar;
