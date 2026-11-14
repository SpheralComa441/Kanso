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
        // The neutral scale is what components use for text and strong borders. Routing it
        // through theme tokens makes every text color themeable (light themes included)
        // without touching the components.
        zinc: {
          50: themeColor("--text-primary"),
          100: themeColor("--text-primary"),
          200: themeColor("--text-secondary"),
          300: themeColor("--text-secondary"),
          400: themeColor("--text-muted"),
          500: themeColor("--text-faint"),
          600: themeColor("--border-strong"),
          700: themeColor("--border-strong"),
          800: themeColor("--bg-elevated"),
          900: themeColor("--bg-surface"),
          950: themeColor("--accent-contrast"),
        },
      },
      borderRadius: {
        card: "var(--card-radius)",
      },
      fontFamily: {
        sans: ["var(--font-sans)"],
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
