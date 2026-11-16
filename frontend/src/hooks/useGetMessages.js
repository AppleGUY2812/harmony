import { useCallback, useEffect, useRef, useState } from "react";
import useServerStore from "../zustand/useServerStore";
import toast from "react-hot-toast";
import useUpdateLastReadMessage from "./useUpdateLastReadMessage";

const PAGE_SIZE = 50;

const fetchJson = async (url) => {
  try {
    const res = await fetch(url, { credentials: "include" });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error);
    return data;
  } catch (error) {
    toast.error(error.message);
    return null;
  }
};

const fetchPage = (channel_id, before) =>
  fetchJson(
    `${import.meta.env.VITE_API_URL}/api/messages/get/${channel_id}?limit=${PAGE_SIZE}` +
      (before ? `&before=${before}` : ""),
  );

const isOpen = (channel_id) =>
  useServerStore.getState().selectedChannel?.channel_id === channel_id;

// Loads the newest page into the store on channel open; loadOlder() prepends the
// page before the oldest loaded message. Returns how many messages were prepended.
const useGetMessages = () => {
  const [loading, setLoading] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const loadingOlder = useRef(false);
  const selectedServer = useServerStore((state) => state.selectedServer);
  const channelId = useServerStore(
    (state) => state.selectedChannel?.channel_id,
  );
  const setMessages = useServerStore((state) => state.setMessages);
  const setUsers = useServerStore((state) => state.setUsers);
  const updateConversationUnread = useServerStore(
    (state) => state.updateConversationUnread,
  );
  const updateServerUnread = useServerStore(
    (state) => state.updateServerUnread,
  );
  const { UpdateLastReadMessage } = useUpdateLastReadMessage();
  const { server_id, server_type } = selectedServer ?? {};

  useEffect(() => {
    if (!channelId || !server_id) return;
    const getMessages = async () => {
      setLoading(true);
      setHasMore(false);
      const [usersData, messagesData] = await Promise.all([
        fetchJson(
          `${import.meta.env.VITE_API_URL}/api/servers/users/${server_id}`,
        ),
        fetchPage(channelId),
      ]);

      if (usersData && messagesData && isOpen(channelId)) {
        setUsers(usersData);
        setMessages(messagesData);
        setHasMore(messagesData.length === PAGE_SIZE);

        const last_message_id = messagesData.slice(-1)[0]?.message_id;
        if (last_message_id) {
          UpdateLastReadMessage(server_id, last_message_id);
        }

        // clicking on any of the server's channels will get rid of the unread
        if (server_type === "Dm" || server_type === "GroupChat") {
          updateConversationUnread(server_id, 0);
        } else if (server_type === "Server") {
          updateServerUnread(server_id, 0);
        }
      }

      setLoading(false);
    };

    getMessages();
  }, [
    server_id,
    server_type,
    channelId,
    setMessages,
    setUsers,
    UpdateLastReadMessage,
    updateConversationUnread,
    updateServerUnread,
  ]);

  const loadOlder = useCallback(async () => {
    if (loadingOlder.current || !hasMore) return 0;
    const oldest = useServerStore
      .getState()
      .messages.find((m) => m.message_id)?.message_id;
    if (!oldest) return 0;

    loadingOlder.current = true;
    const page = await fetchPage(channelId, oldest);
    loadingOlder.current = false;
    if (!page || !isOpen(channelId)) return 0;

    setHasMore(page.length === PAGE_SIZE);
    const { messages } = useServerStore.getState();
    const seen = new Set(messages.map((m) => m.message_id));
    const older = page.filter((m) => !seen.has(m.message_id));
    setMessages([...older, ...messages]);
    return older.length;
  }, [channelId, hasMore, setMessages]);

  return { loading, hasMore, loadOlder };
};

export default useGetMessages;
