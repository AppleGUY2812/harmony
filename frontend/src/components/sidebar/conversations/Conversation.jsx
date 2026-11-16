import PropTypes from "prop-types";
import useServerStore from "../../../zustand/useServerStore";
import { useAuthContext } from "../../../context/AuthContext";
import CloudFrontImage from "../../../utils/CloudFrontImage";

// conversation is a Server that is type DM or GroupChat, and they have only 1 channel
const Conversation = ({ handleClick, conversation }) => {
  const { server, users } = conversation;

  const selectedServer = useServerStore((state) => state.selectedServer);

  const serverId = server.server_id;
  const isSelected = selectedServer?.server_id === serverId;

  const { authUser } = useAuthContext();
  const user_id = authUser.user_id;
  const otherUsers = users.filter((user) => user.user_id !== user_id);
  // handle only dm's rn, handle groupchats later
  const otherUser = otherUsers[0];

  return (
    <div
      onClick={() => handleClick(conversation)}
      className={`mb-1 flex h-[52px] items-center gap-2 rounded-md p-1.5 hover:cursor-pointer hover:bg-base-100 hover:text-content-muted-50 active:bg-base-50 active:text-content-header ${isSelected ? "bg-base-50 text-content-header" : "bg-base-200 text-content-muted-100"} `}
    >
      <div className="h-10 w-10 overflow-hidden rounded-md bg-primary">
        {otherUser.s3_icon_key ? (
          <CloudFrontImage
            className="h-10 w-10"
            folder="user-icons"
            iconKey={otherUser.s3_icon_key}
            thumb
          />
        ) : (
          <img
            className="h-10 w-10"
            src={`https://robohash.org/${otherUser.display_username}`}
          />
        )}
      </div>
      <p className="truncate font-medium">{otherUser.display_username}</p>
    </div>
  );
};

Conversation.propTypes = {
  handleClick: PropTypes.func,
  conversation: PropTypes.shape({
    users: PropTypes.arrayOf(
      PropTypes.shape({
        date_joined: PropTypes.string,
        display_username: PropTypes.string,
        profile_picture: PropTypes.string,
        user_id: PropTypes.number,
      }),
    ),
    server: PropTypes.shape({
      server_id: PropTypes.number,
      server_type: PropTypes.string,
      members: PropTypes.number,
      server_name: PropTypes.string,
      unread_messages: PropTypes.number,
    }),
    channel: PropTypes.shape({
      channel_id: PropTypes.number,
      server_id: PropTypes.number,
      channel_name: PropTypes.string,
    }),
  }),
};
export default Conversation;
