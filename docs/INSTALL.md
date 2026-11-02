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
absent. [`USAGE.md`](USAGE.md) says what goes in each.

**5. Optional: turn on locked acceptance tests.** See
[Acceptance tests](#acceptance-tests) below. Without them, the cycle works as
before: the coder writes its own tests.

Then read [`USAGE.md`](USAGE.md) for the cycle itself.

## For a teammate: nothing to install

Pull the repository and start Claude Code in it. When Claude Code asks whether
you trust the folder, accept: that is what lets the project's settings install
the plugin.

**Check that it works, in a session:**
- `/plugin` lists `invar`, and the **Errors** tab is empty.
- `/hooks` shows the invar hooks (`SessionStart`, `SubagentStart`, `SubagentStop`, `PreToolUse`).
- `/agents` shows `planner`, `test-writer`, `coder` and `reviewer`.
- `/invar:approve`, `/invar:relock`, `/invar:review`, `/invar:log`, `/invar:log-approve`
  and `/invar:done` appear in the command list.

## Acceptance tests

With acceptance tests on, a test-writer agent turns each acceptance criterion
into a test before the coder starts, proves it fails, and locks it, so the
coder cannot weaken it. They are on per project, through `invar.yaml`, and
decided per ticket at approval (`/invar:approve --acceptance` or
`--no-acceptance`).

**1. Your test setup must work on its own first.** The test-writer writes
tests; it does not set up your test environment. For a Laravel project with
Pest:

- **A test database like production's**: MySQL or PostgreSQL, not SQLite in
  memory. Strict mode, JSON columns and collation differ, and acceptance
  criteria tend to touch exactly those. Set it in `phpunit.xml` (`DB_CONNECTION`,
  `DB_DATABASE`) or `.env.testing`.
- **`RefreshDatabase`** for the acceptance directory too, in `tests/Pest.php`
  (Pest 3 and later):
  `pest()->extend(Tests\TestCase::class)->use(Illuminate\Foundation\Testing\RefreshDatabase::class)->in('Feature', 'Acceptance');`
- **A factory for every model** a criterion may need. The test-writer never
  writes or changes factories; a missing one is a `BLOCKED`.
- **The acceptance directory in a test suite**, so your own CI runs these tests
  too: add `<directory>tests/Acceptance</directory>` to a `<testsuite>` in
  `phpunit.xml`.
- **`AI_CONTEXT.md`, under `## Tests`**: how this project writes tests. The
  framework, the database, what may be faked (`Http::fake`, `Mail::fake`) and
  what never is (your own models, scopes and policies).

Check it: `vendor/bin/pest` runs green on a clean checkout.

**2. Turn it on in `invar.yaml`.** `invar init` writes the section commented
out. Uncomment it and adjust:

```yaml
acceptance:
  required: true          # the default at /invar:approve; per ticket you can still say otherwise
  dir: tests/Acceptance
  run: ["vendor/bin/pest", "--colors=never", "--log-junit", "{report}", "{files}"]
  fakes_allowed: [http, mail, queue, notification]
```

- `run` is a list: the program, then each argument as its own element. It runs
  without a shell, from the project root. `{report}` becomes a temporary path
  for the JUnit XML report, and `{files}` the test files to run. Both must
  appear exactly once.
- For PHPUnit: `["vendor/bin/phpunit", "--colors=never", "--log-junit", "{report}", "{files}"]`.
  Any runner that writes JUnit XML works.
- **In a container** (Docker, Sail), the runner must be able to write the report
  to the path invar gives, which is in the host's temp directory, and see the
  test files at the paths invar passes, relative to the project root. Sail
  mounts the project at `/var/www/html` and not `/tmp`, so it does not work as
  it is in 0.1.0. Run the tests on the host, or use a container started with
  the project and the temp directory mounted at the same paths, for example
  `["docker", "run", "--rm", "-v", "/tmp:/tmp", "-v", "/path/to/project:/path/to/project", "-w", "/path/to/project", "<image>", "vendor/bin/pest", "--colors=never", "--log-junit", "{report}", "{files}"]`.

**3. Commit `invar.yaml`, and add the CI check** from
[Verify the locks in CI](#optional-verify-the-locks-in-ci).

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

**An agent stops at once, and says the Plan changed or `acceptance:` is
unknown.** The agents run `invar` through their Bash tool, which takes the first
`invar` on `PATH`. If an older CLI is there (a `cargo install` from before
0.1.0), it reads the Plan differently from the plugin's. Hooks are not affected,
because they name the plugin's binary. Run `which -a invar` as above and remove
or update the old copy. The main session refuses to start anything when
`invar plan check` prints no `acceptance:` line, for the same reason.

**`invar acceptance lock` says "the tests did not run".** The runner could not
even load the tests (a config error, a missing extension, the app failing to
boot) and wrote an empty report. The message shows the end of its output. Run
the `run` command by hand with one test file to see the full error.

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
5. Create a GitHub release for the tag, with that version's section of
   `CHANGELOG.md` as its notes. The workflow publishes `dist` only.

Never commit a binary to `main`; `core/tests/plugin.rs` fails if one appears.

For local development of the plugin itself, see `plugin/NOTES.md` §3:
`claude --plugin-dir plugin`, with a locally built binary copied to
`plugin/bin/invar-<os>-<arch>`.
