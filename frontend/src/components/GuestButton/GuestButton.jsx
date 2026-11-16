import useLogin from "../../hooks/useLogin";

const GuestButton = () => {
  const { guestLoading, guestLogin } = useLogin();

  return (
    <button
      type="button"
      disabled={guestLoading}
      onClick={guestLogin}
      className="mt-2 h-10 w-full rounded-md font-semibold text-button hover:underline"
    >
      {guestLoading ? (
        <span className="loading loading-spinner" />
      ) : (
        "Continue as guest"
      )}
    </button>
  );
};
export default GuestButton;
