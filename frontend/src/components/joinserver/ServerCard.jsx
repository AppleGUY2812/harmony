import PropTypes from "prop-types";
import useJoinServer from "../../hooks/useJoinServer";
import useServerStore from "../../zustand/useServerStore";
import { useMemo } from "react";
const ServerCard = ({ server }) => {
  const { loading, joinServer } = useJoinServer();
  const setSelectedServer = useServerStore((state) => state.setSelectedServer);
  const setSelectedChannel = useServerStore(
    (state) => state.setSelectedChannel,
  );
  const handleClick = async () => {
    await joinServer(server.server_id);
    setSelectedChannel(null);
    setSelectedServer(server);
  };
  const serverNameAbbrev = useMemo(
    () =>
      server.server_name
        .trim()
        .split(/([^a-zA-Z0-9]+)/)
        .filter((char) => char.match(/\S/))
        .map((word) => word[0])
        .join(""),
    [server.server_name],
  );

  return (
    <button
      disabled={loading}
      onClick={handleClick}
      className="server-card flex w-48 flex-col overflow-hidden rounded-md bg-base-300 hover:cursor-pointer"
    >
      <div className="bg-primary">
        {server.s3_icon_key ? (
          <img
            draggable={false}
            className="h-48 w-48"
            src={`https://${import.meta.env.VITE_CLOUDFRONT_IMAGE_URL}/server-icons/${server.s3_icon_key}`}
          />
        ) : (
          <div
            style={{
              fontSize: `clamp(2.5rem, ${16 / serverNameAbbrev.length}rem, 4rem)`,
            }}
            className="flex h-48 w-48 items-center justify-center text-content-normal"
          >
            {serverNameAbbrev}
          </div>
        )}
      </div>

      <div className="w-full px-2 py-1">
        <h3 className="truncate text-left font-semibold text-content-normal">
          {server.server_name}
        </h3>
        <div className="flex items-center justify-between text-xs text-content-normal">
          <div className="flex items-center gap-1">
            <div className="h-[8px] w-[8px] rounded-full bg-content-normal"></div>
            <p>{server.members} Members</p>
          </div>
          <p>
            <span className="select-none">ID: </span>
            <span className="rounded-md bg-base-50 px-1 py-0.5">
              {server.server_id}
            </span>
          </p>
        </div>
      </div>
    </button>
  );
};

ServerCard.propTypes = {
  server: PropTypes.shape({
    server_id: PropTypes.number,
    server_type: PropTypes.string,
    members: PropTypes.number,
    server_name: PropTypes.string,
    s3_icon_key: PropTypes.string,
  }),
};

export default ServerCard;
