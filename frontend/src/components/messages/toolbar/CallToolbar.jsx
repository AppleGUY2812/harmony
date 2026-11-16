import { useEffect, useRef, useState } from "react";
import PropTypes from "prop-types";
import { MdCallEnd } from "react-icons/md";
import {
  FaMicrophone,
  FaMicrophoneSlash,
  FaVideo,
  FaVideoSlash,
} from "react-icons/fa";
import { useAuthContext } from "../../../context/AuthContext";
import useWebRTCStore from "../../../zustand/useWebRTCStore";
import ProfilePicture from "../../../utils/ProfilePicture";

// muted: remote audio plays from the store, so it survives leaving this view
const VideoTile = ({ stream, user }) => {
  const videoRef = useRef(null);
  useEffect(() => {
    if (videoRef.current) videoRef.current.srcObject = stream ?? null;
  }, [stream]);
  const hasVideo = !!stream?.getVideoTracks().length;

  return (
    <div className="relative flex aspect-video items-center justify-center overflow-hidden rounded-lg bg-base-300">
      <video
        ref={videoRef}
        className={hasVideo ? "h-full w-full object-cover" : "hidden"}
        autoPlay
        playsInline
        muted
      />
      {hasVideo || !user ? null : <ProfilePicture size={64} user={user} />}
      <span className="absolute bottom-1 left-1 max-w-[90%] truncate rounded bg-black/60 px-2 text-sm text-content-header">
        {user?.display_username ?? "..."}
      </span>
    </div>
  );
};

VideoTile.propTypes = {
  stream: PropTypes.object,
  user: PropTypes.object,
};

const CallButton = ({ onClick, title, className, children }) => (
  <button
    title={title}
    onClick={onClick}
    className={`flex h-12 w-12 items-center justify-center rounded-full hover:brightness-90 ${className}`}
  >
    {children}
  </button>
);

CallButton.propTypes = {
  onClick: PropTypes.func,
  title: PropTypes.string,
  className: PropTypes.string,
  children: PropTypes.node,
};

// video grid of the active call, resizable by dragging its bottom edge
const CallToolbar = () => {
  const { authUser } = useAuthContext();
  const localStream = useWebRTCStore((state) => state.localStream);
  const remoteStreams = useWebRTCStore((state) => state.remoteStreams);
  const participants = useWebRTCStore((state) => state.participants);
  const users = useWebRTCStore((state) => state.users);
  const micOn = useWebRTCStore((state) => state.micOn);
  const cameraOn = useWebRTCStore((state) => state.cameraOn);
  const toggleMic = useWebRTCStore((state) => state.toggleMic);
  const toggleCamera = useWebRTCStore((state) => state.toggleCamera);
  const leaveCall = useWebRTCStore((state) => state.leaveCall);
  const hasCamera = !!localStream?.getVideoTracks().length;
  const remotes = participants.filter((p) => p.user_id !== authUser.user_id);

  const [height, setHeight] = useState(300);
  const resizableRef = useRef(null);
  const isResizingRef = useRef(false);

  useEffect(() => {
    const handleMouseMove = (e) => {
      if (!isResizingRef.current) return;
      setHeight(e.clientY - resizableRef.current.getBoundingClientRect().top);
    };
    const handleMouseUp = () => {
      isResizingRef.current = false;
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, []);

  const handleMouseDown = () => {
    isResizingRef.current = true;
    document.body.style.userSelect = "none";
  };

  return (
    <div>
      <div
        ref={resizableRef}
        style={{ height }}
        className="flex max-h-[76vh] min-h-[28vh] flex-col bg-base-500"
      >
        <div className="grid flex-1 auto-rows-min grid-cols-[repeat(auto-fit,minmax(min(100%,220px),1fr))] content-center gap-2 overflow-y-auto p-2">
          <VideoTile stream={localStream} user={authUser} />
          {remotes.map((p) => (
            <VideoTile
              key={p.stream_id}
              stream={remoteStreams[p.stream_id]}
              user={users[p.user_id]}
            />
          ))}
        </div>
        <div className="flex justify-center gap-3 py-2">
          <CallButton
            title={micOn ? "Mute" : "Unmute"}
            onClick={toggleMic}
            className="bg-base-50"
          >
            {micOn ? (
              <FaMicrophone color="white" size="20" />
            ) : (
              <FaMicrophoneSlash color="white" size="20" />
            )}
          </CallButton>
          {hasCamera ? (
            <CallButton
              title={cameraOn ? "Turn camera off" : "Turn camera on"}
              onClick={toggleCamera}
              className="bg-base-50"
            >
              {cameraOn ? (
                <FaVideo color="white" size="20" />
              ) : (
                <FaVideoSlash color="white" size="20" />
              )}
            </CallButton>
          ) : null}
          <CallButton
            title="Leave call"
            onClick={leaveCall}
            className="bg-error"
          >
            <MdCallEnd color="white" size="24" />
          </CallButton>
        </div>
      </div>

      <div onMouseDown={handleMouseDown} className="h-1 cursor-ns-resize"></div>
    </div>
  );
};

export default CallToolbar;
