import PropTypes from "prop-types";
import { formatTime } from "../../utils/formatTime";
import CloudFrontImage from "../../utils/CloudFrontImage";
const Message = ({ message, handleClick, onRetry }) => {
  const formatted_date = formatTime(message.timestamp);

  return (
    <div
      className={`relative mt-4 rounded-md pl-16 ${message.status === "sending" ? "opacity-50" : ""}`}
    >
      <div
        onClick={handleClick}
        className="absolute left-4 h-10 w-10 select-none overflow-hidden rounded-md bg-primary hover:cursor-pointer"
      >
        {message.s3_icon_key ? (
          <CloudFrontImage
            className="h-10 w-10"
            folder="user-icons"
            iconKey={message.s3_icon_key}
            thumb
          />
        ) : (
          <img
            draggable={false}
            className="h-10 w-10"
            src={`https://robohash.org/${message.display_username}`}
          />
        )}
      </div>
      <h3 className="relative h-5 leading-[1.25rem]">
        <span
          onClick={handleClick}
          className="mr-1 align-baseline font-medium leading-[1.25rem] text-content-header hover:cursor-pointer hover:underline"
        >
          {message.display_username}
        </span>
        <span className="align-middle text-xs leading-[1.25rem] text-content-muted-100">
          {formatted_date}
        </span>
      </h3>
      <div
        className={`whitespace-break-spaces text-base font-normal leading-[1.25rem] ${message.status === "failed" ? "text-error" : "text-content-normal"}`}
      >
        <span>{message.message}</span>
      </div>
      {message.status === "failed" ? (
        <div className="text-xs text-error">
          Failed to send.{" "}
          <button type="button" className="underline" onClick={onRetry}>
            Retry
          </button>
        </div>
      ) : null}
    </div>
  );
};

Message.propTypes = {
  message: PropTypes.shape({
    s3_icon_key: PropTypes.string,
    display_username: PropTypes.string,
    message: PropTypes.string,
    timestamp: PropTypes.string,
    user_id: PropTypes.number,
    status: PropTypes.oneOf(["sending", "failed"]),
  }),
  handleClick: PropTypes.func,
  onRetry: PropTypes.func,
};
export default Message;
