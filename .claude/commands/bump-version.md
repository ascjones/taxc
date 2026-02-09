---
allowed-tools: Bash(git describe:*), Bash(git log:*), Bash(git show:*), Bash(git rev-list:*), Bash(cargo check:*), Bash(cargo run:*), Bash(git add:*), Bash(git commit:*), Bash(git tag:*), Bash(diff:*)
description: Analyze commits and bump version (minor for interface changes, patch for internal)
---

# Bump Version

Analyze commits since the last version tag and determine the appropriate semver bump.

## Version Rules (pre-1.0)

**Minor bump (0.X.0)** - Interface/user-facing changes:
- New or changed CLI commands, subcommands, or aliases
- New, changed, or removed command-line options/flags
- Changes to input formats (CSV/JSON schema)
- Changes to output formats or structure
- Any breaking change

**Patch bump (0.0.X)** - Internal changes:
- Bug fixes that don't change the interface
- Refactoring, code cleanup
- Performance improvements
- Test additions or fixes
- Documentation updates
- Dependency updates (unless they change output)

## Steps

1. Read `Cargo.toml` to get the current version

2. Find the last version tag:
   ```bash
   git describe --tags --abbrev=0 2>/dev/null
   ```

3. Count commits since tag:
   ```bash
   git rev-list --count <tag>..HEAD
   ```
   If no commits, report "No changes to release" and stop.

4. **Check for schema changes** (automatic interface change detection):
   ```bash
   cargo run --quiet -- schema input 2>/dev/null | diff - schema/input.json
   cargo run --quiet -- schema output 2>/dev/null | diff - schema/output.json
   ```
   If either schema differs from the checked-in version, this is a **minor bump** (interface change).
   Report which schema changed and what the diff is.

5. Get commit list with hashes:
   ```bash
   git log --pretty=format:"%h %s" <tag>..HEAD
   ```

6. For commits that might affect the interface, inspect changed files:
   ```bash
   git show --name-only --pretty="" <hash>
   ```
   Pay attention to changes in `src/main.rs` and `src/cmd/` - these define the CLI.

7. Analyze each commit semantically:
   - Does it add/remove/change user-visible behavior?
   - Does it change command names, flags, or output?
   - Or is it purely internal (refactor, fix, perf)?

8. Present your analysis as a table:
   | Commit | Summary | Classification | Reason |

9. State your version bump decision with reasoning

10. Update `Cargo.toml` with the new version (use Edit tool)

11. Run `cargo check` to update Cargo.lock

12. **Update schemas if changed**:
    If schemas changed in step 4, regenerate and stage them:
    ```bash
    cargo run --quiet -- schema input > schema/input.json
    cargo run --quiet -- schema output > schema/output.json
    ```

13. Ask the user if they want to create the release commit and tag

14. If yes, run:
    ```bash
    git add Cargo.toml Cargo.lock schema/
    git commit -m "chore: release vX.Y.Z"
    git tag vX.Y.Z
    ```

15. Report success with the new version and tag name
