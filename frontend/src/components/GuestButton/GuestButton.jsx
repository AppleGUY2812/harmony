import PropTypes from "prop-types";

const GuestButton = ({ loading, disabled, onClick }) => (
  <button
    type="button"
    disabled={loading || disabled}
    onClick={onClick}
    className="mt-2 h-10 w-full rounded-md font-semibold text-button hover:underline"
  >
    {loading ? (
      <span className="loading loading-spinner" />
    ) : (
      "Continue as guest"
    )}
  </button>
);

GuestButton.propTypes = {
  loading: PropTypes.bool.isRequired,
  disabled: PropTypes.bool.isRequired,
  onClick: PropTypes.func.isRequired,
};

export default GuestButton;
