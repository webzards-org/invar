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
  `phpunit.xml`. `invar acceptance verify --results` in CI refuses a locked
  file your suite did not run.
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

Take the binary for your platform from the
[latest release](https://github.com/webzards-org/invar/releases/latest)
(`invar-linux-x86_64`, `invar-darwin-arm64` or `invar-darwin-x86_64`), check it,
and put it on your `PATH`. With the GitHub CLI:

```bash
gh release download --repo webzards-org/invar --pattern 'invar-linux-x86_64' --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS      # macOS: shasum -a 256 --check --ignore-missing SHA256SUMS
gh attestation verify invar-linux-x86_64 --repo webzards-org/invar
sudo install -m 755 invar-linux-x86_64 /usr/local/bin/invar
invar --version
```

Without the GitHub CLI, download the same two files from the release page and
run the `sha256sum` line.

### Verifying a binary

Each binary is built by `.github/workflows/release.yml` on GitHub's runners, from
the tagged commit, and gets a signed
[build provenance attestation](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations).
`gh attestation verify` checks that a file is exactly what that workflow built,
and prints the commit it was built from. The binaries the plugin runs are the same
files; check the installed ones with:

```bash
for f in ~/.claude/plugins/cache/invar/invar/*/bin/invar-*; do
  gh attestation verify "$f" --repo webzards-org/invar
done
```

The code they were built from is the tagged commit in this repository.

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
With no `acceptance:` section in `invar.yaml` it warns and exits 0.

### Check that the locked tests pass

An unchanged test can still be red. Invar does not run your test suite: your test step
does, and writes a JUnit report that `verify --results` reads. Every locked file must have
a test case in the report, and every one of them must have passed:

```yaml
- name: Tests
  run: vendor/bin/pest --log-junit junit.xml
- name: Verify acceptance locks, and that the locked tests passed
  if: ${{ !cancelled() }}
  run: invar acceptance verify --results junit.xml
```

`if: ${{ !cancelled() }}` runs it after a red test step too, so that the job says which
locked tests failed. It prints `verified <N> locked file(s), and <M> locked test(s) passed`,
or fails naming the file, the test and its criterion (`AC2`) of each problem:

- a locked file with no test case in any report: **not run by your test suite**. Add the
  acceptance directory to a test suite (see [Acceptance tests](#acceptance-tests));
- a criterion the file's lock line lists (`ac=AC1,AC2`) with no test of it from that file in
  any report: the file **ran only in part**, as when a filter or `phpunit.xml` leaves a test
  out. `invar acceptance lock` writes the list from the run that locked the file, and the
  digest covers it;
- a test that is `failure`, `error` or `skipped`. A skipped locked test is never accepted;
- a test that passed in one run and failed in another, such as a retry. Give every report
  your CI writes (`--results junit.xml retry.xml`), and a test that passes only on a retry
  is reported.

Test cases of files that are not locked are ignored. A missing, empty or unreadable report
is an error naming it, and so is `--results` in a project whose `invar.yaml` has no
`acceptance:` section.

**Files locked before the list existed** have a lock line with no `ac=`. They still verify,
and are checked as before: some test of the file ran, and every one that ran passed. A
file whose tests ran only in part is not caught for them. `verify --results` says so on
stderr, once per file:

```
note: tests/Acceptance/CancelOrderTest.php: locked before the criteria a file covers were recorded, so only a full run of it is checked: some test of it ran, and every one that ran passed. Locking it again records them
```

A file is locked again when a later ticket changes it, or after `/invar:relock`.

**In a container.** With Laravel Sail the tests run with the project at `/var/www/html`.
Pest writes paths relative to the project, which need nothing. PHPUnit writes absolute ones,
which then start with `/var/www/html` and are outside the project root as invar sees it. Say
where the runner saw the project:

```yaml
- name: Tests
  run: vendor/bin/sail exec -T laravel.test vendor/bin/phpunit --log-junit junit.xml
- name: Verify acceptance locks, and that the locked tests passed
  if: ${{ !cancelled() }}
  run: invar acceptance verify --results junit.xml --report-root /var/www/html
```

Without `--report-root`, the error for such a path says to use it.

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

To close the cycle only when the ticket's locked tests passed, give `done` the same reports:
`invar done SHOP-214 --results junit.xml`, with `--report-root` as above when the tests ran in
a container. On a red test it refuses and deletes nothing.

## Optional: require a cycle for some paths

The main session may skip the cycle for trivial work, and nothing in Claude Code can be the
check of that decision, because the session is what the check would hold. CI can be. List
the paths a change may only reach through a cycle:

```yaml
cycle:
  required_paths:
    - "app/Models/**"
    - "app/Policies/**"
    - "app/Actions/**"
    - "database/migrations/"
    - "routes/*.php"
```

Patterns are relative to the project root. `*` stays within one segment, `**` spans any
number of them, a trailing `/` means everything under that directory, and a pattern also
covers a directory's contents (`app/Models` is `app/Models/**`). Unlike `.gitignore`, a
pattern is anchored at the root: `*.php` is only the top-level files. The full rules are in
`contracts/formats/invar-yaml.md` §3.8.

Then check every pull request:

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0
# Install the CLI as above, then:
- name: Changes to required paths went through a cycle
  run: invar cycle check --base origin/${{ github.base_ref }}
```

`fetch-depth: 0`, because the check reads the branch's history from its merge base, and a
shallow checkout is refused with that advice. Give `--base` in CI: `git.base_branch` is read
from the branch, which could point it elsewhere.

The list is read from `invar.yaml` **on the base branch**, so a branch cannot switch off its
own check. A branch that changes the list is checked against the base's, with a note; the
change counts once it is merged. With no list on the base, the check says it is off and
exits 0.

A pull request that changes a required path passes when:

- every commit of it that touched the path was made while the Plan of a cycle closed on the
  branch was approved: approved in an earlier commit, and deleted by `/invar:done` in a later
  one. A change made before the approval, or after `done`, is not covered, whatever other
  cycle the branch closed. One pull request may close several tickets, and each covers the
  commits made while its Plan was approved. A Plan still there is an open cycle, named in the
  output, and covers nothing;
- or a commit of the branch has an `Invar-Skip:` trailer with a reason, which the developer
  writes:

  ```bash
  git commit --amend --trailer "Invar-Skip: a comment typo in Order, no behavior"
  ```

  A trailer survives `git rebase --signoff`, which drops commit signatures. One trailer
  anywhere on the branch covers it, since the pull request is what is reviewed, and the
  reason is printed in the job's output.

Otherwise it fails, listing each required path that is not covered, the pattern it matched,
and the commits that touched it outside a cycle.
Changes under `.invar/` never count. The plugin's guard denies an agent's `git commit` that
writes the trailer, but that is a scan of the command line: it is not yet proof that a
person wrote it. A signed skip comes with signed approvals (#154).

## Optional: signed approvals

The checksum shows that a Plan has not changed since it was approved. It does not
show who approved it: anything that can run `invar` can compute a checksum, and
`Approved-by` is your git `user.name`, which anyone on the machine can set. A
signed approval adds a signature by a key you hold, over the ticket and the
checksum, written next to the Plan as `approval.sig`.

**What it is worth depends on the key.** The agent runs in your shell, with your
environment, including `SSH_AUTH_SOCK`. A key it can use without you gives no more
than the checksum:

- a key file with no passphrase, or a key loaded in `ssh-agent` without
  confirmation, signs for the agent as readily as for you;
- a key that asks you on every use signs only when you answer. Use one of:
  - a security key (FIDO2), which needs a touch:
    `ssh-keygen -t ed25519-sk -f ~/.ssh/id_invar_sk`;
  - a key added to the agent with confirmation: `ssh-add -c ~/.ssh/id_invar`,
    which asks through your desktop's askpass each time it is used;
  - a passphrase key that is not loaded in the agent, with an askpass to ask for
    the passphrase.

`/invar:approve` runs without a terminal, so a passphrase or a confirmation is
asked through `SSH_ASKPASS`. Set it before you start Claude Code, for example
`export SSH_ASKPASS=/usr/libexec/gcr4-ssh-askpass` on a GNOME desktop. Without one,
a passphrase key fails at once, with ssh-keygen's message, and nothing is approved.
A touch on a security key needs no askpass.

**The agent can trigger the same prompt.** The prompt says which key, not what is
being signed. Answer it only right after you have run `/invar:approve` yourself. A
prompt you did not expect is a reason to say no, and to find out what asked.

To set it up:

1. Tell git to sign with that key, as for signed commits:

   ```bash
   git config --global gpg.format ssh
   git config --global user.signingkey ~/.ssh/id_invar_sk.pub
   ```

   `gpg.ssh.program` is used when you have set one, such as a password manager's
   signer.

2. Turn it on in `invar.yaml`:

   ```yaml
   approval:
     signed: true
   ```

3. List who may approve in `.invar/approvers`, one line per key, in git's
   allowed-signers format, and merge it to your base branch through review:

   ```
   dev@example.com namespaces="invar-approval" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...
   ```

   A check of approvals reads this file from the base branch, never from the
   branch being checked, so a branch cannot add its own approver. Protect it with
   `CODEOWNERS` like any file that decides who may merge.

`plan approve` then signs every approval, within 90 seconds, and refuses one it
cannot sign. Nothing else changes for the team: the agents never see the key.

**Checking a signature by hand.** For a Plan as it is now, against the list on
`origin/main`:

```bash
spec=.invar/plans/SHOP-214-cancel-unpaid-order/spec.md
git show origin/main:.invar/approvers > /tmp/approvers
printf 'invar-approval 1\nticket: %s\napproved-checksum: %s\n' \
  "$(sed -n 's/^Ticket: //p' "$spec")" "$(sed -n 's/^Approved-checksum: //p' "$spec")" |
  ssh-keygen -Y verify -f /tmp/approvers -I dev@example.com -n invar-approval \
    -s "$(dirname "$spec")/approval.sig"
```

`Good "invar-approval" signature for dev@example.com` is the answer you want.
`invar plan check` and `invar done` do not read the signature: they check the
checksum, as before. A signature is a guarantee only where it is checked, and
checked where the agent cannot change the answer: in CI, against the base branch's
list.

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

---

## For maintainers: publishing a release

`.github/workflows/release.yml` builds the binaries (static musl for Linux,
both macOS architectures), attests their provenance, publishes the plugin with
them on the `dist` branch, and attaches them with `SHA256SUMS` to a GitHub
release for the tag. Users install the plugin from `dist`.

Attestations need a public repository (on a private one they need GitHub
Enterprise Cloud): on a private repository the workflow stops at
**Attest build provenance**, before it publishes anything.

1. **Bump the version** in `plugin/.claude-plugin/plugin.json` and in
   `core/Cargo.toml`, to the same value. Users only receive a release whose
   plugin version changed: with the same version, their cached copy stays.
2. Merge to `main`.
3. Tag and push:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

4. Check that the workflow ran, that `dist` holds
   `invar/bin/invar-linux-x86_64`, `invar-darwin-arm64` and
   `invar-darwin-x86_64`, and that the release has the same three files and
   `SHA256SUMS`.
5. Put that version's section of `CHANGELOG.md` into the release notes:
   `gh release edit v0.2.0 --notes-file <file>`.

Never commit a binary to `main`; `core/tests/plugin.rs` fails if one appears.

For local development of the plugin itself, see `plugin/NOTES.md` §3:
`claude --plugin-dir plugin`, with a locally built binary copied to
`plugin/bin/invar-<os>-<arch>`.
