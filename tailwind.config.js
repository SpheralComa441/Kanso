/**
 * Maps a `src/theme.css` token to a Tailwind color that also honours opacity modifiers
 * (`bg-accent/15`, `border-oled-border/70`, ...). A bare `var(--token)` cannot be given an
 * alpha channel, so Tailwind would silently emit no CSS at all for those classes;
 * `color-mix` lets the tokens stay plain, user-editable hex values.
 */
const themeColor = (token) =>
  `color-mix(in srgb, var(${token}) calc(<alpha-value> * 100%), transparent)`;

/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        oled: {
          bg: themeColor("--bg-primary"),
          surface: themeColor("--bg-surface"),
          elevated: themeColor("--bg-elevated"),
          border: themeColor("--border-subtle"),
        },
        accent: {
          DEFAULT: themeColor("--accent-color"),
          hover: themeColor("--accent-hover"),
        },
      },
      borderRadius: {
        card: "var(--card-radius)",
      },
      fontFamily: {
        sans: [
          "Inter",
          "Segoe UI Variable",
          "Segoe UI",
          "system-ui",
          "-apple-system",
          "sans-serif",
        ],
        mono: [
          "JetBrains Mono",
          "Cascadia Code",
          "Consolas",
          "monospace",
        ],
      },
    },
  },
  plugins: [],
};
