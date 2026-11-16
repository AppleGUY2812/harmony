import PropTypes from "prop-types";
const ProfilePicture = ({ user, size }) => {
  return (
    <div
      style={{ height: size, width: size }}
      className="overflow-hidden rounded-md bg-primary"
    >
      <img
        className="h-full w-full"
        draggable={false}
        src={
          user.s3_icon_key
            ? `https://${import.meta.env.VITE_CLOUDFRONT_IMAGE_URL}/user-icons/${user.s3_icon_key}`
            : `https://robohash.org/${user.display_username}`
        }
      />
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
