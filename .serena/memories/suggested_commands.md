## Suggested commands
- `./swap.sh <destination_worktree_dir> <source_branch_name>` — swap the branches (and working tree state, including untracked files) between the given destination worktree directory and the worktree that hosts the specified branch.
- `bash swap.sh ...` — alternative invocation if the executable bit is missing.
- `shellcheck swap.sh` — optional linting for the Bash script.
- Standard Git commands (`git worktree list`, `git status`, `git switch`) are useful for verifying results before/after the swap.