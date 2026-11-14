---
status: default
---
# Agent guidelines

How the AI works in this project. Change any line; the AI follows it as written.

## Before anything is built

1. Read `.agent/prompt.md`, `.agent/design.md` and this file, then the mode document of the run.
2. If any of the three still says `status: default` at the top, nobody has confirmed them yet. Do NOT
   build anything yet. Ask the person, in one short message, for:
   - the base prompt: what the project is, who it is for, what it must do;
   - the agent guidelines: how you should work (language, how much to ask, what never to touch);
   - the design guidelines: stack, styling, structure.
   Offer the standard in `design.md` (React Native, Expo, TypeScript, Tailwind through NativeWind) as the
   answer to give when they do not know or do not care, and wait for the answer.
3. When they answer, write it into the three files (keep their headings), change `status: default` to
   `status: confirmed` in each, and only then build. If they say to use the standard, keep the standard
   and confirm the files the same way.
4. When all three say `status: confirmed`, do not ask again: build.

## While building

- Work only inside this work folder. Never touch files outside it.
- Frames that are the same size and share their structure are ONE screen in different states: build one
  screen and change only the elements that differ. Never make two pages from them.
- Name files and components after what they are for. Keep each file small.
- Do not install anything or run commands that reach the network unless the person asked.

## When you finish

Say in a few lines what you made and list the files with their paths. Do not paste the code into the chat.
If something in the design could not be built as drawn, say what and why.
