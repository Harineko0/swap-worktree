## Style and conventions
- Shell script uses `bash` with `set -e` for fail-fast behavior.
- Logic is structured in numbered sections with descriptive `echo` logging for each step.
- Error handling: validate arguments, directories, repo state, and print helpful error messages to stderr before exiting.
- No formal linting/formatting is documented, but `shellcheck` would be the natural choice if needed.
- Git interactions prefer explicit `git -C <dir>` invocations over `cd`.