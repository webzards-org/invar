# Contract: the signed approval

**Version:** 1.0.0 — initial version; see §9
**Status:** stable
**Implements:** #154 (an approval the agent cannot produce); builds on
`plan-header.md` §5.3 (what `plan approve` writes) and `invar-yaml.md` §3.7
(`approval.signed`)
**Read by:** the core (`plan approve` signs); a verifier of approvals (§6); any
later reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR. An absent key or file means the
> previous behavior.

---

## 1. Scope

`Approved-checksum` shows that a Plan has not changed since it was approved. It
does not show who approved it: the checksum is computed by a binary the agent can
also run, and `Approved-by` is text (`plan-header.md` §5). This contract adds a
signature, made with a key the developer holds, over what was approved. It
defines:

- the list of people who may approve, `.invar/approvers` (§2);
- the statement that is signed (§3) and the file that holds the signature (§4);
- how `plan approve` signs, when `approval.signed` is on (§5);
- what a verifier checks, and where it reads the list from (§6).

With `approval.signed` absent or `false`, nothing here applies: no signature is
written, and none is read.

## 2. The approvers list

```
.invar/approvers
```

A text file in OpenSSH's allowed-signers format (`ssh-keygen(1)`, "ALLOWED
SIGNERS"), the same format as git's `gpg.ssh.allowedSignersFile`. One line per
key: a principal, optional options, the public key.

```
dev@example.com namespaces="invar-approval" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...
```

- The **principal** is how the approver is reported (§6.4): an email address or
  any name without spaces.
- `namespaces="invar-approval"` is recommended. It limits the key to approvals,
  so the same line does not also make it valid for git commit signatures.
- A verifier reads the list **from the base branch only**
  (`git show <base>:.invar/approvers`), never from the work tree or the branch
  being checked (D-3). A branch that adds a key to the list does not make that
  key an approver until the change is on the base branch. Changes to the file
  go through the project's own review: CODEOWNERS, branch protection.
- `plan approve` does not read the list. A signature by a key that is not in it
  is made and written, and a verifier refuses it.

## 3. The statement

What is signed is three lines, each ending with `\n` (LF), in this order and with
nothing else:

```
invar-approval 1
ticket: <Ticket>
approved-checksum: <Approved-checksum>
```

`<Ticket>` and `<Approved-checksum>` are the values of the Plan's header after
the approval is written (`plan-header.md` §5.3). The `1` is the version of this
statement. A later version changes that number and is a contract change.

The statement holds no date and no name: `Approved-checksum` already covers the
body and `Acceptance` (D-19 of `plan-header.md`), and the signer is known from
the key (D-2).

## 4. The signature file

```
.invar/plans/<TICKET>-<slug>/approval.sig
```

Next to the Plan's `spec.md`. It holds exactly what `ssh-keygen -Y sign` writes:
an armored SSH signature (`-----BEGIN SSH SIGNATURE-----` … `-----END SSH
SIGNATURE-----`), made with the namespace `invar-approval`, over the statement of
§3. Its line endings are LF.

It belongs to one approval: a new approval of the Plan replaces it. It is
deleted with the Plan directory by `invar done`. A `failed` entry, which returns
the Plan to `draft`, leaves it where it is; for a draft it means nothing, and the
next approval replaces it.

An absent file means an unsigned approval: the behavior before this contract.

## 5. Signing — `plan approve` with `approval.signed: true`

1. **The key is git's.** `gpg.format` must be `ssh`, and `user.signingkey` must
   be set, as `git config` reads them in the project root. Its value is what git
   accepts: a path to a public or private key file (`~/` expands to the home
   directory), or a public key written in place, as `key::ssh-ed25519 AAAA…` or
   `ssh-ed25519 AAAA…`, which is written to a temporary file for the signing
   program and removed after it. Invar never reads a private key.
2. **The program is git's.** `gpg.ssh.program` when it is set, else
   `ssh-keygen`, run as `<program> -Y sign -n invar-approval -f <key>`, with
   the statement on standard input and the signature read from standard output.
   No shell is involved. The developer's agent, passphrase prompt, askpass or
   security key works as it does for a signed commit.
3. **Within 90 seconds.** A signing program still running after 90 seconds is
   killed, and the approval is refused (D-4).
4. **Before anything is written.** The order of `plan-header.md` §5.3 is:
   validate, read approver and date, print the criteria, **sign**, write. A
   refusal at the signing step leaves the Plan and any earlier `approval.sig`
   as they were.
5. **Written together.** The signature is written to `approval.sig`, replacing
   the file atomically, then the header. If the header cannot be written, the
   earlier `approval.sig` is put back, or the new one removed when there was
   none.

`plan approve` refuses, and writes nothing, when:

- `gpg.format` is not `ssh`, or `user.signingkey` is not set;
- the signing program cannot be started, exits non-zero, or writes no
  signature. The refusal repeats what the program wrote to standard error;
- the deadline of step 3 passes.

An approval that is already current (`plan-header.md` §5.3, step 2) writes
nothing, with or without a signature on disk.

## 6. Verifying

What a verifier of approvals checks for a branch, against a base branch `<base>`.
It reads, and writes nothing.

### 6.1 Preconditions

- **Full history.** Every commit from the merge base of `<base>` and `HEAD` to
  `HEAD` is read. A shallow checkout is refused, naming full history
  (`fetch-depth: 0` for `actions/checkout`).
- **The list from the base.** `<base>:.invar/approvers`, never the work tree's
  copy (§2). When the base has no such file, the run is refused: no approval can
  be verified.
- **`ssh-keygen` directly**, never through git's verification (`git
  verify-commit`, `%G?`), whose program a repository's config can change (D-5).

### 6.2 What each commit is, for each Plan

For each commit `C` from the merge base (excluded) to `HEAD`, oldest first, and
each `.invar/plans/*/spec.md` at `C` or at its first parent, the Plan at `C` is
compared with the Plan at that parent:

- an **approval**: `Status: approved` at `C`, and at the parent the Plan is
  absent, is a draft, or has another `Approved-checksum` or another
  `approval.sig`;
- a **body change while approved**: `Status: approved` at both, the same
  `Approved-checksum`, and another approval digest (`plan-header.md` §5.3);
- a **reset**: approved at the parent, a draft at `C`;
- a **deletion**: present at the parent, absent at `C`;
- nothing to check, otherwise.

### 6.3 Rules

1. An **approval** has `approval.sig` at `C`, and the signature verifies
   (`ssh-keygen -Y verify -n invar-approval`) over the statement built from the
   Plan at `C`, for a principal in the base's list. The Plan's
   `Approved-checksum` at `C` equals its approval digest at `C`.
2. A **body change while approved** is followed, at a later commit up to `HEAD`,
   by an approval that passes rule 1, a reset or a deletion.
3. A **deletion** comes after an approval that passes rule 1, with no body
   change while approved between them that rule 2 has not settled. A Plan
   created and deleted on the branch without one is refused.
4. A **reset** needs no signature. Nor does a draft.
5. At `HEAD`, a Plan that is approved passes rule 1 as it stands.

A branch that has no Plan at any commit passes, with a note that there was
nothing to verify. A branch that changes `.invar/approvers` passes or fails as
it would without that change, with a note that the change applies once it is on
the base.

Every problem is reported in one run, each naming the commit and the Plan, and
the run then fails.

### 6.4 Who approved

For each approval that passes rule 1, the verifier reports the principal the
signature matched (`ssh-keygen -Y find-principals`), and the key's fingerprint.
That is the record of who approved. When the header's `Approved-by` differs, it
says so: the header stays text.

## 7. Limits

A signature shows that the key was used, not that a person meant to approve this
Plan. Two cases matter:

- **A key usable without a person** — a key file with no passphrase, or a key
  loaded in an `ssh-agent` that the agent's shell can reach — can be used by the
  agent as well as by the developer. A signature with it proves no more than
  `Approved-checksum`.
- **A key that asks a person on each use** — a FIDO key (`sk-ssh-ed25519`) that
  needs a touch, a key added with `ssh-add -c`, a passphrase asked for through
  an askpass — can also be made to ask by the agent. The prompt does not say what
  is being signed. The guarantee is a person who answers the prompt only right
  after asking for the approval themselves.

What a verifier adds over `Approved-checksum` holds only where the agent cannot
change it: in CI, with the list read from the base branch.

## 8. Decisions and their reasons

**D-1 — The approval is signed, not the commit** (1.0.0). A commit signature
covers the commit's parent, so any rebase, `--signoff` included, unsigns it; a
rebase with `commit.gpgsign=true` re-signs every commit with the rebaser's key,
a forged approval commit included; a squash merge drops every signature. Measured
in `plugin/NOTES.md` §19 (A4). A signature over the statement depends on no
commit, so it survives every way a branch reaches the base.

**D-2 — The statement is the ticket and the checksum** (1.0.0). The checksum
already covers what was approved, and the ticket binds it to one Plan. A date or
a name in it would be text the signer chose; the key says who signed.

**D-3 — The list is read from the base** (1.0.0). A list read from the branch
could be extended by the branch. Only the base, which the branch cannot change
before it is merged, says who may approve.

**D-4 — A deadline of 90 seconds** (1.0.0). Claude Code moves a skill's shell
block to the background after 120 seconds, and the block's output, which
`/invar:approve` decides on, never reaches the session (`plugin/NOTES.md` §19,
A2). Refusing at 90 seconds keeps the outcome in the output.

**D-5 — Signing goes through git's settings; verifying does not** (1.0.0).
Signing with `user.signingkey` and `gpg.ssh.program` uses the setup the developer
already has for commits, a hardware key or a password manager's agent included.
Verifying with `ssh-keygen` directly means no repository config, which the agent
can change on its own machine, takes part in the verdict.

**D-6 — SSH signatures only** (1.0.0). One verifier and one list, with no
keyring or trust database to set up in CI, and namespaces that keep an approval
apart from a commit signature. A GPG signature would need all of those, and
`gpg-agent` caches a passphrase for ten minutes by default, inside which the
agent could sign too.

## 9. Version history

**1.0.0** — initial version (#154).
