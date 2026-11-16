import { useEffect, useRef } from "react";
import type { AmbienceKind } from "../lib/ambience";

interface Particle {
  x: number;
  y: number;
  /** Size in pixels. */
  size: number;
  /** Fall and drift, in pixels per second. */
  speed: number;
  drift: number;
  /** Where it is in its sway and in its turning. */
  phase: number;
  spin: number;
  color: string;
  alpha: number;
}

/** How each kind looks: how many, how fast, what shape, which of the theme's colors. */
interface Look {
  count: number;
  size: [number, number];
  speed: [number, number];
  drift: [number, number];
  shape: "dot" | "streak" | "leaf" | "square" | "heart" | "haze";
  /** Names of theme colors, or fixed ones where the thing has a color of its own. */
  colors: string[];
  alpha: [number, number];
}

const LOOKS: Record<AmbienceKind, Look> = {
  snow: { count: 70, size: [1.5, 4], speed: [18, 55], drift: [-8, 8], shape: "dot", colors: ["text"], alpha: [0.35, 0.9] },
  rain: { count: 90, size: [9, 18], speed: [520, 760], drift: [-70, -40], shape: "streak", colors: ["muted"], alpha: [0.18, 0.4] },
  storm: { count: 130, size: [11, 22], speed: [640, 900], drift: [-140, -90], shape: "streak", colors: ["muted"], alpha: [0.2, 0.45] },
  fog: { count: 9, size: [160, 320], speed: [0, 0], drift: [6, 16], shape: "haze", colors: ["muted"], alpha: [0.04, 0.09] },
  petals: { count: 34, size: [5, 9], speed: [22, 48], drift: [-22, 22], shape: "leaf", colors: ["#f6b8cf", "#ffd9e6", "second"], alpha: [0.5, 0.9] },
  motes: { count: 42, size: [1, 2.6], speed: [5, 14], drift: [-6, 6], shape: "dot", colors: ["#ffd98a", "accent"], alpha: [0.25, 0.75] },
  leaves: { count: 30, size: [6, 11], speed: [28, 62], drift: [-34, 34], shape: "leaf", colors: ["#d9772b", "#b3541e", "#e2a52e", "accent"], alpha: [0.55, 0.95] },
  confetti: { count: 70, size: [4, 7], speed: [45, 110], drift: [-30, 30], shape: "square", colors: ["accent", "second", "#ffd166", "text"], alpha: [0.6, 1] },
  hearts: { count: 26, size: [6, 11], speed: [18, 40], drift: [-14, 14], shape: "heart", colors: ["#ff5c8a", "#ff8fab", "accent"], alpha: [0.45, 0.9] },
  festive: { count: 80, size: [1.5, 4], speed: [18, 52], drift: [-8, 8], shape: "dot", colors: ["text", "text", "#e63946", "#2a9d8f", "#ffd166"], alpha: [0.4, 0.95] },
  spooky: { count: 34, size: [6, 11], speed: [26, 58], drift: [-30, 30], shape: "leaf", colors: ["#f77f00", "#7b2cbf", "#d9772b"], alpha: [0.5, 0.9] },
  pastel: { count: 36, size: [5, 9], speed: [20, 44], drift: [-20, 20], shape: "leaf", colors: ["#ffd6a5", "#caffbf", "#bde0fe", "#ffc8dd"], alpha: [0.5, 0.9] },
};

const between = ([low, high]: [number, number]) => low + Math.random() * (high - low);

/** The theme's own colors, read from the page at the moment the effect starts. */
function themeColor(name: string): string {
  if (name.startsWith("#")) return name;
  const style = getComputedStyle(document.documentElement);
  const variable = { text: "--text-primary", muted: "--text-muted", accent: "--accent-color", second: "--accent-secondary" }[name];
  return (variable && style.getPropertyValue(variable).trim()) || "#ffffff";
}

function heart(context: CanvasRenderingContext2D, size: number) {
  context.beginPath();
  context.moveTo(0, size * 0.35);
  context.bezierCurveTo(-size, -size * 0.35, -size * 0.45, -size, 0, -size * 0.45);
  context.bezierCurveTo(size * 0.45, -size, size, -size * 0.35, 0, size * 0.35);
  context.fill();
}

interface HomeAmbienceProps {
  kind: AmbienceKind;
  /** Changes when the theme does: its colors are read again. */
  themeKey: string;
}

/**
 * The moving background of the Home page: a few dozen small shapes falling from the top and
 * fading out before the content. Decoration only: it takes no clicks, is hidden from screen
 * readers, stops while the window is not in view and does not exist at all with the
 * animations off (the page that draws it checks that).
 */
export function HomeAmbience({ kind, themeKey }: HomeAmbienceProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const look = LOOKS[kind];
    const colors = look.colors.map(themeColor);
    let width = 0;
    let height = 0;
    const ratio = Math.min(window.devicePixelRatio || 1, 2);

    const fit = () => {
      width = canvas.clientWidth;
      height = canvas.clientHeight;
      canvas.width = Math.max(1, Math.round(width * ratio));
      canvas.height = Math.max(1, Math.round(height * ratio));
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(canvas);

    const born = (anywhere: boolean): Particle => ({
      x: Math.random() * (width + 80) - 40,
      y: anywhere ? Math.random() * height : -20 - Math.random() * 40,
      size: between(look.size),
      speed: between(look.speed),
      drift: between(look.drift),
      phase: Math.random() * Math.PI * 2,
      spin: (Math.random() - 0.5) * 2.4,
      color: colors[Math.floor(Math.random() * colors.length)],
      alpha: between(look.alpha),
    });
    const particles: Particle[] = Array.from({ length: look.count }, () => born(true));

    let frame = 0;
    let last = performance.now();
    let flash = 0;
    const draw = (now: number) => {
      frame = requestAnimationFrame(draw);
      // A window left in the background does not pile up a long step
      const step = Math.min(0.05, (now - last) / 1000);
      last = now;
      if (document.hidden || width === 0) return;
      context.clearRect(0, 0, width, height);

      // A storm lights up now and then
      if (kind === "storm") {
        if (flash <= 0 && Math.random() < step * 0.12) flash = 0.22;
        if (flash > 0) {
          context.globalAlpha = flash * 0.5;
          context.fillStyle = colors[0];
          context.fillRect(0, 0, width, height);
          flash -= step;
        }
      }

      for (let index = 0; index < particles.length; index += 1) {
        const particle = particles[index];
        particle.phase += step * (look.shape === "leaf" || look.shape === "heart" ? 1.6 : 1);
        particle.y += particle.speed * step;
        particle.x += (particle.drift + Math.sin(particle.phase) * (look.shape === "streak" ? 0 : 14)) * step;
        if (particle.y > height + 24 || particle.x < -60 || particle.x > width + 60) {
          particles[index] = born(look.shape === "haze");
          if (look.shape === "haze") particles[index].x = -40;
          continue;
        }
        context.globalAlpha = particle.alpha;
        context.fillStyle = particle.color;
        context.strokeStyle = particle.color;
        switch (look.shape) {
          case "dot":
            context.beginPath();
            context.arc(particle.x, particle.y, particle.size, 0, Math.PI * 2);
            context.fill();
            break;
          case "streak":
            context.lineWidth = 1;
            context.beginPath();
            context.moveTo(particle.x, particle.y);
            context.lineTo(particle.x + (particle.drift / particle.speed) * particle.size, particle.y + particle.size);
            context.stroke();
            break;
          case "haze": {
            const haze = context.createRadialGradient(particle.x, particle.y, 0, particle.x, particle.y, particle.size);
            haze.addColorStop(0, particle.color);
            haze.addColorStop(1, "transparent");
            context.fillStyle = haze;
            context.fillRect(particle.x - particle.size, particle.y - particle.size, particle.size * 2, particle.size * 2);
            break;
          }
          default:
            context.save();
            context.translate(particle.x, particle.y);
            context.rotate(particle.phase * particle.spin);
            if (look.shape === "leaf") {
              // Turning as it falls, a leaf is seen from the side and from the front
              context.scale(1, 0.35 + Math.abs(Math.cos(particle.phase)) * 0.65);
              context.beginPath();
              context.ellipse(0, 0, particle.size, particle.size * 0.5, 0, 0, Math.PI * 2);
              context.fill();
            } else if (look.shape === "heart") {
              heart(context, particle.size);
            } else {
              context.scale(1, 0.3 + Math.abs(Math.cos(particle.phase * 2)) * 0.7);
              context.fillRect(-particle.size / 2, -particle.size / 2, particle.size, particle.size * 0.6);
            }
            context.restore();
        }
      }
      context.globalAlpha = 1;
    };
    frame = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [kind, themeKey]);

  return <canvas ref={canvasRef} className="home-ambience" data-ambience={kind} aria-hidden="true" />;
}
