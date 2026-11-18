# Security

## Reporting a problem

Please report security problems privately through GitHub: **Security → Report a vulnerability**
on https://github.com/Moxier/hanabi-studio. Don't open a public issue for them.

## What to know

**The AI connection (MCP).** While Hanabi Studio runs, it listens on
`http://127.0.0.1:7860/mcp` so an AI assistant on the same computer can drive it. The tools can
open files, save projects, motions and renders to paths you give them, and start renders.

- The server only listens on this computer (127.0.0.1), never on the network.
- Browsers are blocked: requests from web pages are refused unless the page itself is local
  (`localhost`, `127.0.0.1`, `[::1]`). Look-alike names, sandboxed frames, local HTML files
  (`Origin: null`) and DNS-rebinding tricks (a foreign `Host`) all get `403`.
- Programs on your computer can use it, just as they could use any other app. To turn it off,
  start the app with `--mcp-port 0`. With `--mcp-stdio` an AI client starts its own copy instead.
- Requests are limited in size, and deleting things needs `confirm=true`.

**Files.** Models, motions and textures are parsed by the app's own code, which repairs broken
numbers and refuses malformed files instead of crashing. Model files are never modified; physics
tweaks go to separate preset files in the settings folder.

**Other programs.** Video export runs `ffmpeg` (installed separately by you) with its arguments
passed directly, never through a shell. Opening files and links uses the system's normal opener
(`xdg-open`, `explorer.exe`), not a command shell.

**Builds.** Release builds are made by GitHub Actions with read-only access, pinned action
versions and a dependency audit against the RustSec advisory database.
