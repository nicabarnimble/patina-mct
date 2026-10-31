# Engineering skills

Cursor loads every child of this directory that contains `SKILL.md`. Pi discovers project skills under `.pi/skills`. Each name there is a symlink to `../../.cursor/skills/<name>`. pimux starts Pi, and Pi follows that symlink.

## Where the files come from

This directory holds `poteto-mode` and the `principle-*` skills from pstack 0.15.36. Lauren Tan holds the copyright. The license is MIT. The text is `PSTACK-LICENSE`, in this directory beside those skills. `diff -rq` against that plugin's `skills` tree reported the copies identical.

`ub-mode` is user-authored. `PSTACK-LICENSE` covers the pstack skills only.

## Refresh the pstack skills

1. Copy `poteto-mode` and each `principle-*` directory from a newer pstack `skills` tree onto the same names in this directory.
2. Copy that plugin's `LICENSE` onto `PSTACK-LICENSE`.
3. Leave `ub-mode` unchanged.
4. If a destination directory already exists and this refresh should not replace it, keep the existing directory and record the name clash.
5. For a new skill directory, add `.pi/skills/<name>` as a symlink to `../../.cursor/skills/<name>`.
6. Run `find .cursor/skills -name SKILL.md`. The list must include `poteto-mode`, `ub-mode`, and every `principle-*` skill named in the Principles section of `poteto-mode/SKILL.md`.

## Skills poteto-mode asks for that are not here

`poteto-mode` tells the agent to read these pstack skills. This directory does not contain them.

- `architect`
- `arena`
- `benchmark-checklist`
- `figure-it-out`
- `how`
- `interrogate`
- `ketchup`
- `no-comments`
- `reflect`
- `setup-pstack`
- `show-me-your-work`
- `swarm`
- `tdd`
- `technical-writing`
- `unslop`
- `why`

`principle-explain-the-number` links to `../benchmark-checklist/SKILL.md`. That file is not in this directory.

`poteto-mode` also cites `create-skill`, which is a Cursor built-in. It cites `deslop`, `control-cli`, and `control-ui` from the cursor-team-kit plugin. None of those four are pstack skills. This directory does not contain them.
