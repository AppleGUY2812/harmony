import PropTypes from "prop-types";
import CloudFrontImage from "./CloudFrontImage";
const ProfilePicture = ({ user, size }) => {
  return (
    <div
      style={{ height: size, width: size }}
      className="overflow-hidden rounded-md bg-primary"
    >
      {user.s3_icon_key ? (
        <CloudFrontImage
          className="h-full w-full"
          folder="user-icons"
          iconKey={user.s3_icon_key}
          thumb={size <= 64}
        />
      ) : (
        <img
          className="h-full w-full"
          draggable={false}
          src={`https://robohash.org/${user.display_username}`}
        />
      )}
    </div>
  );
};

ProfilePicture.propTypes = {
  user: PropTypes.shape({
    date_joined: PropTypes.string,
    display_username: PropTypes.string,
    s3_icon_key: PropTypes.string,
    user_id: PropTypes.number,
  }),
  size: PropTypes.number,
};

export default ProfilePicture;
