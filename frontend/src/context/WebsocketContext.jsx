import PropTypes from "prop-types";
import { createContext, useContext, useEffect, useState } from "react";
import { useAuthContext } from "./AuthContext";

export const WebsocketContext = createContext(null);

// websocket: current socket (may be connecting or closed while reconnecting)
// epoch: 0 until the first reconnect, then +1 every time a reconnect opens;
// anything that may have missed events while disconnected refetches when it changes
export const useWebsocketContext = () => {
  return useContext(WebsocketContext);
};
export const WebsocketContextProvider = ({ children }) => {
  const [websocket, setWebsocket] = useState(null);
  const [epoch, setEpoch] = useState(0);
  const { authUser } = useAuthContext();
  const userId = authUser?.user_id;

  useEffect(() => {
    if (!userId) return;
    const base =
      import.meta.env.VITE_API_URL ||
      `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}`;
    let ws;
    let timer;
    let attempts = 0;
    let opened = false;
    let stopped = false;

    const connect = () => {
      clearTimeout(timer);
      ws = new WebSocket(`${base}/ws/chat_ws/${userId}`);
      ws.onopen = () => {
        attempts = 0;
        if (opened) setEpoch((e) => e + 1);
        opened = true;
      };
      ws.onclose = () => {
        if (stopped) return;
        // exponential backoff with full jitter, capped at 15s
        const delay = Math.random() * Math.min(15000, 500 * 2 ** attempts++);
        timer = setTimeout(connect, delay);
      };
      setWebsocket(ws);
    };
    const reconnectNow = () => {
      if (ws.readyState === WebSocket.CLOSED) connect();
    };
    const onVisible = () => {
      if (document.visibilityState === "visible") reconnectNow();
    };

    connect();
    window.addEventListener("online", reconnectNow);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      stopped = true;
      clearTimeout(timer);
      window.removeEventListener("online", reconnectNow);
      document.removeEventListener("visibilitychange", onVisible);
      ws.close();
      setWebsocket(null);
    };
  }, [userId]);

  return (
    <WebsocketContext.Provider value={{ websocket, epoch }}>
      {children}
    </WebsocketContext.Provider>
  );
};

WebsocketContextProvider.propTypes = {
  children: PropTypes.oneOfType([PropTypes.array, PropTypes.element]),
};
