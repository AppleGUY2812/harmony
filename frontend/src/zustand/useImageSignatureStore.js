import { create } from "zustand";

const cloudFront = import.meta.env.VITE_CLOUDFRONT_IMAGE_URL;

// Signed CloudFront query string ("" when the backend has no signing key); null until loaded.
// Without CloudFront the backend serves images unsigned at /images, so start ready.
const useImageSignatureStore = create(() => ({
  query: cloudFront ? null : "",
  expiresAt: 0,
}));

let loading = false;
let listening = false;
let timer;

// Due for refresh ~10 min before expiry. Checked on image mount and tab show too,
// because timers don't count time the laptop slept or the tab was frozen.
const stale = () =>
  Date.now() >
  useImageSignatureStore.getState().expiresAt * 1000 - 10 * 60 * 1000;

const loadIn = (ms) => {
  clearTimeout(timer);
  timer = setTimeout(loadSignature, ms);
};

const loadSignature = async () => {
  if (loading) return;
  loading = true;
  try {
    const res = await fetch(
      `${import.meta.env.VITE_API_URL}/api/images/signature`,
      { credentials: "include" },
    );
    // logged out; the next image mount tries again
    if (res.status === 401) return;
    if (!res.ok) throw new Error(res.statusText);
    const { query, expires_at } = await res.json();
    useImageSignatureStore.setState({ query, expiresAt: expires_at });
    loadIn(
      Math.max(expires_at * 1000 - Date.now() - 10 * 60 * 1000, 60 * 1000),
    );
  } catch {
    loadIn(30 * 1000);
  } finally {
    loading = false;
  }
};

// Called by every CloudFront image mount.
export const ensureImageSignature = () => {
  if (!cloudFront) return;
  if (!listening) {
    listening = true;
    document.addEventListener("visibilitychange", () => {
      if (!document.hidden && stale()) loadSignature();
    });
  }
  if (stale()) loadSignature();
};

// Only valid once query has loaded.
export const iconUrl = (folder, key, { thumb = false } = {}) => {
  const { query } = useImageSignatureStore.getState();
  const path = thumb ? `${folder}/thumb/${key}` : `${folder}/${key}`;
  if (!cloudFront) return `${import.meta.env.VITE_API_URL}/images/${path}`;
  return `https://${cloudFront}/${path}${query ? `?${query}` : ""}`;
};

export default useImageSignatureStore;
