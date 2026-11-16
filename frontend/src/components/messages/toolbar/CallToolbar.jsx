import { useAuthContext } from "../../../context/AuthContext";
import useServerStore from "../../../zustand/useServerStore";
import { TbBrandAmongUs } from "react-icons/tb";
import CallButton from "./CallButton";
import { MdCallEnd } from "react-icons/md";
import ProfilePicture from "../../../utils/ProfilePicture";
import { useEffect, useRef, useState } from "react";
import useWebRTCStore from "../../../zustand/useWebRTCStore";

// toolbar contains information about the selected channel
const CallToolbar = ({ endCallButtonClick, localVideoRef, remoteVideoRef }) => {
  const selectedChannel = useServerStore((state) => state.selectedChannel);
  const selectedConversation = useServerStore(
    (state) => state.selectedConversation,
  );
  // handle groupchat later, only handling dm rn
  const { authUser } = useAuthContext();
  const otherUser = selectedConversation?.users.filter(
    (user) => user.user_id !== authUser.user_id,
  )[0];

  const [height, setHeight] = useState(230);
  const resizableRef = useRef(null);
  const isResizingRef = useRef(false);

  const handleMouseMove = (e) => {
    if (!isResizingRef.current) return;
    console.log(2);
    const newHeight =
      e.clientY - resizableRef.current.getBoundingClientRect().top;
    setHeight(newHeight);
  };
  const handleMouseUp = () => {
    isResizingRef.current = false;
    document.body.style.userSelect = "";
  };
  const handleMouseDown = () => {
    isResizingRef.current = true;
    document.body.style.userSelect = "none";
  };

  useEffect(() => {
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.addEventListener("mousemove", handleMouseMove);
      window.addEventListener("mouseup", handleMouseUp);
    };
  }, []);

  // fix the dumb flex wrap
  return (
    <div>
      <div
        ref={resizableRef}
        style={{ height }}
        className="flex max-h-[76vh] min-h-[28vh] flex-col justify-between bg-base-500"
      >
        {/* <div className="flex select-none items-center justify-between bg-transparent px-4 py-2 text-content-header">
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
        </div>
        <div className="my-4 flex flex-1 items-center justify-center bg-transparent">
          <ProfilePicture size={80} user={authUser} />
        </div>
        <div className="flex justify-center bg-transparent pb-4">
          <button
            onClick={endCallButtonClick}
            className="flex h-14 w-14 items-center justify-center rounded-full bg-error hover:brightness-90"
          >
            <MdCallEnd color="white" size="24" />
          </button>
        </div> */}

        <div className="flex h-full flex-wrap justify-center gap-2 bg-white px-2">
          <video
            style={{
              height: "100%",
            }}
            ref={localVideoRef}
            autoPlay
            playsInline
            muted
          />

          <video
            style={{
              height: "100%",
            }}
            ref={remoteVideoRef}
            autoPlay
            playsInline
          />
        </div>
      </div>

      <div onMouseDown={handleMouseDown} className="h-1 cursor-ns-resize"></div>
    </div>
  );
};

export default CallToolbar;
