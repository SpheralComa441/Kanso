import { useEffect, useRef } from "react";
import type { AmbienceKind } from "../lib/ambience";

/**
 * A small picture drawn pixel by pixel: one string per row, "." for nothing and a digit for
 * a color of the palette it is painted with (1 the body, 2 and 3 its details).
 */
type Pixels = string[];

interface Sprite {
  /** The pictures it goes through as it falls (a leaf turning, a bat's wings); one for a still. */
  frames: Pixels[];
  /**
   * The colors it can come in: each entry gives colors 1, 2 and 3. A name ("text", "muted",
   * "accent", "second") is a color of the theme; anything else is the color itself.
   */
  palettes: string[][];
  /** How often it is picked among the sprites of its kind (1 when not said). */
  weight?: number;
}

/** How a kind looks and moves. Speeds are in screen pixels per second. */
interface Look {
  /** Sprites per 100 000 square pixels of page in view, and the most there can be. */
  density: number;
  most: number;
  /** The side of one drawn pixel, in screen pixels. */
  pixel: number;
  speed: [number, number];
  drift: [number, number];
  /** How far it sways sideways as it falls. */
  sway: number;
  /** Seconds each picture of a sprite is shown; 0 for stills. */
  frameSeconds: number;
  alpha: [number, number];
  sprites: Sprite[];
  /** Crosses the page sideways instead of falling (fog). */
  sideways?: boolean;
  /** Lights the page up now and then (a storm). */
  flashes?: boolean;
}

const mirror = (pixels: Pixels): Pixels => pixels.map((row) => [...row].reverse().join(""));

/* ---------------------------------------------------------------- sprites */

const LEAF_MAPLE: Pixels = [
  "....1....",
  "...111...",
  ".1.111.1.",
  ".1111111.",
  "111121111",
  ".1112111.",
  "..11211..",
  "....2....",
  "....2....",
];
const LEAF_MAPLE_EDGE: Pixels = [
  "....1....",
  "....1....",
  "...111...",
  "...121...",
  "...121...",
  "...111...",
  "....2....",
  "....2....",
];
const LEAF_OVAL: Pixels = [
  ".....1",
  "...111",
  "..1121",
  ".11211",
  ".1211.",
  "1211..",
  "111...",
  "2.....",
];
const LEAF_OVAL_EDGE: Pixels = [
  "....1",
  "...1.",
  "..12.",
  ".12..",
  "12...",
  "2....",
];
const LEAF_SMALL: Pixels = [
  "..11",
  ".112",
  "1121",
  "121.",
  "2...",
];
/** A leaf or a petal turning as it falls: seen from the front for longer than from the side. */
const tumbling = (face: Pixels, edge: Pixels): Pixels[] => [face, face, edge, mirror(face), mirror(face), mirror(edge)];

const FLAKE_BIG: Pixels = [
  "..1..",
  "1.1.1",
  ".111.",
  "1.1.1",
  "..1..",
];
const FLAKE: Pixels = [
  ".1.",
  "111",
  ".1.",
];
const DOT: Pixels = ["1"];
const DOT_BIG: Pixels = [
  "11",
  "11",
];

const PETAL: Pixels = [
  ".11..",
  "1112.",
  "11122",
  ".122.",
];
const PETAL_EDGE: Pixels = [
  ".1...",
  ".12..",
  "..12.",
  "...2.",
];

const SPARK: Pixels = [
  ".1.",
  "121",
  ".1.",
];
const SPARK_DIM: Pixels = [
  "...",
  ".1.",
  "...",
];

const DROP: Pixels = [
  ".1",
  ".1",
  ".1",
  "1.",
  "1.",
];
const DROP_LONG: Pixels = [
  "..1",
  "..1",
  ".1.",
  ".1.",
  ".1.",
  "1..",
  "1..",
];

const CLOUD: Pixels = [
  "....111111....",
  "..1111111111..",
  "11111111111111",
  ".111111111111.",
  "...1.1.1.1....",
];
const CLOUD_SMALL: Pixels = [
  "..11111..",
  "111111111",
  ".1111111.",
  "..1.1.1..",
];

const PAPER_FLAT: Pixels = ["111"];
const PAPER_TALL: Pixels = [
  "1",
  "1",
  "1",
];
const PAPER_SQUARE: Pixels = [
  "11",
  "11",
];
const PAPER_THIN: Pixels = ["11"];

const HEART: Pixels = [
  ".11.11.",
  "1211111",
  "1111111",
  ".11111.",
  "..111..",
  "...1...",
];
const HEART_SMALL: Pixels = [
  "1.1",
  "111",
  ".1.",
];

const STAR: Pixels = [
  "..1..",
  ".111.",
  "11111",
  ".111.",
  ".1.1.",
];
const BAUBLE: Pixels = [
  "..2..",
  ".111.",
  "13111",
  "11111",
  ".111.",
];

const BAT_UP: Pixels = [
  "1.......1",
  "11.1.1.11",
  "111111111",
  "..11111..",
];
const BAT_DOWN: Pixels = [
  "...1.1...",
  ".1111111.",
  "111111111",
  "1.11111.1",
];
const PUMPKIN: Pixels = [
  "...2...",
  ".11111.",
  "1131311",
  "1111111",
  "1133311",
  ".11111.",
];

const EGG: Pixels = [
  ".111.",
  "11111",
  "12221",
  "11111",
  "13331",
  "11111",
  ".111.",
];

/* ------------------------------------------------------------------ looks */

const AUTUMN = [
  ["#d9772b", "#8a4513"],
  ["#b3541e", "#6e2f0d"],
  ["#e2a52e", "#9c6a12"],
  ["#c0392b", "#7a1f16"],
  ["accent", "#6e2f0d"],
];
const SNOW = [["text"], ["text"], ["muted"]];
const PINK = [
  ["#f6b8cf", "#e58fb0"],
  ["#ffd9e6", "#f3aac4"],
  ["second", "#e58fb0"],
];

const LOOKS: Record<AmbienceKind, Look> = {
  snow: {
    density: 6,
    most: 150,
    pixel: 3,
    speed: [16, 52],
    drift: [-8, 8],
    sway: 14,
    frameSeconds: 0,
    alpha: [0.4, 0.95],
    sprites: [
      { frames: [FLAKE_BIG], palettes: SNOW },
      { frames: [FLAKE], palettes: SNOW, weight: 2 },
      { frames: [DOT_BIG], palettes: SNOW, weight: 2 },
      { frames: [DOT], palettes: SNOW, weight: 3 },
    ],
  },
  rain: {
    density: 8,
    most: 200,
    pixel: 3,
    speed: [480, 700],
    drift: [-150, -110],
    sway: 0,
    frameSeconds: 0,
    alpha: [0.25, 0.5],
    sprites: [{ frames: [DROP], palettes: [["muted"], ["text"]] }],
  },
  storm: {
    density: 11,
    most: 260,
    pixel: 3,
    speed: [620, 880],
    drift: [-290, -230],
    sway: 0,
    frameSeconds: 0,
    alpha: [0.25, 0.55],
    flashes: true,
    sprites: [{ frames: [DROP_LONG], palettes: [["muted"], ["text"]] }],
  },
  fog: {
    density: 0.7,
    most: 18,
    pixel: 12,
    speed: [0, 0],
    drift: [8, 20],
    sway: 0,
    frameSeconds: 0,
    alpha: [0.05, 0.1],
    sideways: true,
    sprites: [
      { frames: [CLOUD], palettes: [["muted"], ["text"]] },
      { frames: [CLOUD_SMALL], palettes: [["muted"]] },
    ],
  },
  petals: {
    density: 3,
    most: 80,
    pixel: 3,
    speed: [22, 50],
    drift: [-22, 22],
    sway: 22,
    frameSeconds: 0.45,
    alpha: [0.55, 0.95],
    sprites: [{ frames: tumbling(PETAL, PETAL_EDGE), palettes: PINK }],
  },
  motes: {
    density: 3.3,
    most: 90,
    pixel: 3,
    speed: [5, 16],
    drift: [-8, 8],
    sway: 10,
    frameSeconds: 0.7,
    alpha: [0.35, 0.85],
    sprites: [
      { frames: [SPARK, SPARK_DIM], palettes: [["#ffd98a", "#fff3c4"], ["accent", "#fff3c4"]] },
      { frames: [DOT, DOT, DOT_BIG], palettes: [["#ffd98a"], ["accent"]], weight: 2 },
    ],
  },
  leaves: {
    density: 3,
    most: 80,
    pixel: 3,
    speed: [26, 62],
    drift: [-34, 34],
    sway: 26,
    frameSeconds: 0.3,
    alpha: [0.6, 1],
    sprites: [
      { frames: tumbling(LEAF_MAPLE, LEAF_MAPLE_EDGE), palettes: AUTUMN, weight: 2 },
      { frames: tumbling(LEAF_OVAL, LEAF_OVAL_EDGE), palettes: AUTUMN, weight: 2 },
      { frames: [LEAF_SMALL, mirror(LEAF_SMALL)], palettes: AUTUMN },
    ],
  },
  confetti: {
    density: 5,
    most: 140,
    pixel: 4,
    speed: [45, 110],
    drift: [-30, 30],
    sway: 18,
    frameSeconds: 0.22,
    alpha: [0.65, 1],
    sprites: [
      { frames: [PAPER_FLAT, PAPER_SQUARE, PAPER_TALL, PAPER_SQUARE], palettes: [["accent"], ["second"], ["#ffd166"], ["text"]] },
      { frames: [PAPER_THIN, PAPER_TALL], palettes: [["accent"], ["second"], ["#ffd166"]] },
    ],
  },
  hearts: {
    density: 2,
    most: 60,
    pixel: 3,
    speed: [18, 40],
    drift: [-14, 14],
    sway: 16,
    frameSeconds: 0,
    alpha: [0.5, 0.95],
    sprites: [
      { frames: [HEART], palettes: [["#ff5c8a", "#ffc2d4"], ["#ff8fab", "#ffe0ea"], ["accent", "#ffe0ea"]] },
      { frames: [HEART_SMALL], palettes: [["#ff5c8a"], ["#ff8fab"]] },
    ],
  },
  festive: {
    density: 6.3,
    most: 160,
    pixel: 3,
    speed: [18, 52],
    drift: [-8, 8],
    sway: 14,
    frameSeconds: 0,
    alpha: [0.45, 1],
    sprites: [
      { frames: [FLAKE_BIG], palettes: SNOW, weight: 2 },
      { frames: [FLAKE], palettes: SNOW, weight: 4 },
      { frames: [DOT], palettes: SNOW, weight: 4 },
      { frames: [STAR], palettes: [["#ffd166"]] },
      { frames: [BAUBLE], palettes: [["#e63946", "#ffd166", "#ffb3b8"], ["#2a9d8f", "#ffd166", "#a8e6dc"]] },
    ],
  },
  spooky: {
    density: 2.5,
    most: 70,
    pixel: 3,
    speed: [26, 58],
    drift: [-30, 30],
    sway: 24,
    frameSeconds: 0.3,
    alpha: [0.55, 0.95],
    sprites: [
      { frames: [BAT_UP, BAT_DOWN], palettes: [["#b388ff"], ["#9d4edd"]], weight: 2 },
      { frames: [PUMPKIN], palettes: [["#f77f00", "#2d6a4f", "#3a1d00"]] },
      { frames: tumbling(LEAF_OVAL, LEAF_OVAL_EDGE), palettes: [["#d9772b", "#6e2f0d"], ["#9d4edd", "#5a189a"]], weight: 2 },
    ],
  },
  pastel: {
    density: 2.7,
    most: 70,
    pixel: 3,
    speed: [20, 44],
    drift: [-20, 20],
    sway: 18,
    frameSeconds: 0.45,
    alpha: [0.55, 0.95],
    sprites: [
      {
        frames: [EGG],
        palettes: [
          ["#ffd6a5", "#ffadad", "#fdffb6"],
          ["#caffbf", "#9bf6ff", "#ffc6ff"],
          ["#bde0fe", "#ffc8dd", "#fdffb6"],
        ],
      },
      { frames: tumbling(PETAL, PETAL_EDGE), palettes: [["#ffc8dd", "#ffafcc"], ["#caffbf", "#a3e6a0"], ["#bde0fe", "#a2d2ff"]], weight: 2 },
    ],
  },
};

const between = ([low, high]: [number, number]) => low + Math.random() * (high - low);

/** The theme's own colors, read from the page at the moment the effect starts. */
function themeColor(name: string): string {
  if (name.startsWith("#")) return name;
  const style = getComputedStyle(document.documentElement);
  const variable = { text: "--text-primary", muted: "--text-muted", accent: "--accent-color", second: "--accent-secondary" }[name];
  return (variable && style.getPropertyValue(variable).trim()) || "#ffffff";
}

/** A sprite painted once in one of its colors, ready to be stamped on the page. */
interface Stamp {
  frames: HTMLCanvasElement[];
  /** The widest and tallest of its pictures, in device pixels: its place on the page. */
  width: number;
  height: number;
}

/** Paints every sprite of a look, in each of its colors, at `size` device pixels per pixel. */
function paintStamps(look: Look, size: number): Stamp[] {
  const stamps: Stamp[] = [];
  for (const sprite of look.sprites) {
    for (const palette of sprite.palettes) {
      const colors = palette.map(themeColor);
      const frames = sprite.frames.map((pixels) => {
        const canvas = document.createElement("canvas");
        canvas.width = Math.max(...pixels.map((row) => row.length)) * size;
        canvas.height = pixels.length * size;
        const context = canvas.getContext("2d");
        if (context) {
          pixels.forEach((row, y) => {
            [...row].forEach((cell, x) => {
              if (cell === ".") return;
              // A color the palette does not name is painted as the body
              context.fillStyle = colors[Number(cell) - 1] ?? colors[0];
              context.fillRect(x * size, y * size, size, size);
            });
          });
        }
        return canvas;
      });
      const stamp = { frames, width: Math.max(...frames.map((frame) => frame.width)), height: Math.max(...frames.map((frame) => frame.height)) };
      // A sprite that should come up more often is simply listed more than once
      for (let copy = 0; copy < (sprite.weight ?? 1); copy += 1) stamps.push(stamp);
    }
  }
  return stamps;
}

interface Particle {
  stamp: Stamp;
  x: number;
  y: number;
  speed: number;
  drift: number;
  /** Where it is in its sway and in its turning. */
  phase: number;
  turn: number;
  alpha: number;
}

interface HomeAmbienceProps {
  kind: AmbienceKind;
  /** Changes when the theme does: its colors are read again. */
  themeKey: string;
}

/**
 * The moving background of the Home page, in pixel art: small sprites of the season, of a
 * holiday or of the weather (leaves turning as they fall, snowflakes, rain, bats…) coming down
 * the whole page, behind its content. It stays over the part of the page in view however far
 * the page is scrolled, so the fall reaches the bottom of the window.
 *
 * Decoration only: it takes no clicks, is hidden from screen readers, stops while the window
 * is not in view and does not exist at all with the animations off (the page that draws it
 * checks that).
 */
export function HomeAmbience({ kind, themeKey }: HomeAmbienceProps) {
  const holderRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const holder = holderRef.current;
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    // What scrolls the page: the background covers what it shows of it
    const scroller = holder?.closest<HTMLElement>(".app-main") ?? holder?.parentElement;
    const page = holder?.parentElement;
    if (!holder || !canvas || !context || !scroller || !page) return;
    const look = LOOKS[kind];
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    // Whole device pixels per drawn pixel: a fraction would blur the edges
    const size = Math.max(1, Math.round(look.pixel * ratio));
    const stamps = paintStamps(look, size);
    const flashColor = themeColor("text");
    let width = 0;
    let height = 0;
    let particles: Particle[] = [];

    const born = (anywhere: boolean): Particle => {
      const stamp = stamps[Math.floor(Math.random() * stamps.length)];
      const particle: Particle = {
        stamp,
        x: Math.random() * (width + stamp.width * 2) - stamp.width,
        y: anywhere ? Math.random() * height : -stamp.height - Math.random() * 60 * ratio,
        speed: between(look.speed) * ratio,
        drift: between(look.drift) * ratio,
        phase: Math.random() * Math.PI * 2,
        turn: Math.random() * 10,
        alpha: between(look.alpha),
      };
      if (look.sideways) {
        particle.y = Math.random() * (height - stamp.height);
        if (!anywhere) particle.x = -stamp.width;
      } else if (look.drift[1] < 0 && particle.speed > 0) {
        // Rain blown to the left: born as far upwind as it will travel sideways on its way
        // down, or the bottom right of the page would never see a drop
        particle.x = Math.random() * (width + stamp.width + (height * -particle.drift) / particle.speed);
      }
      return particle;
    };

    const fit = () => {
      const scrollerBox = scroller.getBoundingClientRect();
      const pageBox = page.getBoundingClientRect();
      // The holder sits at the top of the page and "sticky" keeps it at the top of the window
      // once the page is scrolled. The canvas hangs from it, starting as far above it as the
      // page starts below the top of the window, and is that much taller than the window: so
      // it covers everything in view both before and after the page is scrolled
      const above = Math.max(0, pageBox.top - scrollerBox.top + scroller.scrollTop);
      // ...but no taller than the page gives it room for: on a Home shorter than the window
      // the extra height would make the page scroll by itself
      const pageEnd = pageBox.bottom - scrollerBox.top + scroller.scrollTop + (parseFloat(getComputedStyle(scroller).paddingBottom) || 0);
      const tall = scroller.clientHeight + Math.min(above, Math.max(0, pageEnd - scroller.clientHeight));
      canvas.style.top = `${-above}px`;
      canvas.style.left = `${scrollerBox.left - pageBox.left}px`;
      canvas.style.width = `${scroller.clientWidth}px`;
      canvas.style.height = `${tall}px`;
      const nextWidth = Math.max(1, Math.round(scroller.clientWidth * ratio));
      const nextHeight = Math.max(1, Math.round(tall * ratio));
      if (nextWidth === width && nextHeight === height) return;
      width = nextWidth;
      height = nextHeight;
      canvas.width = width;
      canvas.height = height;
      context.imageSmoothingEnabled = false;
      // As many as the page in view has room for
      const area = (width / ratio) * (height / ratio);
      const wanted = Math.min(look.most, Math.max(6, Math.round((area / 100_000) * look.density)));
      particles = Array.from({ length: wanted }, (_, index) => particles[index] ?? born(true));
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(scroller);
    observer.observe(page);
    // The page comes in with a short animation that moves it: where things are is measured
    // again once it has settled (a move alone does not tell the observer)
    scroller.addEventListener("animationend", fit);
    scroller.addEventListener("animationcancel", fit);

    let frame = 0;
    let last = performance.now();
    let flash = 0;
    const draw = (now: number) => {
      frame = requestAnimationFrame(draw);
      // A window left in the background does not pile up a long step
      // ...and the first frame can be stamped a hair before the effect started: never backwards
      const step = Math.max(0, Math.min(0.05, (now - last) / 1000));
      last = now;
      if (document.hidden || width <= 1) return;
      context.clearRect(0, 0, width, height);

      // A storm lights up now and then
      if (look.flashes) {
        if (flash <= 0 && Math.random() < step * 0.12) flash = 0.22;
        if (flash > 0) {
          context.globalAlpha = flash * 0.5;
          context.fillStyle = flashColor;
          context.fillRect(0, 0, width, height);
          flash -= step;
        }
      }

      for (let index = 0; index < particles.length; index += 1) {
        const particle = particles[index];
        particle.phase += step * 1.6;
        if (look.frameSeconds > 0) particle.turn += step / look.frameSeconds;
        particle.y += particle.speed * step;
        particle.x += (particle.drift + Math.sin(particle.phase) * look.sway * ratio) * step;
        const { stamp } = particle;
        // Past the right edge only what moves right is gone for good: rain is born there
        const goneRight = (particle.drift > 0 || look.sideways === true) && particle.x > width + stamp.width * 2;
        if (particle.y > height || particle.x < -stamp.width * 2 || goneRight) {
          particles[index] = born(false);
          continue;
        }
        const picture = stamp.frames[Math.floor(Math.abs(particle.turn)) % stamp.frames.length];
        if (!picture) continue;
        context.globalAlpha = particle.alpha;
        // On the grid of the drawn pixels: sprites move a pixel at a time, as pixel art does
        context.drawImage(
          picture,
          Math.round((particle.x + (stamp.width - picture.width) / 2) / size) * size,
          Math.round((particle.y + (stamp.height - picture.height) / 2) / size) * size
        );
      }
      context.globalAlpha = 1;
    };
    frame = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      scroller.removeEventListener("animationend", fit);
      scroller.removeEventListener("animationcancel", fit);
    };
  }, [kind, themeKey]);

  return (
    <div ref={holderRef} className="home-ambience-holder" aria-hidden="true">
      <canvas ref={canvasRef} className="home-ambience" data-ambience={kind} aria-hidden="true" />
    </div>
  );
}
