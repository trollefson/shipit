# Command-Line Help for `shipit`

This document contains the help content for the `shipit` command-line program.

**Command Overview:**

* [`shipit`↴](#shipit)
* [`shipit b2b`↴](#shipit-b2b)
* [`shipit b2t`↴](#shipit-b2t)
* [`shipit t2r`↴](#shipit-t2r)
* [`shipit init`↴](#shipit-init)

## `shipit`

**Usage:** `shipit [OPTIONS] [COMMAND]`

###### **Subcommands:**

* `b2b` — Open a merge/pull request from a source branch to a target branch
* `b2t` — Create an annotated tag on a branch with formatted release notes
* `t2r` — Create a platform release from an existing annotated tag
* `init` — Write the default config to the platform config directory (overwrites existing config)

###### **Options:**

* `-v`, `--verbose` — Increase log verbosity (-v for info, -vv for debug)



## `shipit b2b`

Open a merge/pull request from a source branch to a target branch

**Usage:** `shipit b2b [OPTIONS] <SOURCE> <TARGET>`

###### **Arguments:**

* `<SOURCE>` — Source branch to open the merge/pull request from
* `<TARGET>` — Target branch to merge into

###### **Options:**

* `--agent <AGENT>` — Use an agent to generate the merge/pull request title and description (e.g., ollama, shipit)

  Possible values: `ollama`, `shipit`

* `--dry-run` — Print the merge/pull request details without creating it
* `--dir <DIR>` — Path to the git repository (defaults to current directory)
* `--id <ID>` — GitLab project id or GitHub 'owner/repo' (auto-detected from remote url if not provided)
* `--remote <REMOTE>` — Name of the git remote to use

  Default value: `origin`
* `--prompt <PROMPT>` — Prompt prefix to send to Ollama (overrides the config file value)
* `--title <TITLE>` — Title to use for the merge/pull request (defaults to 'source to target')
* `--description <DESCRIPTION>` — Description to use for the merge/pull request (skips commit discovery and ai summary)
* `--only-merges` — Only include merge commits in the discovered commits



## `shipit b2t`

Create an annotated tag on a branch with formatted release notes

**Usage:** `shipit b2t [OPTIONS] <BRANCH> <TAG>`

###### **Arguments:**

* `<BRANCH>` — Branch to create the tag on
* `<TAG>` — Name of the tag to create

###### **Options:**

* `--agent <AGENT>` — Use an agent to generate the tag notes (e.g., ollama, shipit)

  Possible values: `ollama`, `shipit`

* `--dry-run` — Print the tag notes without creating it
* `--dir <DIR>` — Path to the git repository (defaults to current directory)
* `--id <ID>` — GitLab project id or GitHub 'owner/repo' (auto-detected from remote url if not provided)
* `--remote <REMOTE>` — Name of the git remote to use

  Default value: `origin`
* `--prompt <PROMPT>` — Prompt prefix to send to Ollama (overrides the config file value)
* `--description <DESCRIPTION>` — Description to use for the tag notes (skips commit discovery and ai summary)
* `--only-merges` — Only include merge commits in the discovered commits
* `--latest-tag <LATEST_TAG>` — The most recent tag to compare against (defaults to auto-detected most recent tag on the branch)



## `shipit t2r`

Create a platform release from an existing annotated tag

**Usage:** `shipit t2r [OPTIONS]`

###### **Options:**

* `--tag <TAG>` — Tag to create the release for (defaults to the latest tag on the platform)
* `--dry-run` — Print the release details without creating it
* `--dir <DIR>` — Path to the git repository (defaults to current directory)
* `--id <ID>` — GitLab project id or GitHub 'owner/repo' (auto-detected from remote url if not provided)
* `--remote <REMOTE>` — Name of the git remote to use

  Default value: `origin`



## `shipit init`

Write the default config to the platform config directory (overwrites existing config)

**Usage:** `shipit init`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>

