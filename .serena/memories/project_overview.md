## Project overview
- Purpose: Provide a CLI script that swaps the branches/worktrees inside a Git repository, including staged and untracked changes, so that two worktrees exchange their branches and working state.
- Tech stack: Bash script (`swap.sh`) executed with `/bin/bash`, heavily relies on the `git` CLI.
- Structure: Flat repo with a single executable script `swap.sh`. No libraries or modules yet; everything happens inside that script.
- Runtime dependencies: standard Unix tools plus `git` with worktree support.
- Platform: Developed/tested on macOS (Darwin).