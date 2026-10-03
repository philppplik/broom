# Security Policy

Broom runs with administrator rights, so security reports are taken seriously.

## Supported versions

Only the latest release receives fixes.

## Reporting a vulnerability

**Please don't open a public issue.** Use GitHub's private reporting instead:
**[Report a vulnerability](https://github.com/philppplik/broom/security/advisories/new)**

Useful details:

- Broom version, Windows build, architecture (x64 / ARM64)
- What was deleted or changed that shouldn't have been, or how the behavior could be abused
- The relevant lines from `C:\ProgramData\Broom\Logs\broom-*.log`

You'll get a reply within 7 days. Fixes for confirmed issues are released as soon as possible and credited unless you prefer otherwise.

## What counts

- Deleting user data or system files outside the documented items
- Following junctions/symlinks during deletion
- Registry changes without a backup
- Anything that lets a non-admin user influence what Broom deletes as admin

"It cleaned something I wanted to keep" is usually a regular bug or feature request, unless the item is documented as doing exactly that.
