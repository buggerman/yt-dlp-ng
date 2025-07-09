# Contributing to yt-dlp-ng

Thank you for your interest in contributing to yt-dlp-ng! We welcome contributions from the community.

## Development Setup

1. **Clone the repository**:
   ```bash
   git clone https://github.com/buggerman/yt-dlp-ng.git
   cd yt-dlp-ng
   ```

2. **Install Rust** (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source ~/.cargo/env
   ```

3. **Install dependencies**:
   ```bash
   cargo build
   ```

4. **Run tests**:
   ```bash
   cargo test
   ```

5. **Install pre-commit hooks** (optional but recommended):
   ```bash
   pip install pre-commit
   pre-commit install
   ```

## Development Workflow

1. **Create a feature branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes**:
   - Write code following Rust best practices
   - Add tests for new functionality
   - Update documentation as needed

3. **Test your changes**:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt -- --check
   ```

4. **Commit your changes**:
   ```bash
   git add .
   git commit -m "feat: description of your changes"
   ```

5. **Push and create a pull request**:
   ```bash
   git push origin feature/your-feature-name
   ```

## Code Standards

- **Formatting**: Use `cargo fmt` to format code
- **Linting**: Use `cargo clippy` and fix all warnings
- **Testing**: Add tests for all new functionality
- **Documentation**: Update documentation for public APIs
- **Commit Messages**: Use conventional commit format

### Commit Message Format

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:
- `feat(downloader): add resume capability`
- `fix(youtube): handle signature decryption edge case`
- `docs(readme): update installation instructions`

## Testing

- Run all tests: `cargo test`
- Run specific test: `cargo test test_name`
- Run tests with output: `cargo test -- --nocapture`

## Code Review Process

1. All submissions require review
2. CI must pass (tests, linting, formatting)
3. At least one maintainer approval required
4. No merge conflicts with main branch

## Reporting Issues

When reporting issues, please include:

- Operating system and version
- Rust version (`rustc --version`)
- URL that's failing (if applicable)
- Complete error message
- Steps to reproduce

## Feature Requests

Before submitting a feature request:

1. Check existing issues to avoid duplicates
2. Provide clear use case and motivation
3. Consider implementation complexity
4. Be open to discussion and alternatives

## Security Issues

For security-related issues, please email the maintainers directly rather than opening a public issue.

## Questions?

Feel free to open an issue for questions about contributing or development setup.

Thank you for contributing! 🦀