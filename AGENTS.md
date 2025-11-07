## Agent Playbook

A quick-reference for automated or human agents working on `swap-worktree`.

### Project Snapshot

- **Purpose:** Rust CLI that swaps branches (plus untracked/staged work) between two Git worktrees.
- **Entrypoint:** `cargo run -- <dest_worktree_dir> <source_branch>`.
- **Key files:** `Cargo.toml`, `src/main.rs`, `.github/workflows`, `README.md`.

### Everyday Commands

```bash
cargo fmt --all                          # format
cargo clippy --all-targets -- -D warnings # lint
cargo test --all-features                # unit tests
cargo build --release                    # release binary
cargo run -- <dest_dir> <branch>         # manual test
```

### CI / Release

- `.github/workflows/ci.yml` runs fmt, clippy, test, and build on PRs + pushes to `main`.
- `.github/workflows/release.yml` builds binaries for Linux/macOS/Windows and publishes a GitHub release on pushes to `main`.
- `.github/dependabot.yml` triggers daily dependency checks for Cargo + GitHub Actions.

### Contribution Flow

1. Create a feature branch from `main`.
2. Make minimal, well-scoped commits.
3. Run **all** commands in “Everyday Commands”.
4. Open a pull request; CI must pass before merging.

### Validation Checklist

- [ ] `cargo fmt --all`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all-features`
- [ ] `cargo build --release`
- [ ] Manual sanity run of `swap-worktree` if logic changed.
- [ ] Update `README.md`/docs when behavior or workflows evolve.

### Notes

- When editing `src/main.rs`, favor small helper functions and informative log output.
- For GitHub Actions changes, double-check permissions and artifact names.
- Prefer keeping `swap.sh` as historical reference unless explicitly removed.
