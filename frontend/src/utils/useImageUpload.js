import { useCallback, useEffect, useState } from "react";
import toast from "react-hot-toast";

const MAX_BYTES = 10 * 1024 * 1024;

// The backend crops/resizes/encodes; we just send the original file.
const useImageUpload = () => {
  const [icon, setIcon] = useState(null);
  const [previewUrl, setPreviewUrl] = useState(null);
  const [iconChanged, setIconChanged] = useState(false);

  const handleFileChange = useCallback((event) => {
    const file = event.target.files[0];
    if (!file) return false;
    if (file.size > MAX_BYTES) {
      toast.error("Image must be 10 MB or smaller");
      event.target.value = "";
      return false;
    }
    setIcon(file);
    setPreviewUrl(URL.createObjectURL(file));
    setIconChanged(true);
    return true;
  }, []);

  // Release the old blob when it's replaced, reset to null, or on unmount.
  useEffect(
    () => () => {
      if (previewUrl?.startsWith("blob:")) URL.revokeObjectURL(previewUrl);
    },
    [previewUrl],
  );

  return {
    icon,
    previewUrl,
    iconChanged,
    setIconChanged,
    handleFileChange,
    setPreviewUrl,
    setIcon,
  };
};

export default useImageUpload;
