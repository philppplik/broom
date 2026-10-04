# Third-party notices

Broom is MIT-licensed. It **embeds data** from one project and **follows the approach** of several others.
This file lists exactly what comes from where.

## Embedded (redistributed unmodified)

### winutil - Chris Titus Tech / CT Tech Group LLC - MIT

The files in [`third_party/winutil/`](third_party/winutil/) (`tweaks.json`, `feature.json`, `appx.json`, `dns.json`, `preset.json`)
are copied unmodified from https://github.com/ChrisTitusTech/winutil (commit noted in `third_party/winutil/SOURCE.txt`).
Broom compiles them into the binary and interprets them with its own code (`src/tweaks/winutil.rs`, `debloat.rs`, `dns.rs`).

```
MIT License

Copyright (c) 2022 CT Tech Group LLC

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Approach followed (own implementation, no code copied)

| Project | Author | License | What Broom took as inspiration |
|---|---|---|---|
| [Bulk Crap Uninstaller](https://github.com/BCUninstaller/Bulk-Crap-Uninstaller) | Marcin Szeniak | Apache-2.0 | Quiet-uninstall detection per installer technology (MSI, Inno Setup, NSIS, Squirrel); per-program leftover scan with confidence levels |
| [Prune](https://github.com/jimman0I/prune) | jimman0I | MIT | Quarantine leftovers instead of deleting them; show measured vs. claimed sizes honestly |

## Ideas only (copyleft projects - deliberately no code, data or text copied)

Broom stays MIT, so nothing was taken from these GPL / CC-BY-SA projects. Their feature sets inspired
independently written features; Windows registry locations used by Broom are documented Microsoft settings.

| Project | Author | License | Inspired |
|---|---|---|---|
| [Optimizer](https://github.com/hellzerg/optimizer) | hellzerg | GPL-3.0 | Service, privacy and performance tweak categories |
| [Sparkle](https://github.com/thedogecraft/sparkle) | thedogecraft | GPL-3.0 | "Find fastest DNS", GPU-aware tweak hints, repair utilities |
| [ReviOS Playbook](https://github.com/meetrevision/playbook) | Revision | CC-BY-SA-4.0 | Privacy and debloat focus |
| [Slate Desktop](https://github.com/QuiteAFancyEmerald/Slate-Desktop-for-Windows-11) | QuiteAFancyEmerald | GPL-3.0 | Performance and shell tweak ideas |
| [MangoDisk](https://github.com/harry0703/MangoDisk) | harry0703 | GPL-3.0 | Project build artifacts, AI caches, startup manager, system maintenance |

## Rust crates

Broom is built with ratatui, crossterm, clap, serde, serde_json, anyhow, chrono, rayon, winreg, windows-sys and sysinfo
(all MIT and/or Apache-2.0). Their licenses ship with their source on crates.io.
