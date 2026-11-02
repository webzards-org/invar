# Security policy

## Reporting a vulnerability

**Do not open a public issue for a security problem.** Report it privately
through GitHub: on the repository's **Security** tab, choose **Report a
vulnerability**. Only the maintainers see the report.

Please include what you found, the version (`invar --version`), and the steps
to reproduce it. You will get an answer within seven days. Once a fix is
released, the advisory is published, with credit to you unless you prefer
otherwise.

## Supported versions

Only the latest release receives security fixes.

## What counts

Invar runs inside a developer's Claude Code session and in CI, so these are in
scope:

- the `invar` binary: anything that lets a file it reads (a Plan, a context
  file, a test file, a JUnit report, `invar.yaml`) make it execute code, write
  outside the project, or leak a file's content;
- the plugin's hooks and skills: anything that lets the agent run a command
  only the developer may run (`plan approve`, `acceptance unlock`,
  `decision approve`, `done`), or get past `plan guard` by another route than
  the ones `plugin/NOTES.md` already records as known;
- the release: a binary on `dist` or in a release that was not built by
  `.github/workflows/release.yml` from the tagged commit.

Not a vulnerability: `plan guard` is a convenience, not a boundary, and a write
through `Bash` is caught at `plan check`, `done` and `acceptance verify`
instead (`plugin/NOTES.md` §11). A report showing one of those checks missing
such a write is in scope.

## Verifying what you run

Every released binary has a build provenance attestation. See
[`docs/INSTALL.md`](docs/INSTALL.md#verifying-a-binary).
