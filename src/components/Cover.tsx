import { useEffect, useState } from "react";

interface CoverProps {
  url: string;
  alt?: string;
  /** Zooms with the hovered card around it, as much as the theme says (grid cards). */
  zoomOnHover?: boolean;
  onError: () => void;
}

/**
 * Cover art inside a `relative overflow-hidden` 2:3 frame.
 *
 * Most covers are vertical and simply fill the frame. When only landscape art exists (a
 * store header used as a last resort) it is shown whole over a blurred copy of itself, the
 * way the Steam client presents games without a library capsule, instead of being cropped
 * to an unrecognisable strip.
 */
export function Cover({ url, alt = "", zoomOnHover = false, onError }: CoverProps) {
  const [isLandscape, setIsLandscape] = useState(false);

  useEffect(() => {
    setIsLandscape(false);
  }, [url]);

  return (
    <>
      {isLandscape && (
        <img
          src={url}
          alt=""
          aria-hidden="true"
          draggable={false}
          className="absolute inset-0 h-full w-full scale-150 object-cover opacity-50 blur-xl"
        />
      )}
      <img
        src={url}
        alt={alt}
        loading="lazy"
        draggable={false}
        onError={onError}
        onLoad={(e) => {
          const { naturalWidth, naturalHeight } = e.currentTarget;
          setIsLandscape(naturalWidth > naturalHeight * 1.1);
        }}
        className={
          isLandscape
            ? "relative h-full w-full object-contain"
            : `h-full w-full object-cover ${zoomOnHover ? "game-card-art" : ""}`
        }
      />
    </>
  );
}
