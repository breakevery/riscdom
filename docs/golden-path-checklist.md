[中文](golden-path-checklist.zh-CN.md) | English

# Golden path — the manual walk (steps 1 and 2)

> **Why by hand.** Steps 1–2 are machine setup: install QEMU, resolve a RISC-V
> compiler, configure a provider, run the preflight. CI has no QEMU, no GUI and no
> real API key, so it cannot honestly walk them (see [golden-path.md](golden-path.md) §6).
> A person does it on a clean machine once per release and **records it here** —
> decision 6 in [golden-path.md](golden-path.md) §8: *a walk nobody wrote down is a
> walk nobody can check.*
>
> Steps 3–7 are covered instead by
> `cargo test -p host-core --test golden_path -- --ignored` (mock LLM + a real QEMU guest;
> see [golden-path.md](golden-path.md) §6a). This checklist covers what that test cannot.

## How to use it

1. Copy the template below into `docs/walks/<date>-<version>.md` (create `docs/walks/`
   if it does not exist), or paste it into the release notes. *If the record goes into the
   repository, it must satisfy `scripts/check-bilingual.sh`: every `*.md` there needs a
   counterpart — so write both languages, or keep the record in the release notes.*
2. Walk steps 1 and 2 on a machine that has never run this project.
3. Fill **every** field. `unknown` is an answer; an empty field is not.
4. When something fails, do not write "it failed": name the step, paste the raw
   error, and say whether it was recoverable and how.
5. Keep the record with the release.

## Template (copy this)

~~~markdown
### Golden-path walk — <version> — <date>

**Machine and date**
- Machine: <hostname / model>, <CPU / RAM>
- Date: <YYYY-MM-DD> — operator: <name>

**Operating system**
- <edition> — build <build>

**QEMU (step 1)**
- `qemu-system-riscv64 --version` → <version>
- Source: <winget / official installer / manual download>
- Path in Settings → Toolchain: <path, or "auto-discovered">

**RISC-V GCC (step 1)**
- `<gcc> --version` → <version>
- Source: <auto-discovered / manual pick / in-app xPack download>
- Path in Settings → Toolchain: <path>

**Provider and model (step 2)**
- Provider: <deepseek / …> — model: <…>
- Base URL: <…>
- Key: <stored in the OS keyring / not stored>

**Preflight (step 2 — Settings → Toolchain → Environment preflight)**
- Step 1 <name>: <pass / fail> — <detail>
- Step 2 <name>: <pass / fail> — <detail>
- Step 3 <name>: <pass / fail> — <detail>
- Step 4 <name>: <pass / fail> — <detail>
- Verdict: <green / overridden: reason>

**Two runs (proof the walk reached the path)**
- Run A, short fingerprint: <16 hex characters>
- Run B, short fingerprint: <16 hex characters>
- The two differ: <yes — the field that changed / no>

**Exported audit record**
- File: <path>
- `audit-verify <path-to-db> --runs` → <Intact { length: N } + RunIndex { findings: 0 } / Broken { … }>

**Failures**
- Step: <1 / 2 / none>
- Raw error: <paste it, verbatim>
- Recoverable: <yes — what fixed it / no — blocker>
~~~

## Worked example

```markdown
### Golden-path walk — v0.5.0 — 2026-09-19

**Machine and date**
- Machine: Z0624145651262 (x64), 16 GB RAM
- Date: 2026-09-19 — operator: project owner

**Operating system**
- Windows 11 Pro — build 22631

**QEMU (step 1)**
- `qemu-system-riscv64 --version` → QEMU emulator version 10.1.0
- Source: winget
- Path in Settings → Toolchain: C:\Program Files\qemu\qemu-system-riscv64.exe

**RISC-V GCC (step 1)**
- `riscv-none-elf-gcc --version` → xPack 15.2.0-1
- Source: in-app xPack download
- Path in Settings → Toolchain: %APPDATA%\…\toolchains\xpack-riscv-none-elf-gcc-15.2.0-1\bin\riscv-none-elf-gcc.exe

**Provider and model (step 2)**
- Provider: deepseek — model: deepseek-chat
- Base URL: https://api.deepseek.com
- Key: stored in the OS keyring

**Preflight (step 2 — Settings → Toolchain → Environment preflight)**
- Step 1 toolchain_runs: pass — `--version` answered
- Step 2 compile: pass — the four-line guest built
- Step 3 qemu_runs: pass — QEMU answered `--version`
- Step 4 boot: pass — the guest printed its banner
- Verdict: green

**Two runs (proof the walk reached the path)**
- Run A, short fingerprint: 933566304b8632a7
- Run B, short fingerprint: 4b0f0d2c81aa7e93
- The two differ: yes — `llm.model` (deepseek-chat → deepseek-reasoner)

**Exported audit record**
- File: <workspace>\exports\run_01a0b812….jsonl
- `audit-verify <path-to-db> --runs` → Intact { length: 41 } + RunIndex { findings: 0 }

**Failures**
- Step: 1
- Raw error: error: could not find `qemu-system-riscv64` on PATH (searched: PATH, RISCDOM_QEMU, C:\Program Files\qemu)
- Recoverable: yes — installed QEMU with `winget install SoftwareFreedomConservancy.QEMU`, reopened the app, auto-discovery found it
```

## Notes

- The fingerprints above are the two runs of the same request with one configuration
  field changed (step 7). Recording them is what makes "the environment was captured"
  checkable rather than asserted.
- **A preview's version number looks different in Windows.** If you installed the preview MSI,
  *Settings → Apps → Installed apps* shows **`0.5.0.1`**, not `0.5.0-preview.1`: WiX cannot take a
  pre-release version, so the preview pins the MSI's own installer version separately while the
  package version — and every artifact name — stays `0.5.0-preview.1`. That is expected, not a bug
  to report; the final `0.5.0` deletes the override (`CHANGELOG.md`, the preview entry's §Notes).
- An exported run record is **self-contained**: put the file into an empty database and
  `audit-verify` judges it with nothing carried over from the machine that produced it
  (`--runs` additionally needs the derived index rebuilt there, which is `audit-rebuild`'s
  job). A file that does not verify that way is a finding, not a formatting detail.
- `Broken` or `findings > 0` is a **stop** for the release: the record no longer
  explains itself.
