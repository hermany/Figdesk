---
status: default
---
# Design guidelines

How what the AI builds must look and be organised. Change any line; the AI follows it as written.

## Standard (used when nothing else is chosen)

- **stack**: React Native with TypeScript, built with Expo. It also runs on the web through
  `react-native-web`.
- **styling**: Tailwind through NativeWind (`className` on components). No other styling library and no
  loose inline style objects unless Tailwind cannot say it.
- **tokens**: the design's colours, fonts and spacing go in `tailwind.config.js` under `theme.extend`,
  named after what they are for (`brand`, `surface`, `text-muted`), and components use those names.
- **structure**: `app/` or `src/screens/` for screens, `src/components/` for parts that repeat, `assets/`
  for exported pictures. One component per file.
- **layout**: flexbox, fluid widths, works from a phone to a wide screen. Touch targets are at least
  44 pt.
- **accessibility**: `accessibilityLabel` on images and icon buttons, readable contrast, text that
  scales.
- **no network**: no CDN and no remote fonts or images; the project's own assets only.

## Project choices

(Not set yet. The AI asks for them before it builds, and writes the answer here.)
