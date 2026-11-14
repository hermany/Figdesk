# Smart sections (Figdesk)

A smart section is a section with a **mode** and a **Play** button beside its name. Play hands the
section to the AI agent together with the mode's prompt document, and the agent builds what the document
asks for from the section's content, in the project's work folder. A Web section builds a website, a
Document section writes a document.

## Using one

- **Make one**: Actions (the palette) ▸ *New smart section*, or the AI tab's *New smart section* button.
  With sections selected it makes them smart; otherwise it adds a new one in the middle of the view.
  Commands: `smartSection.create`, `smartSection.setMode`, `smartSection.clear`.
- **Pick the mode**: the chip next to the name (*Web ▾*) lists the modes.
- **Run it**: Play. The AI tab opens and shows the run. It needs the **work folder** (AI tab ▸ Setup);
  without one it says so and does nothing.

The mark is the section's plugin data `figdesk.smart` (`{"mode": "web"}`): it is saved with the file and
copied with the section, and the file format has nothing new in it.

## Prompt documents

Each project keeps its own in `<work folder>/prompts/<mode>.md`. The first time a folder is used, the app
copies the documents that ship with it (`web`, `document`, `general`) there, and never overwrites one that
exists, so edit them freely. A file of your own in that folder is a mode of its own and shows in the list.
A mode's name is its file's name: letters, digits, `-` and `_`, up to 32 (anything else is refused, so a
document from someone else can't point it at another folder).

A document is Markdown with optional front matter between `---` lines. `title` is the name the mode list
shows; the other lines (`output`, `stack`, `responsive`, `language`…) are the parameters the agent reads
with the rest, and are also described in the document's *Parameters* section:

```markdown
---
title: Landing page
output: web
stack: HTML5 and CSS3, no build step
---
# Mode: Landing page
…what to read, what to build, where to write it, what to report…
```

What the agent receives is a short header (the section's id and name, the mode, the work folder), the
whole document, and the instruction to inspect the section and do what the document says. The section's
name is quoted and marked as data, not instruction.

## Limits

The agent works with the same rules as the AI tab's chat: the work folder is the only place it may read or
write files, through the MCP server's path check and the permission rules of each CLI (`docs/mcp.md`).
Play is not available while the agent is answering another prompt.

## The project's base documents (`.agent/`)

The first time a folder is used the app also copies `.agent/prompt.md` (what the project is),
`.agent/design.md` (stack, styling, structure) and `.agent/agent.md` (how the AI works) into it, and never
over a file that exists. Each starts with `status: default`. While any of the three says that, a run
makes the AI ask the person for the base prompt and the agent and design guidelines before it builds
anything, offering the standard: React Native with Expo and TypeScript, styled with Tailwind through
NativeWind. The AI writes the answer into the files and sets `status: confirmed`; later runs build without
asking.

Frames of the same size and structure inside a smart section are one screen in different states: the AI
builds one screen and changes the elements that differ, not separate pages.
