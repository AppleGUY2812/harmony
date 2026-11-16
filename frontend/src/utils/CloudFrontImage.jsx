import PropTypes from "prop-types";
import { useEffect, useState } from "react";
import useImageSignatureStore, {
  ensureImageSignature,
  iconUrl,
} from "../zustand/useImageSignatureStore";

// Signed CloudFront <img>. Renders nothing until the signature loads (avoids a 403 flash).
// thumb: old uploads have no thumbnail, so on error fall back once to full size.
// Failure is keyed on the signed URL, so a 403 from an expired query retries after refresh.
const CloudFrontImage = ({ folder, iconKey, thumb = false, className }) => {
  const query = useImageSignatureStore((state) => state.query);
  const [failedThumbSrc, setFailedThumbSrc] = useState(null);
  useEffect(ensureImageSignature, []);

  if (query === null) return null;
  const thumbSrc = thumb && iconUrl(folder, iconKey, { thumb: true });
  const useThumb = thumb && failedThumbSrc !== thumbSrc;
  return (
    <img
      draggable={false}
      className={className}
      src={useThumb ? thumbSrc : iconUrl(folder, iconKey)}
      onError={useThumb ? () => setFailedThumbSrc(thumbSrc) : undefined}
    />
  );
};

CloudFrontImage.propTypes = {
  folder: PropTypes.oneOf(["user-icons", "server-icons"]).isRequired,
  iconKey: PropTypes.string.isRequired,
  thumb: PropTypes.bool,
  className: PropTypes.string,
};

export default CloudFrontImage;
