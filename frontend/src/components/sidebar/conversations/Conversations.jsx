import Conversation from "./Conversation";

import toast from "react-hot-toast";
import { useCallback, useEffect, useMemo } from "react";
import useConversationInfo from "../../../hooks/useConversationInfo";
import useServerStore from "../../../zustand/useServerStore";

// TODO: START CACHEING EVERYTHING
// TODO: make last messaged conversation the top
const Conversations = () => {
  // useWebsocketContext();

  const conversations = useServerStore((state) => state.conversations);
  const setConversations = useServerStore((state) => state.setConversations);

  const setSelectedConversation = useServerStore(
    (state) => state.setSelectedConversation,
  );
  const setSelectedServer = useServerStore((state) => state.setSelectedServer);
  const setSelectedChannel = useServerStore(
    (state) => state.setSelectedChannel,
  );

  // TODO: put groupchats in here
  // TODO: handle the server name in the backend?
  const { loading, conversations: fetchedConversations } =
    useConversationInfo("Dm");

  useEffect(
    () => setConversations(fetchedConversations),
    [fetchedConversations, setConversations],
  );

  const handleConversationClick = useCallback(
    (conversation) => {
      setSelectedConversation(conversation);
      setSelectedServer(conversation.server);
      // rename channel_name with the other user
      // also dms and groupchats only have 1 channel
      setSelectedChannel(conversation.channel);
    },
    [setSelectedChannel, setSelectedConversation, setSelectedServer],
  );

  const mapDmConversations = useMemo(() => {
    return conversations.map((conversation) => {
      if (conversation.users.length != 2) {
        toast.error("DM servers should have only 2 members");
        return null;
      }

      return (
        <Conversation
          key={conversation.server.server_id}
          handleClick={handleConversationClick}
          conversation={conversation}
        />
      );
    });
  }, [conversations, handleConversationClick]);

  return (
    <nav className="scrollbar-sidebar visible-on-hover flex-1 overflow-y-scroll pl-2">
      <h2 className="px-1.5 pt-2 text-xs font-semibold text-content-muted-100">
        DIRECT MESSAGES
      </h2>
      {loading ? (
        <span className="loading loading-spinner" />
      ) : (
        mapDmConversations
      )}
    </nav>
  );
};

export default Conversations;
