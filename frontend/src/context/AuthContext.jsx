import { createContext, useCallback, useContext, useState } from "react";
import PropTypes from "prop-types";

export const AuthContext = createContext(null);

export const useAuthContext = () => {
  return useContext(AuthContext);
};

export const AuthContextProvider = ({ children }) => {
  const [authUser, setAuthUser] = useState(
    JSON.parse(localStorage.getItem("user-info") || null),
  );

  const setLocalStorageAuthUser = (newUser) => {
    localStorage.setItem("user-info", JSON.stringify(newUser));
    setAuthUser(newUser);
  };
  // stable: the websocket effect calls it
  const removeLocalStorageAuthUser = useCallback(() => {
    localStorage.removeItem("user-info");
    setAuthUser(null);
  }, []);

  return (
    <AuthContext.Provider
      value={{
        authUser,
        setAuthUser,
        setLocalStorageAuthUser,
        removeLocalStorageAuthUser,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
};

AuthContextProvider.propTypes = {
  children: PropTypes.oneOfType([PropTypes.array, PropTypes.element]),
};
