import { useCallback, useEffect, useState } from "react";
import { useWebsocketContext } from "../../context/WebsocketContext";
import useServerStore from "../../zustand/useServerStore";
import toast from "react-hot-toast";
import useUpdateLastReadMessage from "../useUpdateLastReadMessage";
import { upsertMessage } from "../useSendMessage";
const useListenMessages = () => {
  const { websocket } = useWebsocketContext();
  const selectedChannel = useServerStore((state) => state.selectedChannel);
  const selectedServer = useServerStore((state) => state.selectedServer);
  const updateConversationUnread = useServerStore(
    (state) => state.updateConversationUnread,
  );
  const conversations = useServerStore((state) => state.conversations);
  const setConversations = useServerStore((state) => state.setConversations);
  const updateServerUnread = useServerStore(
    (state) => state.updateServerUnread,
  );
  const { UpdateLastReadMessage } = useUpdateLastReadMessage();

  const [ping] = useState(new Audio("/harmony_ping_sound.mp3"));
  const playSound = useCallback(() => {
    const newPing = new Audio(ping.src);
    newPing.play();
  }, [ping]);
  useEffect(() => {
    if (!websocket) return;
    const handleIncomingMessage = (event) => {
      try {
        const ws_event = JSON.parse(event.data);
        if (ws_event.event_type === "Message") {
          // update the order of conversations
          // IMPORTANT, needs to go before updating unread count, or this will set an old unread count
          if (
            ws_event.data.server_type === "Dm" ||
            ws_event.data.server_type === "GroupChat"
          ) {
            setConversations(
              conversations.map((conversation) => {
                if (conversation.server.server_id === ws_event.data.server_id) {
                  return {
                    ...conversation,
                    server: {
                      ...conversation.server,
                      last_message_at: ws_event.data.message.timestamp,
                    },
                  };
                }
                return conversation;
              }),
            );
          }
          // user is looking at channel of the received message
          if (
            ws_event.data.message.channel_id === selectedChannel?.channel_id
          ) {
            upsertMessage(ws_event.data.message, ws_event.data.nonce);
            UpdateLastReadMessage(
              selectedServer.server_id,
              ws_event.data.message.message_id,
            );
          } else {
            // increment unread message for the server
            if (
              ws_event.data.server_type === "Dm" ||
              ws_event.data.server_type === "GroupChat"
            ) {
              updateConversationUnread(ws_event.data.server_id, "increment");
              playSound();
            } else if (ws_event.data.server_type === "Server") {
              updateServerUnread(ws_event.data.server_id, "increment");
            }
          }
        }
      } catch (error) {
        toast.error(error.message);
      }
    };

    websocket.addEventListener("message", handleIncomingMessage);
    return () => {
      websocket.removeEventListener("message", handleIncomingMessage);
    };
  }, [
    websocket,
    UpdateLastReadMessage,
    conversations,
    selectedChannel?.channel_id,
    selectedServer?.server_id,
    setConversations,
    updateConversationUnread,
    updateServerUnread,
    playSound,
  ]);
};

export default useListenMessages;
