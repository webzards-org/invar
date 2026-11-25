# Contract: the signed approval

**Version:** 1.0.0 — initial version; see §9
**Status:** stable
**Implements:** #154 (an approval the agent cannot produce); builds on
`plan-header.md` §5.3 (what `plan approve` writes) and `invar-yaml.md` §3.7
(`approval.signed`)
**Read by:** the core (`plan approve` signs, `decision record --type failed`
removes the signature, `approval verify` checks it, §6); any later
reimplementation

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
- what a verifier checks, and where it reads the list from (§6);
- the same for two more decisions of a person, a skip of the cycle and a change
  that weakens a check, signed as trailers (§6.5).

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
- `namespaces=` limits what the key may sign. A line without it accepts every
  namespace, git's commit signatures (`git`) included. The namespaces here are
  `invar-approval` (§3), `invar-skip` and `invar-config` (§6.5), so a line can
  give a person some of the three and not the others. One who approves Plans
  and skips the cycle, but does not weaken the configuration:

  ```
  lead@example.com namespaces="invar-approval,invar-skip" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...
  ```
- A verifier reads the list **from the base branch only**
  (`git show <base>:.invar/approvers`), never from the work tree or the branch
  being checked (D-3). A branch that adds a key to the list does not make that
  key an approver until the change is on the base branch. Changes to the file
  go through the project's own review: CODEOWNERS, branch protection.
- `plan approve` does not read the list. A signature by a key that is not in it
  is made and written, and a verifier refuses it.

## 3. The statement

What is signed is four lines, each ending with `\n` (LF), in this order and with
nothing else:

```
invar-approval 1
ticket: <Ticket>
approved-checksum: <Approved-checksum>
approval-id: <Approval-id>
```

`<Ticket>`, `<Approved-checksum>` and `<Approval-id>` are the values of the
Plan's header after the approval is written (`plan-header.md` §5.3). The `1` is
the version of this statement. A later version changes that number and is a
contract change.

`<Approval-id>` is 128 random bits from the operating system, written as 32
lowercase hex digits, new for every approval, a re-approval of the same body
included (D-7). `plan approve` chooses it just before it signs, and writes it to
the header only with a signature: an unsigned approval has none.

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
the Plan to `draft`, deletes it, and puts it back if the entry cannot be written,
as it does the Plan (`decision-entry.md` §7). That is for clarity, not security:
the file stays in git's history, and what keeps it from being put back is the
replay rule (§6.3, D-7).

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
   killed, and the approval is refused at once (D-4). What the program started
   (an askpass dialog, a password manager's helper) is killed with it when
   there is no controlling terminal, as under `/invar:approve`: the program runs
   in a process group of its own, and the whole group is killed. With a
   terminal, the program stays in the caller's process group, so that it can
   still read a passphrase from it, and only the program is killed. Either way,
   the refusal never waits for what the program started to exit.
4. **Before anything is written.** The order of `plan-header.md` §5.3 is:
   validate, read approver and date, print the criteria, **choose the
   `Approval-id` and sign**, write. A refusal at the signing step leaves the
   Plan and any earlier `approval.sig` as they were.
5. **Written together.** The signature is written to `approval.sig`, replacing
   the file atomically, then the header, with its `Approval-id`. If the header
   cannot be written, the earlier `approval.sig` is put back, or the new one
   removed when there was none.

`plan approve` refuses, and writes nothing, when:

- `gpg.format` is not `ssh`, or `user.signingkey` is not set;
- no random bits can be read for the `Approval-id`;
- the signing program cannot be started, exits non-zero, or writes no
  signature. The refusal repeats what the program wrote to standard error;
- the deadline of step 3 passes.

An approval that is already current (`plan-header.md` §5.3, step 2) writes
nothing, with or without a signature on disk.

## 6. Verifying

What a verifier of approvals checks for a branch, against a base branch `<base>`.
It reads, and writes nothing. The core's verifier is `invar approval verify
[--base <ref>]`, which finds `<base>` as every CI command does (`invar-yaml.md`
§3.3.1): `--base`, else the remote's default branch, else `main`, never
`git.base_branch` from the branch.

### 6.1 Preconditions

- **On when the base says so.** Verification is required when `invar.yaml` at
  `<base>` has `approval.signed: true`. Otherwise the verifier says that it is
  off, and passes. A branch that turns it off is still checked, with a note:
  what a branch is checked against is never the branch's to choose
  (`invar-yaml.md` §3.3.1, §3.7).
- **Full history.** Every commit from the merge base of `<base>` and `HEAD` to
  `HEAD` is read. A shallow checkout is refused, naming full history
  (`fetch-depth: 0` for `actions/checkout`).
- **The branch's own head.** A `HEAD` that is a merge commit whose first parent
  is `<base>` is refused: the branch's commits are not on its first-parent line,
  so §6.2 would see one commit holding the whole branch (D-8). That is the
  commit `actions/checkout` makes for a pull request by default; check out the
  pull request's head instead.
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
  `Approved-checksum` and `approval.sig`, and another approval digest than at
  the parent (`plan-header.md` §5.3);
- a **reset**: approved at the parent, a draft at `C`;
- a **deletion**: present at the parent, absent at `C`;
- nothing to check, otherwise.

A `spec.md` that is not a valid Plan and does not read `Status: draft` counts as
approved, and fails rule 1: an approval that cannot be read is not one that
passes.

### 6.3 Rules

1. An **approval** has `approval.sig` and an `Approval-id` at `C`, and the
   signature verifies (`ssh-keygen -Y verify -n invar-approval`) over the
   statement built from the Plan at `C`, for a principal in the base's list.
   The Plan's `Approved-checksum` at `C` equals its approval digest at `C`. An
   approval with no `Approval-id` is refused.
2. A **body change while approved** is followed, at a later commit up to `HEAD`,
   by an approval that passes rule 1, a reset or a deletion.
3. A **deletion** comes after an approval that passes rule 1: the last approval
   of the Plan on the branch passes it, and no body change while approved since
   was settled only by the deletion itself. A Plan created and deleted on the
   branch without one is refused.
4. A **reset** needs no signature. Nor does a draft.
5. At `HEAD`, a Plan that is approved passes rule 1 as it stands.
6. **No replay.** An approval is refused as replayed, whatever its
   signature, when its `Approval-id` appeared before (D-7):
   - in an earlier approval of the branch;
   - in an approved Plan, a `.invar/plans/*/spec.md` with `Status: approved`,
     at any commit that a ref of the repository reaches (`git log --all`) and
     that is not in `<merge base>..HEAD`: the base's history and every other
     branch, tag and remote-tracking branch included;
   - in a `failed` entry under `.invar/decisions/` at `<base>`, whose
     `approval-id` records the approval it cancelled (`decision-entry.md` §7).

   The refusal names the commit, and a ref that holds it where git can tell,
   or the entry. Only commits a ref reaches are searched: a commit of a branch
   that was deleted, or never fetched (a shallow or single-branch fetch), is
   not seen, and a `failed` entry survives a deleted branch only if it was
   merged to the base. So the verifier needs a checkout that fetched every
   branch: `fetch-depth: 0`, with which `actions/checkout` fetches them all.

A branch that has no Plan at any commit passes, with a note that there was
nothing to verify. A branch that changes `.invar/approvers` passes or fails as
it would without that change, with a note that the change applies once it is on
the base.

Every problem is reported in one run, each naming the commit (its short id and
subject) and the Plan, and the run then fails, non-zero. The last line is one of
`approval verify: off: <why>`, `approval verify: passed: <what>` and `approval
verify: failed`.

### 6.4 Who approved

For each approval that passes rule 1, the verifier reports the principal the
signature matched (`ssh-keygen -Y find-principals`), and the key's fingerprint.
That is the record of who approved. When the header's `Approved-by` differs, it
says so: the header stays text.

### 6.5 Signed skips and configuration changes

Two more decisions of a person are commit trailers, read by the CI checks of
`invar-yaml.md`: `Invar-Skip: <reason>`, a change to required paths made without
a cycle (§3.8 there), and `Invar-Config: <reason>`, a change that weakens a check
(§3.9 there). When `invar.yaml` at the base has `approval.signed: true`, each
counts only with a signature by an approver, made and checked as an approval is.

**The statements**, each line ending with `\n` (LF), in this order and with
nothing else:

```
invar-skip 1
reason: <reason>
change: <before> <after> <path>
```

- `<reason>` is the value of the `Invar-Skip` trailer, trimmed: one line, not
  empty.
- One `change:` line per required change (`invar-yaml.md` §3.8): each path that
  `cycle.required_paths` at the base requires and that differs between the merge
  base and `HEAD`, sorted by path, byte by byte. `<before>` is its git object id
  at the merge base, `<after>` at `HEAD`, and `-` where it is absent. The path
  comes last, so that it may hold spaces; a path with a newline cannot be
  signed.

```
invar-config 1
reason: <reason>
config: <base> <head>
```

- `<reason>` is the value of the `Invar-Config` trailer, trimmed.
- `<base>` and `<head>` are the git object ids of `invar.yaml` at the base and at
  `HEAD`, what `config check` compares (`invar-yaml.md` §3.9), and `-` where it
  is absent.

The namespaces are `invar-skip` and `invar-config`, so that no signature of one
kind is ever valid as another, or as an approval (D-9).

**The trailers.** On one commit of the branch, the reason trailer and beside it
`Invar-Skip-Signature: <signature>`, or `Invar-Config-Signature: <signature>`.
`<signature>` is what `ssh-keygen -Y sign` writes (§4) without its first and
last lines and without line breaks: its base64 on one line. A verifier puts the
armor back, 70 characters a line, as `ssh-keygen` writes it (D-10).

**Making one.** `invar cycle skip --reason <reason>` and `invar config accept
--reason <reason>`, each with `[--base <ref>] [--print]`, are the developer's
commands. The base is found by the rule of the CI commands (`invar-yaml.md`
§3.3.1), because the statement depends on it. Each prints what it covers, signs
as `plan approve` does (§5, steps 1 to 3, in its own namespace), and makes one
empty commit with the two trailers, under the project's own hooks; nothing is
pushed. With `--print` it prints the two trailers instead, and commits nothing.
It refuses, and signs nothing, when `approval.signed` is not on at the base, when
there is nothing to cover (no required change, or no weakening), when the reason
is empty or not one line, and when a path holds a newline.

**Verifying.** A signed trailer counts when its commit is in `<base>..HEAD`, it
has one reason trailer and one signature trailer of its kind, and the signature
verifies in its namespace over the statement built from the branch **as it is
when checked**, for a principal in `<base>:.invar/approvers`, with `ssh-keygen`
directly (§6.1, D-5). Each one that counts is reported with its principal and
the key's fingerprint (§6.4). Each one that does not is named with the reason: no
signature, a signature that does not verify, or one by a key the list does not
have. Since the statement is built from the branch as it is, a signature covers
the change it was made for and nothing else: a later commit to a required path,
a new required path, or a base that changed `invar.yaml` or a covered file,
needs a new signature.

When `approval.signed` is not on at the base, a signature trailer is not read,
and a reason trailer counts as it did before.

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

The replay rule (§6.3, rule 6) sees what a ref reaches, and the base's
`failed` entries. An approval taken from a branch that was deleted without being
merged, whose `failed` entry was never merged either, is not refused. A ref that
holds an earlier version of the branch being checked, such as the pushed branch
before a rebase is pushed, or a branch stacked on it, holds its approvals too,
and an honest approval then reads as a replay: run the check on the branch as it
is pushed, as CI does.

## 8. Decisions and their reasons

**D-1 — The approval is signed, not the commit** (1.0.0). A commit signature
covers the commit's parent, so any rebase, `--signoff` included, unsigns it; a
rebase with `commit.gpgsign=true` re-signs every commit with the rebaser's key,
a forged approval commit included; a squash merge drops every signature. Measured
in `plugin/NOTES.md` §19 (A4). A signature over the statement depends on no
commit, so it survives every way a branch reaches the base.

**D-2 — The statement is the ticket, the checksum and the approval id** (1.0.0).
The checksum already covers what was approved, and the ticket binds it to one
Plan. The id makes each approval a statement of its own (D-7). A date or a name
in it would be text the signer chose; the key says who signed.

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

**D-7 — Every approval has an id of its own, and a verifier refuses one seen
before** (1.0.0, #168). With only the ticket and the checksum in the statement,
two approvals of the same body sign the same three lines, and ed25519 makes the
same signature for the same lines. After a `failed` reset, an agent with a shell
could put back the old header and `approval.sig`, from the work tree or from git
history, and every other rule of §6 would pass it: the signature is good, by a
listed key, over a checksum that matches. Removing `approval.sig` on a reset
(§4) does not close it, since history keeps the file. A random id in the header
and in the statement makes each approval's signature its own, so an approval put
back is one the branch has already seen, and rule 6 refuses it. 128 bits from
the operating system, because the id must not be guessed ahead of a signature,
and no counter or date in the repository can promise that.

Looking at the branch alone was not enough (found in review of PR #169). After a
failed cycle the usual path is a new branch for the same ticket, from the base,
and an approval copied there from the failed branch (`git show <commit>:<path>`)
is new to that branch's history. So a verifier also searches every commit a ref
reaches outside the branch, in one `git log --all` over the Plans, and the base's
`failed` entries, which record the id they cancel and outlive the branch once
merged. Neither can see a commit no ref reaches: that is the limit, stated in
rule 6 and §7, and the reason CI fetches every branch.

**D-8 — The branch's own head, not CI's merge commit** (1.0.0, #168). For a pull
request, `actions/checkout` checks out a merge commit whose first parent is the
base. Its merge base with the base is the base itself, so the commits of §6.2
are that one merge, and a Plan approved without a signature and then deleted is
in none of it. Following every parent instead would count the merge as an
approval of its own, with the same `Approval-id` as the one it merges, and rule
6 would refuse an honest branch. So the verifier follows first parents, and
refuses a `HEAD` that is a merge onto the base, naming what to check out.

**D-9 — A skip or a configuration change is bound to what it covers** (1.0.0,
#168). The candidates were the merge base, HEAD's tree, the content covered, and
a random id as for an approval. Measured in `plugin/NOTES.md` §20: `git rebase
--signoff`, which this repository runs before every merge, changes the merge base
and HEAD's tree, so a signature bound to either would not survive the merge
process, and the merge base does not even cover the commits made after the
signature. The blobs of the covered paths survive the rebase, and change with any
later change to them. What it leaves is a trailer copied to another branch that
makes the same change, byte for byte: the same decision about the same change, so
no id is added to refuse it. Separate namespaces, so that a team can give a person
one of these decisions without the others (§2).

**D-10 — In a trailer, on an empty commit** (1.0.0, #168). A trailer survives a
rebase (`git rebase --signoff` keeps it and the empty commit), leaves nothing in
the tree, and needs no clean-up after the merge. A file under `.invar/` would be
merged into the base, and every later branch would have to tell its own from the
base's. The signature goes on one line, because a trailer that continues on more
lines is easy to break by hand, and the armor is fixed text a verifier can put
back. The command makes an empty commit of its own rather than amend one of the
agent's. A squash merge drops the trailers, which does not matter: the checks run
on the pull request, before it.

## 9. Version history

**1.0.0** — initial version (#154).

Not yet released, so changed in place during review (PR #162):

- §5, step 3: the deadline also kills what the signing program started, when
  there is no terminal, and the refusal never waits for it. Before, only the
  program was killed, and a helper holding its output open kept `plan approve`
  waiting until the helper exited, long past the 90 seconds.

Not yet released, so changed in place again with the verifier (#168):

- §3: the statement is four lines; the fourth is `approval-id: <Approval-id>`,
  the new header key of `plan-header.md` 1.10.0, new on every signed approval.
  Still `invar-approval 1`. D-2 says why, and new D-7.
- §4: a `failed` entry deletes `approval.sig` instead of leaving it, for
  clarity; history still has it.
- §5: `plan approve` chooses the id before it signs, and refuses when it cannot
  read random bits.
- §6: the verifier is `invar approval verify`. §6.1: it is on when the base's
  `invar.yaml` has `approval.signed: true`, whatever the branch says, and it
  refuses a `HEAD` that is a merge commit onto the base (new D-8). §6.2: a body
  change is a digest other than the parent's under the same checksum and
  signature, and a `spec.md` that cannot be read and is not a draft counts as
  approved. §6.3: rule 1 needs an `Approval-id`; rule 3 says which approval a
  deletion follows; new rule 6 refuses a replayed approval; the last line of
  the output. §7: what the replay rule does not cover.
- In review of PR #169, §6.3 rule 6 and D-7: a replay is also an `Approval-id`
  in an approved Plan at a commit any ref reaches outside `<merge base>..HEAD`,
  or in a `failed` entry at the base; the limit, a commit no ref reaches. §7
  says what that leaves.

Not yet released, so changed in place again for signed skips and configuration
changes (#168, its second PR):

- §1, §2: the namespaces `invar-skip` and `invar-config`, and what a line
  without `namespaces=` accepts.
- New §6.5: the two statements, their trailers, the commands that make them, and
  how `cycle check` and `config check` verify them. New D-9 and D-10.

**Why:** the review of #162 found that a reset approval could be replayed from
history with every rule of §6 passing it (D-7), and that §6.1 read the
approvers list from the base but not the switch. Building the verifier showed
that the default pull-request checkout hides the branch from §6.2 (D-8). The
review of PR #169 replayed an approval from a failed branch onto a new branch
for the same ticket, and the branch-only rule passed it (D-7).

**Compatibility:** none needed. 1.0.0 has not been released. An approval signed
by a build of `main` from before this change has no `Approval-id`, and the
verifier refuses it; approving it again signs the new statement.
