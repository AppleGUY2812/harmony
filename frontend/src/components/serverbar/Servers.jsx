import { useCallback, useEffect, useMemo } from "react";
import useGetServers from "../../hooks/useGetServers";
import Server from "./Server";
import useServerStore from "../../zustand/useServerStore";
import toast from "react-hot-toast";
import Conversation from "../sidebar/conversations/Conversation";

const Servers = () => {
  const { loading, servers: fetchedServers } = useGetServers("Server");
  const servers = useServerStore((state) => state.servers);
  const setServers = useServerStore((state) => state.setServers);
  const updateServerUnread = useServerStore(
    (state) => state.updateServerUnread,
  );
  const setSelectedConversation = useServerStore(
    (state) => state.setSelectedConversation,
  );
  const setSelectedServer = useServerStore((state) => state.setSelectedServer);
  const setSelectedChannel = useServerStore(
    (state) => state.setSelectedChannel,
  );

  useEffect(() => {
    if (fetchedServers) {
      setServers(fetchedServers);
      // a refetch after a reconnect can count the open server's messages as unread
      const { selectedServer, selectedChannel } = useServerStore.getState();
      if (selectedChannel) updateServerUnread(selectedServer.server_id, 0);
    }
  }, [fetchedServers, setServers, updateServerUnread]);

  const handleServerClick = useCallback(
    (server) => {
      setSelectedConversation(null);
      setSelectedServer(server);
      // TODO: CHANGE TO LAST VISITED CHANNEL?
      setSelectedChannel(null);
    },
    [setSelectedChannel, setSelectedServer, setSelectedConversation],
  );

  const mapServers = useMemo(
    () =>
      servers.map((server) => (
        <Server
          key={server.server_id}
          server={server}
          handleClick={handleServerClick}
        />
      )),
    [servers, handleServerClick],
  );

  return (
    <div className="scrollbar-none mb-2 flex-1 overflow-y-scroll">
      {mapServers}
    </div>
  );
};

export default Servers;
