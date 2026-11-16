import { useCallback, useEffect, useRef, useState } from "react";
import { CSSTransition } from "react-transition-group";
import { IoIosClose } from "react-icons/io";
import PropTypes from "prop-types";
import { usePopupContext } from "../PopupContext";
import useServerStore from "../../../zustand/useServerStore";
import toast from "react-hot-toast";
import { useAuthContext } from "../../../context/AuthContext";
import useCreateServer from "../../../hooks/useCreateServer";
import UploadImageIcon from "./UploadImageIcon";
import useImageUpload from "../../../utils/useImageUpload";

const CreateServerMenu = () => {
  const menuRef = useRef(null);
  const setSelectedServer = useServerStore((state) => state.setSelectedServer);
  const setSelectedChannel = useServerStore(
    (state) => state.setSelectedChannel,
  );
  const [buttonsDisabled, setButtonsDisabled] = useState(false);
  const { createServer } = useCreateServer();
  const { serverMenu, setServerMenuVisible, setModalOverlayVisible } =
    usePopupContext();

  const { authUser } = useAuthContext();
  const inputRef = useRef(null);

  const [serverName, setServerName] = useState("");
  const {
    icon: serverIcon,
    setIcon: setServerIcon,
    previewUrl,
    handleFileChange,
  } = useImageUpload();

  const handleInputChange = useCallback((event) => {
    setServerName(event.target.value);
  }, []);

  const onClose = useCallback(() => {
    setServerMenuVisible(false);
    setModalOverlayVisible(false);
  }, [setServerMenuVisible, setModalOverlayVisible]);

  useEffect(() => {
    const handleKeyDown = (event) => {
      if (event.key === "Escape") {
        onClose();
      }
      // apparently focusing on another inputref prevents the messageinput from listening
      inputRef.current?.focus();
    };
    const handleClickOutside = (event) => {
      if (menuRef.current && !menuRef.current.contains(event.target)) {
        onClose();
      }
    };
    document.addEventListener("click", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);

    if (!serverMenu.visible) {
      document.removeEventListener("click", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    }
    return () => {
      document.removeEventListener("click", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [onClose, serverMenu.visible]);

  const submit = async () => {
    if (!serverName || serverName.length > 30) {
      toast.error("Server name too long");
    } else {
      const { server: newServer, channel: newChannel } = await createServer(
        serverName,
        serverIcon,
        [],
        "Server",
      );
      if (newServer) {
        // server added in listen server created
        setSelectedServer(newServer);
        setSelectedChannel(newChannel);
      }
      onClose();
    }
  };
  const handleSubmit = async (event) => {
    event.preventDefault();
    await submit();
  };

  const handleInputKeyDown = async (event) => {
    // for some reason enter submits the form
    if (event.key === "Enter") {
      event.preventDefault();
      await submit();
    }
  };

  return (
    <CSSTransition
      in={serverMenu.visible}
      nodeRef={menuRef}
      timeout={200}
      classNames="modal-menu"
      unmountOnExit
      onEntering={() => {
        setButtonsDisabled(true);
        setServerIcon(null);
        setServerName(`${authUser.display_username}'s server`);
      }}
      onEntered={() => setButtonsDisabled(false)}
      onExiting={() => setButtonsDisabled(true)}
    >
      <div className="absolute left-0 top-0 flex h-screen w-screen items-center justify-center">
        <form onSubmit={handleSubmit} ref={menuRef}>
          <fieldset disabled={buttonsDisabled}>
            <div className="w-[460px] rounded-md bg-base-100 p-4">
              <div className="mb-4 flex items-center justify-between">
                <h1 className="text-xl font-bold text-content-header">
                  Create Server
                </h1>
                <button
                  onClick={onClose}
                  className="text-button hover:text-content-normal"
                >
                  <IoIosClose className="flex-shrink-0" size={40} />
                </button>
              </div>
              <div className="mb-4 flex justify-center">
                <div className="relative h-[80px] w-[80px]">
                  {serverIcon ? (
                    <img
                      src={previewUrl}
                      className="h-[80px] w-[80px] rounded-md object-cover"
                    />
                  ) : (
                    <UploadImageIcon />
                  )}

                  <input
                    onChange={handleFileChange}
                    style={{ fontSize: "0px" }}
                    type="file"
                    accept=".jpg,.jpeg,.png,.gif,.webp"
                    className="absolute left-0 top-0 h-full w-full opacity-0 hover:cursor-pointer"
                  />
                </div>
              </div>
              <div className="mb-8">
                <h2 className="mb-2 text-xs font-bold text-button">
                  SERVER NAME
                </h2>
                <div className="h-10 rounded-md bg-base-400 pl-2 text-content-normal">
                  <input
                    onKeyDown={handleInputKeyDown}
                    ref={inputRef}
                    value={serverName}
                    onChange={handleInputChange}
                    type="text"
                    className="h-full w-full bg-transparent"
                  />
                </div>
              </div>

              <div className="flex justify-end gap-2">
                <button
                  onClick={onClose}
                  className="h-9 rounded-md px-4 text-button hover:underline"
                >
                  Cancel
                </button>
                <button
                  disabled={!serverName || serverName.length > 30}
                  className={`h-9 rounded-md text-content-normal ${!serverName || serverName.length > 30 ? "cursor-not-allowed bg-error" : "bg-primary"} px-4`}
                >
                  Create Server
                </button>
              </div>
            </div>
          </fieldset>
        </form>
      </div>
    </CSSTransition>
  );
};

CreateServerMenu.propTypes = {
  setVisible: PropTypes.func,
};
export default CreateServerMenu;
