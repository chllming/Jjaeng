# Repository Guidelines

## Project Structure & Module Organization
- Workspace root is `Cargo.toml`; the legacy single-crate layout is retired.
- `crates/jjaeng-core/` owns domain logic, storage, OCR, theming, IPC, and shared identity/config helpers.
- `crates/jjaeng-ui/` owns the GTK runtime, preview/editor windows, launchpad, and Hyprland-facing UI wiring.
- `crates/jjaeng-daemon/` provides the daemon binary entrypoint.
- `crates/jjaeng-cli/` provides the end-user `jjaeng` binary and CLI tests.
- Tests are mostly colocated with implementation; CLI integration tests live under `crates/jjaeng-cli/tests/`.

## Build, Test, and Development Commands
- `cargo check --workspace` : fast compile/type validation before commits.
- `cargo test --workspace` : runs workspace unit and CLI integration tests.
- `cargo fmt --check` : enforces formatting used in CI/PR checks.
- `cargo run -p jjaeng-cli -- --help` : exercises the CLI package.
- `cargo run -p jjaeng-daemon --` : launches the daemon entrypoint locally.
- `cargo clippy --all-targets --all-features -- -D warnings` : optional but recommended lint gate.

## Coding Style & Naming Conventions
- Follow Rust 2021 defaults and `rustfmt` output (4-space indentation, standard wrapping).
- Naming: `snake_case` for files/functions/modules, `UpperCamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for constants.
- Keep logic in focused modules instead of growing `mod.rs` files; prefer small, testable helpers.
- Use typed errors (`thiserror`) and module result aliases instead of `unwrap()` in production paths.

## Testing Guidelines
- Add or update unit tests next to changed code with behavior-focused names (example: `capture_region_errors_when_selection_empty`).
- Run `cargo test` locally before opening a PR.
- For UI/runtime behavior changes, manually verify capture flow, preview/editor interactions, keyboard navigation, and temp-file cleanup behavior.

## Commit & Pull Request Guidelines
- Git history uses concise gitmoji-style subjects (examples: `♻️ extract editor runtime module`, `🧩 add collapsible editor options panel`).
- Keep commits single-purpose and imperative.
- Follow `.github/PULL_REQUEST_TEMPLATE.md`: include summary, change checklist, explicit test plan (`cargo check`, `cargo test`, `cargo fmt --check`), and logs/screenshots when UI changes.
- Note runtime dependency impacts in PRs (`grim`, `slurp`, `wl-clipboard`, `gtk4-layer-shell`).

## Git Workflow
- `main` must stay release-ready; merge only tested release changes.
- `develop` is the integration branch for the next release cycle.
- Create feature work on `feature/*` branches from `develop`.
- Merge `feature/*` into `develop` with **squash merge** to keep history concise.
- Promote releases by merging `develop` into `main` at release time.
- Create a version tag (for example `v0.2.0`) immediately after merging to `main`.
- For urgent production fixes, branch from `main` as `hotfix/*`, merge to `main` first, then back-merge the same fix into `develop`.

## Version & Release Notes
- Project versioning follows Semantic Versioning (SemVer).
- Canonical app version lives in the workspace root `Cargo.toml` (`[workspace.package].version = "X.Y.Z"`).
- For a new upstream release, update `Cargo.toml`, run `cargo check` to regenerate `Cargo.lock`, create annotated Git tag `vX.Y.Z`, and keep AUR metadata in sync.
- **Always commit `Cargo.lock` together with `Cargo.toml`** when bumping the version — CI uses `--locked` and will fail if the lock file is stale.
- Update `PKGBUILD` with `pkgver=X.Y.Z` and reset `pkgrel=1` when `pkgver` changes.
- If only packaging metadata changes (no upstream version bump), keep `pkgver` and increment `pkgrel`.
- After tag push, refresh AUR metadata with `updpkgsums` and regenerate `.SRCINFO` via `makepkg --printsrcinfo > .SRCINFO`.
- Commit packaging updates with only `PKGBUILD` and `.SRCINFO` for AUR sync flow.
- Preserve upstream attribution and license texts when reshaping packaging/docs; `NOTICE`, `LICENSE-MIT`, and `LICENSE-APACHE` are part of the release surface.

## Configuration & Runtime Notes
- User config files live at `$XDG_CONFIG_HOME/jjaeng/` (fallback `$HOME/.config/jjaeng/`), with fallback reads from legacy `chalkak/` paths for compatibility.
- Temporary captures are created under `$XDG_RUNTIME_DIR/jjaeng/` (fallback `/tmp/jjaeng`).
