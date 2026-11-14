# FigCraft AI

Panel de chat (estilo pen.dev) que edita tu documento de FigCraft en tiempo real con Claude.
Lo que tengas seleccionado en FigCraft se envía como contexto en cada mensaje.

## Cómo funciona

```
Panel web (127.0.0.1:4848)  ⇄  server.mjs  ⇄  servidor de control de FigCraft (127.0.0.1:47821)
                                   ⇅
                              Claude API (tools: figcraft, figcraft_docs, get_selection)
```

- FigCraft trae un servidor de control local (`--control PORT`): JSON por línea sobre TCP,
  `{"id","method":"engine.execute","params":{"command","params"}}`.
- `server.mjs` lee la selección cada 500 ms (`select.add {ids:[]}` + `node.get`) y la empuja al panel.
- Claude ejecuta comandos (`node.create`, `node.set`, `text.set`, `layout.add`…) y el canvas cambia al instante.
- `figcraft_docs` busca en la referencia de comandos que viene dentro del binario de FigCraft.

¿Por qué no un plugin dentro de FigCraft? Los plugins de FigCraft corren en un sandbox WASM
sin red, así que no pueden hablar con una IA. El servidor de control es la vía oficial para agentes.

## Uso

```sh
export ANTHROPIC_API_KEY=sk-ant-...
./start.sh                 # abre FigCraft con --control 47821 y el panel
./start.sh archivo.figcraft
```

Si FigCraft ya está abierto sin `--control`, ciérralo (guarda antes) y usa `./start.sh`.

Variables: `FIGCRAFT_CONTROL_PORT` (47821), `PORT_UI` (4848), `FIGCRAFT_AI_MODEL` (claude-opus-5-5).
