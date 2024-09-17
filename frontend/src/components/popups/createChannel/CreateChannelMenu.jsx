import { useCallback, useEffect, useRef, useState } from "react";
import { TbBrandAmongUs } from "react-icons/tb";
import { CSSTransition } from "react-transition-group";
import { IoIosClose } from "react-icons/io";
import PropTypes from "prop-types";
import { usePopupContext } from "../PopupContext";
import useCreateChannel from "../../../hooks/useCreateChannel";
import useServer from "../../../zustand/useServer";
import toast from "react-hot-toast";
const CreateChannelMenu = () => {
  const menuRef = useRef(null);
  const inputRef = useRef(null);
  const selectedServer = useServer((state) => state.selectedServer);
  const [buttonsDisabled, setButtonsDisabled] = useState(false);
  const { loading, createChannel } = useCreateChannel();
  const { channelMenu, setChannelMenuVisible, setModalOverlayVisible } =
    usePopupContext();

  const [channelName, setChannelName] = useState("");

  const handleInputChange = useCallback((event) => {
    setChannelName(event.target.value);
  }, []);

  const onClose = useCallback(() => {
    setChannelMenuVisible(false);
    setModalOverlayVisible(false);
  }, [setChannelMenuVisible, setModalOverlayVisible]);

  useEffect(() => {
    const handleKeyDown = (event) => {
      event.stopPropagation();
      inputRef.current?.focus();
    };
    const handleClickOutside = (event) => {
      if (menuRef.current && !menuRef.current.contains(event.target)) {
        onClose();
      }
    };
    document.addEventListener("click", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("click", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose]);

  const handleSubmit = async (event) => {
    event.preventDefault();
    console.log(channelName.length);
    if (channelName.length > 30) {
      toast.error("Channel name too long");
    } else {
      await createChannel(selectedServer.server_id, channelName);
      onClose();
    }
  };

  return (
    <CSSTransition
      in={channelMenu.visible}
      nodeRef={menuRef}
      timeout={200}
      classNames="channel-menu"
      unmountOnExit
      onEntering={() => {
        setButtonsDisabled(true);
        setChannelName("");
      }}
      onEntered={() => setButtonsDisabled(false)}
      onExiting={() => setButtonsDisabled(true)}
    >
      <div className="absolute left-0 top-0 flex h-screen w-screen items-center justify-center">
        <form onSubmit={handleSubmit} ref={menuRef}>
          <fieldset disabled={buttonsDisabled}>
            <div className="w-[460px] rounded-md bg-green-200 p-4">
              <div className="mb-4 flex items-center justify-between">
                <h1 className="text-xl font-medium">Create Channel</h1>
                <button onClick={onClose} className="hover:text-green-500">
                  <IoIosClose className="flex-shrink-0" size={40} />
                </button>
              </div>
              <div className="mb-8">
                <h2 className="mb-2 text-xs font-bold">CHANNEL NAME</h2>
                <div className="flex h-10 items-center gap-2 rounded-md bg-base-100 pl-2">
                  <TbBrandAmongUs className="flex-shrink-0" size={20} />
                  <input
                    ref={inputRef}
                    value={channelName}
                    onChange={handleInputChange}
                    type="text"
                    placeholder="new-channel"
                    className="h-full flex-1 bg-transparent"
                  />
                </div>
              </div>
              <div className="flex justify-end gap-2">
                <button
                  onClick={onClose}
                  className="h-9 rounded-md px-4 hover:underline"
                >
                  Cancel
                </button>
                <button
                  disabled={!channelName || channelName.length > 30}
                  className={`h-9 rounded-md ${!channelName || channelName.length > 30 ? "cursor-not-allowed bg-red-500 text-yellow-500" : "bg-cyan-400 text-lime-400"} px-4`}
                >
                  Create Channel
                </button>
              </div>
            </div>
          </fieldset>
        </form>
      </div>
    </CSSTransition>
  );
};

CreateChannelMenu.propTypes = {
  setVisible: PropTypes.func,
};
export default CreateChannelMenu;
