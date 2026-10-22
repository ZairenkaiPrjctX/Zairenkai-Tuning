# Licensing and the program boundary

Zairenkai-Tuning is **GPL-3.0-only**: it contains code adapted from three GPL-3.0 projects
(`PROVENANCE.md`). The rest of Zairenkai (App, Core, FAS, Modules' scripts) is Apache-2.0.
This page records why the two can be distributed together, and what keeps that true.

## The boundary

`ztune` is a separate program, and the Apache-2.0 components treat it as one:

| Question | Answer, from the code |
|---|---|
| Shared address space? | No. Core starts `ztune` with `execve` and a fixed argument vector (no shell), and reads its stdout. Nothing links `ztune` code or loads it as a library |
| Shared source? | No. No file in Core, App or Modules includes, copies or translates code from this repository or from its upstreams. Core's adapter and App's parser are written from `CONTRACT.md` |
| What crosses the boundary? | A small, documented, versioned JSON document (`CONTRACT.md`): readings with units, capability states, operation results. Not internal data structures |
| Is `ztune` useful on its own? | Yes. It is a complete command-line tool a person can run from a root shell |
| Is Core useful without it? | Yes. Core runs without `ztune` and reports its capabilities as absent |

On that basis `ztune` and the Apache-2.0 programs are separate works communicating at arm's
length, and the unified module zip is an aggregate (GPLv3 section 5, last paragraph): the GPL
covers `ztune` and does not extend to Core, App or the module scripts.

Using a CLI or a socket is not, by itself, what makes this so. The boundary holds because of
the facts in the table. Any change that alters one of them needs a new licence analysis before
it is merged, for example:

- linking `ztune-core` (or any crate of this repository) into Core or the App;
- copying, porting or "re-implementing from" upstream code in an Apache-2.0 repository;
- turning the contract into a channel for internal structures (a serialized parser state, a
  shared table layout) that only make sense together with this code;
- shipping a combined binary.

## Obligations, and where each is met

| GPLv3 obligation | Where |
|---|---|
| Licence text with the program | `LICENSE` in the repository; the component and the module zip carry it under `licenses/zairenkai-tuning/` |
| Keep copyright and licence notices | Upstream copyright lines in each adapted file's header and in `NOTICE` |
| Mark modified files, with a date | Each adapted file's header: upstream file, commit, "Changes (2026-10-22): …" |
| Corresponding Source with object code (section 6) | `scripts/build-android.sh` builds the source archive from the exact commit, with every linked crate vendored, and ships it inside the component and the module (`source/`). `SOURCE.md` says how to rebuild |
| Build instructions (part of Corresponding Source) | `SOURCE.md` in the component; `scripts/build-android.sh` in the archive |
| No added restrictions | The module zip adds none; the App's install flow does not restrict use of `ztune` |
| Installation information (section 6, User Products) | `ztune` is installed as a file in the module directory, replaceable by the user with root; no signature check prevents running a modified `ztune` |

Notes:

- The module's catalog signing (Zairenkai-App, `integration/module-trust.json`) decides
  whether the App installs a module zip. It does not stop a user from replacing or running a
  modified `ztune`, so it is not a restriction under section 6.
- The upstream licences say "GPL v3" without "or later"; this component chose GPL-3.0-only.
- The licence headers of adapted files must never be changed to Apache-2.0.
