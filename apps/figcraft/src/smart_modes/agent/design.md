---
status: default
---
# Design guidelines

How what the AI builds must look and be organised. Change any line; the AI follows it as written.

## Standard (used when nothing else is chosen)

- **stack**: React (plain React for the web, not React Native or Expo) with TypeScript, built with Vite.
- **styling**: Tailwind CSS (`className` on components). No other styling library and no loose inline
  style objects unless Tailwind cannot say it.
- **tokens**: the design's colours, fonts and spacing go in the Tailwind theme (`theme.extend` in
  `tailwind.config.js`), named after what they are for (`brand`, `surface`, `text-muted`), and components
  use those names.
- **structure**: `src/pages/` or `src/screens/` for screens, `src/components/` for parts that repeat,
  `public/` or `src/assets/` for exported pictures. One component per file.
- **dependencies**: `package.json` lists every package the stack needs (`react`, `react-dom`, `vite`,
  `@vitejs/plugin-react`, `typescript`, `tailwindcss`, `postcss`, `autoprefixer`, the `@types/*`), with
  `index.html`, `vite.config.ts`, `postcss.config.js` and `tailwind.config.js` present. Say in the final
  message the commands to run: `npm install`, `npm run dev`.
- **layout**: flexbox or grid, fluid widths, works from a phone to a wide screen. Touch targets are at
  least 44 px.
- **accessibility**: `accessibilityLabel` on images and icon buttons, readable contrast, text that
  scales.
- **no network**: no CDN and no remote fonts or images; the project's own assets only.

## Project choices

(Not set yet. The AI asks for them before it builds, and writes the answer here.)
