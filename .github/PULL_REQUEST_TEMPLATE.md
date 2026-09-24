[中文](PULL_REQUEST_TEMPLATE.zh-CN.md) | English

<!-- Thanks for contributing. See `CONTRIBUTING.md`: the gate is the single list of what
     "green" means, and commits go through the wrapper (`scripts\commit.ps1` / `./scripts/commit.sh`),
     never `git commit` directly. File names are written as code because this text is inserted
     into a pull request body, where a relative link would not resolve as it does in the file.
     Please delete these comments as you fill the sections in. -->

## What changed

<!-- One paragraph. Name the crate(s) or document(s) touched and the behaviour, not the diff. -->

## Related issue

<!-- "Closes #NN" or "Refs #NN"; leave "none" if there is no issue. -->

none

## How it was verified

<!-- The command you ran and its relevant output. If you could not run it, say what you did
     instead. Every pull request must pass the gate in CI. -->

## Checklist

- [ ] The gate is green locally (`scripts\gate.ps1` on Windows, `sh scripts/gate.sh` elsewhere)
- [ ] Documentation changed here is bilingual (`.md` + `.zh-CN.md`, switcher on the first line)
- [ ] No API key, token or credential is added anywhere — not in code, logs, `Debug` output or the frontend
- [ ] The change is scoped; no unrelated files are touched
- [ ] The CLA is signed (see `CLA.md`; a trivial fix is exempt under §9)
