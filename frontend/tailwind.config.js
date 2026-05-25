/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        ide: {
          bg: "#1e1e1e",
          sidebar: "#252526",
          panel: "#2d2d2d",
          border: "#3e3e3e",
          active: "#094771",
          hover: "#2a2d2e",
          text: "#cccccc",
          muted: "#858585",
          accent: "#007acc",
        },
      },
    },
  },
  plugins: [],
};
