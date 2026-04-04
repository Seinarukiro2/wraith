# codeguard

Deterministic linter for AI-generated Python code. Rust workspace, ships as a single binary via `pip install codeguard`.

## Build

```bash
cargo build            # debug build (excludes Python bindings)
cargo build --release  # release build
cargo test             # run all tests (44 tests)
```

Python bindings (requires maturin):
```bash
pip install maturin
maturin develop        # build + install into current venv
```

## Project structure

```
crates/
  codeguard-core/       # Diagnostic, Span, TextEdit, Config, Reporter, RuleCode
  codeguard-ast/        # tree-sitter-python parsing, FileInfo extraction, LineIndex
  codeguard-api-guard/  # AG001-AG003: hallucinated API detection via Python introspection
  codeguard-phantom/    # PH001-PH003: phantom package detection via PyPI + SQLite cache
  codeguard-vibe/       # VC001-VC006: AI artifact hygiene (secrets, debug code, AI comments)
  codeguard-cli/        # CLI entry point (clap, rayon parallel scanning)
python/codeguard/       # PyO3 bindings (excluded from default cargo build)
```

## Usage

```bash
codeguard check ./src/                     # check all .py files
codeguard check ./src/ --select AG,VC      # only API Guard + Vibe Check
codeguard check ./src/ --fix               # apply autofixes
codeguard check ./src/ --strict            # exit code 1 on issues
codeguard check ./src/ --offline           # skip PyPI HTTP
codeguard check ./src/ --format json       # JSON output
codeguard rules                            # list all rules
```

## Rules

- `AG0xx` — API Guard: hallucinated attributes (AG001), kwargs (AG002), deprecated (AG003)
- `PH0xx` — Phantom: not on PyPI (PH001), not installed (PH002), suspicious/typosquat (PH003)
- `VC0xx` — Vibe Check: secrets (VC001), AI comments (VC002), debug code (VC003), pdb (VC004), source maps (VC005), unauthed endpoints (VC006)

## Architecture notes

- AG and PH modules use batch architecture: collect queries across all files → prefetch (Python subprocess / HTTP) → lint in parallel with rayon
- Python bindings crate (`python/codeguard`) is in `default-members` exclusion — built only via `maturin develop`, not `cargo build`
- PyPI cache: `~/.cache/codeguard/pypi.db` (SQLite, 24h TTL)
- API Guard introspection: single Python subprocess, batch JSON stdin/stdout
