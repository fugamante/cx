# XSHELF Suite Install

The suite install keeps `XSHELF` and `cxops` in separate repositories while
presenting one local product surface.

## Local Install

From the `XSHELF` repository:

```bash
./bin/xshelf-suite-install
```

The installer:

- links `xshelf`, `xs`, and `cx` into `~/.local/bin`
- installs `cxops` and `cxopsj` from the sibling `cx-eval-lab` or `cx-ops` repo
- creates `~/Desktop/XSHELF.command`
- verifies `xshelf version` and `cxops version`

If the control-plane repo is somewhere else:

```bash
./bin/xshelf-suite-install --cx-ops-repo /path/to/cx-eval-lab
```

Shell profile edits are opt-in:

```bash
./bin/xshelf-suite-install --shell
```

## Launch

After install:

```bash
xshelf launch
```

This delegates to `cxops bringup` and opens the local relay UI through
`cxops ui --local`. If `cxops` is not on `PATH`, `xshelf launch` also checks
`~/.cargo/bin/cxops`, which is the default `cargo install` destination.

For automation:

```bash
xshelf launch --json
xshelf launch --no-open
```
