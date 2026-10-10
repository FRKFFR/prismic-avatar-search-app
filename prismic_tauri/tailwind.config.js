/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        vrc: {
          dark: "#0b0f19",
          card: "#111827",
          border: "#1f2937",
          cyan: "#06b6d4",
          teal: "#14b8a6",
          purple: "#8b5cf6",
        }
      }
    },
  },
  plugins: [],
}

