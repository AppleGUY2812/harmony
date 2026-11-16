import { useState } from "react";
import { useAuthContext } from "../context/AuthContext";
import toast from "react-hot-toast";

const useLogin = () => {
  const [loading, setLoading] = useState(false);
  const [guestLoading, setGuestLoading] = useState(false);
  const { setAuthUser } = useAuthContext();

  const authenticate = async (path, body, setBusy) => {
    setBusy(true);
    try {
      const res = await fetch(
        `${import.meta.env.VITE_API_URL}/api/auth/${path}`,
        {
          method: "POST",
          credentials: "include",
          headers: { "Content-Type": "application/json" },
          body: body && JSON.stringify(body),
        },
      );

      const data = await res.json();

      if (!res.ok) {
        throw new Error(data.error);
      }

      localStorage.setItem("user-info", JSON.stringify(data));
      setAuthUser(data);
    } catch (error) {
      toast.error(error.message);
    } finally {
      setBusy(false);
    }
  };

  const login = async ({ username, password }) => {
    const success = handleInputErrors({ username, password });
    if (!success) return;
    await authenticate("login", { username, password }, setLoading);
  };

  const guestLogin = () => authenticate("guest", null, setGuestLoading);

  return { loading, login, guestLoading, guestLogin };
};

export default useLogin;

function handleInputErrors({ username, password }) {
  if (!username || !password) {
    toast.error("Please fill in all fields");
    return false;
  }
  return true;
}
