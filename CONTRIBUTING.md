# Contributing to SessionWeave

Thank you for your interest in contributing to SessionWeave! 🎉

## Development Setup

1. **Prerequisites**
   - Rust (latest stable recommended)
   - Git
   - (Optional but recommended) Ollama + `nomic-embed-text`

2. **Clone & Build**

```bash
git clone https://github.com/johanvillalba/sessionweave.git
cd sessionweave
cargo build
```

3. **Run tests**

```bash
cargo test
```

4. **Run the tool**

```bash
cargo run -- index tests/fixtures/sample_claude.jsonl
cargo run -- resume "auth"
```

## Code Style

- Follow standard Rust formatting (`cargo fmt`).
- Keep the CLI experience fast and predictable.
- Prefer graceful degradation when external services (Ollama, LanceDB) are unavailable.
- Add comments for non-obvious logic, especially around parsing and embeddings.

## Commit Guidelines

- Use conventional commits when possible (`feat:`, `fix:`, `docs:`, `refactor:`).
- Keep commits focused.

## Pull Request Process

1. Create a feature branch from `main`.
2. Make your changes + add/update tests when relevant.
3. Run `cargo fmt`, `cargo clippy`, and `cargo test`.
4. Open a Pull Request with a clear description of the change and motivation.

## Reporting Issues

When reporting bugs, please include:
- `sw --version`
- Operating system
- Steps to reproduce
- Relevant logs or session excerpts (redacted if necessary)

## Questions?

Open a GitHub Discussion or an issue with the `question` label.

We appreciate thoughtful contributions that help power users keep control of their AI coding memory!