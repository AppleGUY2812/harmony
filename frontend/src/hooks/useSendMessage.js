import { useCallback } from "react";
import useServerStore from "../zustand/useServerStore";
import { useAuthContext } from "../context/AuthContext";

// Put a confirmed message in the open channel: replace the optimistic copy with the
// same nonce, skip it if already shown, else append. Called by both the HTTP
// response and the websocket echo, whichever arrives first wins.
export const upsertMessage = (message, nonce) => {
  const { messages, setMessages, selectedChannel } = useServerStore.getState();
  if (message.channel_id !== selectedChannel?.channel_id) return;
  if (messages.some((m) => m.message_id === message.message_id)) return;
  if (nonce && messages.some((m) => m.nonce === nonce)) {
    // keep the nonce so the React key (and the DOM node) stays the same
    setMessages(
      messages.map((m) => (m.nonce === nonce ? { ...message, nonce } : m)),
    );
  } else {
    setMessages([...messages, message]);
  }
};

const setStatus = (nonce, status) => {
  const { messages, setMessages } = useServerStore.getState();
  setMessages(messages.map((m) => (m.nonce === nonce ? { ...m, status } : m)));
};

const post = async (channel_id, message, nonce) => {
  try {
    const res = await fetch(
      `${import.meta.env.VITE_API_URL}/api/messages/send`,
      {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ channel_id, message, nonce }),
      },
    );
    const data = await res.json();
    if (!res.ok) throw new Error(data.error);
    upsertMessage(data, nonce);
  } catch {
    // the websocket echo may already have confirmed it (broadcast precedes the response)
    const { messages, setMessages } = useServerStore.getState();
    setMessages(
      messages.map((m) =>
        m.nonce === nonce && !m.message_id ? { ...m, status: "failed" } : m,
      ),
    );
  }
};

const useSendMessage = () => {
  const { authUser } = useAuthContext();

  const sendMessage = useCallback(
    (message) => {
      const { selectedChannel, addMessage } = useServerStore.getState();
      // unique per client is enough (crypto.randomUUID needs a secure context)
      const nonce = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
      addMessage({
        nonce,
        status: "sending",
        user_id: authUser.user_id,
        channel_id: selectedChannel.channel_id,
        message,
        timestamp: new Date().toISOString(),
        display_username: authUser.display_username,
        s3_icon_key: authUser.s3_icon_key,
      });
      post(selectedChannel.channel_id, message, nonce);
    },
    [authUser],
  );

  // ponytail: no server-side nonce dedupe; a retry after a lost response can duplicate.
  const retryMessage = useCallback((pending) => {
    setStatus(pending.nonce, "sending");
    post(pending.channel_id, pending.message, pending.nonce);
  }, []);

  return { sendMessage, retryMessage };
};

export default useSendMessage;
