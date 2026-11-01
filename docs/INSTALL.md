# Installing Invar

Invar is a Claude Code plugin with the `invar` CLI inside it. **Installing the
plugin is the only required step.** The plugin carries a binary for every
supported platform, and inside Claude Code `invar` is already on the agent's
`PATH`.

Supported platforms: Linux x86_64 (any distribution, static binary), and macOS
on Apple silicon and Intel. Windows and Linux arm64 are not supported yet.

---

## For a team: set up a project once

One person does this once per repository. Everyone else gets Invar
automatically when they open the project in Claude Code.

**You need:**
- Claude Code
- `git`
- read access to `github.com/webzards-org/invar` (see [Private repository access](#private-repository-access))

**1. Register the marketplace and install the plugin for this project.** Run
in the project's root, in your shell:

```bash
claude plugin marketplace add webzards-org/invar#dist --scope project
claude plugin install invar@invar --scope project
```

`#dist` is the branch the release pipeline publishes, with the binaries
included. Both commands write to `.claude/settings.json` in the project.

**2. Initialize the project.** Start Claude Code in the project and ask:

> Run `invar init`.

Or run it in your shell if you have the CLI ([below](#optional-the-cli-in-your-terminal)).
It creates `invar.yaml`, `.invar/`, empty `AI_CONTEXT.md` and
`BUSINESS_CONTEXT.md`, and merges its entries into `.gitattributes`,
`.gitignore` and `.claude/settings.json` without overwriting anything.

Until `invar init` has run, the plugin's `SessionStart` hook reports an error
at the start of each session ("not inside an invar project"). It is harmless
and goes away after `init`.

**3. Commit:**

```bash
git add .claude/settings.json invar.yaml .invar AI_CONTEXT.md BUSINESS_CONTEXT.md .gitattributes .gitignore
git commit -m "chore: set up invar"
```

**4. Fill in the context files.** `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md`
hold your team's conventions and business rules. Invar never writes their
content. Until they hold something under their headings, they are treated as
absent.

## For a teammate: nothing to install

Pull the repository and start Claude Code in it. When Claude Code asks whether
you trust the folder, accept: that is what lets the project's settings install
the plugin.

**Check that it works, in a session:**
- `/plugin` lists `invar`, and the **Errors** tab is empty.
- `/hooks` shows the invar hooks (`SessionStart`, `SubagentStart`, `SubagentStop`, `PreToolUse`).
- `/agents` shows `planner`, `coder` and `reviewer`.
- `/invar:approve`, `/invar:relock`, `/invar:log-approve` and `/invar:done` appear in the command list.

## For yourself only: install for every project

```bash
claude plugin marketplace add webzards-org/invar#dist
claude plugin install invar@invar
```

This enables the plugin in every project, including the ones without
`invar init`, where the `SessionStart` hook reports an error at each start.
The per-project setup above avoids that.

## Updating

Each release bumps the plugin's version. To take a new release:

```bash
claude plugin marketplace update invar
claude plugin update invar@invar
```

Or, in a session, turn on **Enable auto-update** for the `invar` marketplace
under `/plugin` → **Marketplaces**.

## Optional: the CLI in your terminal

Not required: the plugin brings its own binary. You need the CLI in a terminal
only to run `invar` outside Claude Code, for example `invar plan approve` from
your own shell or in CI.

Take the binary from the `dist` branch:

```bash
git clone --depth 1 -b dist git@github.com:webzards-org/invar.git /tmp/invar-dist
# Linux x86_64:
sudo install -m 755 /tmp/invar-dist/invar/bin/invar-linux-x86_64 /usr/local/bin/invar
# macOS Apple silicon: invar-darwin-arm64; Intel: invar-darwin-x86_64
rm -rf /tmp/invar-dist
invar --version
```

## Optional: verify the locks in CI

A locked acceptance test is only worth something if CI notices when it changes. Run
`invar acceptance verify` on every push. It runs no tests and needs no Plan and no git
history, so a shallow checkout is enough and it still works after `invar done` has deleted
the Plan:

```yaml
- uses: actions/checkout@v4
# Install the CLI as above, then:
- name: Verify acceptance locks
  run: invar acceptance verify
```

It prints `verified <N> locked file(s)`, or fails with every problem: a locked file whose
digest no longer matches, a malformed `invar-lock:` line, or a pending line nobody locked.
Whether the tests pass is your own test step. With no `acceptance:` section in `invar.yaml`
it warns and exits 0.

`verify` needs no history, but `invar done` does: it compares the branch with its merge base,
and refuses when it cannot. To run `done` in CI, check out with full history and give git the
base branch by the name invar looks for, since `actions/checkout` creates only `origin/develop`:

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
# Install the CLI as above, then, with `develop` as your base branch:
- run: git branch develop origin/develop
- name: Close the cycle
  run: invar done SHOP-214 --commit "chore: remove the Plan of SHOP-214"
```

In a checkout you already have, `git fetch --unshallow origin develop:develop` does the same.
Without the `develop:develop`, the fetch brings the history and still creates no branch.

---

## Troubleshooting

**`error: unrecognized subcommand 'init'`, or any command missing.** An older
`invar` is earlier on your `PATH`, often from a `cargo install` in
`~/.cargo/bin`. Find every copy and remove the old one:

```bash
which -a invar
for b in $(which -a invar); do echo "$b"; "$b" --version; done
hash -r
```

**A hook error with exit code 127, "no binary for …".** The plugin has no
binary for this platform, or the plugin was copied without its `bin/`
directory. Install from the `dist` branch as above, never from `main`: `main`
holds no binaries.

**`SessionStart` hook error: "not inside an invar project".** The plugin is
enabled in a project where `invar init` has not run. Run it, or install the
plugin per project instead of for your user.

### Private repository access

Claude Code clones the marketplace with your own git credentials and never
prompts for them:

- **SSH** (the default when it works): your key must be loaded in `ssh-agent`,
  and `github.com` must be in `known_hosts`. Check with `ssh -T git@github.com`.
- **HTTPS**: run `gh auth login` and `gh auth setup-git` once, then set
  `CLAUDE_CODE_PLUGIN_PREFER_HTTPS=1` so Claude Code skips SSH.

---

## For maintainers: publishing a release

`.github/workflows/release.yml` builds the binaries (static musl for Linux,
both macOS architectures) and publishes the plugin with them on the `dist`
branch. Users install from `dist`.

1. **Bump the version** in `plugin/.claude-plugin/plugin.json` and in
   `core/Cargo.toml`, to the same value. Users only receive a release whose
   plugin version changed: with the same version, their cached copy stays.
2. Merge to `main`.
3. Tag and push:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

4. Check that the workflow ran, and that `dist` holds
   `invar/bin/invar-linux-x86_64`, `invar-darwin-arm64` and
   `invar-darwin-x86_64`.

Never commit a binary to `main`; `core/tests/plugin.rs` fails if one appears.

For local development of the plugin itself, see `plugin/NOTES.md` §3:
`claude --plugin-dir plugin`, with a locally built binary copied to
`plugin/bin/invar-<os>-<arch>`.
