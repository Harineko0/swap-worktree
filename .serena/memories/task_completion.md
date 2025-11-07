## Task completion checklist
- Re-run or simulate `./swap.sh` (with safe dummy repos) if the logic changes, to ensure branch swapping works end to end.
- Since there are no automated tests, manually verify git worktree states and stash handling.
- If you touched the shell script, optionally run `shellcheck swap.sh` to catch common Bash issues.
- Confirm the script remains executable (`chmod +x swap.sh`) before handing off.
- Summarize changes and mention any manual verification performed when delivering work.