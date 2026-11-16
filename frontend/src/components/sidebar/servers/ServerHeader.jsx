import { RiArrowDropDownLine } from "react-icons/ri";
import { usePopupContext } from "../../popups/PopupContext";
import useServerStore from "../../../zustand/useServerStore";

const ServerHeader = () => {
  const { toggleServerDropdown } = usePopupContext();
  const selectedServer = useServerStore((state) => state.selectedServer);
  const handleClick = (event) => {
    event.stopPropagation();
    toggleServerDropdown();
  };
  return (
    <header className="h-12 border-b border-base-400 px-4 py-3">
      <div
        onClick={handleClick}
        className="flex cursor-pointer items-center justify-between rounded-md font-semibold text-neutral-200"
      >
        <h2 className="truncate">{selectedServer.server_name}</h2>
        <RiArrowDropDownLine className="flex-shrink-0" size={32} />
      </div>
    </header>
  );
};

export default ServerHeader;
