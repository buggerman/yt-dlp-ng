# yt-dlp-ng (Next Generation)

[![CI](https://github.com/buggerman/yt-dlp-ng/actions/workflows/ci.yml/badge.svg)](https://github.com/buggerman/yt-dlp-ng/actions/workflows/ci.yml)
[![Release](https://github.com/buggerman/yt-dlp-ng/actions/workflows/release.yml/badge.svg)](https://github.com/buggerman/yt-dlp-ng/actions/workflows/release.yml)
[![codecov](https://codecov.io/gh/buggerman/yt-dlp-ng/branch/main/graph/badge.svg)](https://codecov.io/gh/buggerman/yt-dlp-ng)
[![Crates.io](https://img.shields.io/crates/v/yt-dlp-ng.svg)](https://crates.io/crates/yt-dlp-ng)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A modern, high-performance implementation of yt-dlp built with Rust, focusing on speed, reliability, and maintainability.

## Features

- **🚀 High Performance**: Built with Rust and async/await for maximum performance
- **📦 Resume Downloads**: Automatically resume interrupted downloads
- **🔄 Progress Reporting**: Real-time progress updates with byte-accurate reporting
- **🛡️ Anti-Detection**: Advanced anti-detection measures to bypass restrictions
- **🎯 Signature Decryption**: Modern signature decryption based on yt-dlp's approach
- **📊 Comprehensive Testing**: Extensive test suite ensuring reliability
- **🔧 Modular Architecture**: Easy to extend with new extractors

## Installation

### From Pre-built Binaries

Download the latest release from the [releases page](https://github.com/buggerman/yt-dlp-ng/releases).

### From Source

```bash
cargo install yt-dlp-ng
```

### Using Homebrew (macOS)

```bash
brew install buggerman/tools/yt-dlp-ng
```

### Using Snap (Linux)

```bash
snap install yt-dlp-ng
```

### Using Flatpak (Linux)

```bash
flatpak install flathub com.github.buggerman.yt-dlp-ng
```

## Usage

### Basic Usage

```bash
# Download a video
yt-dlp-ng "https://www.youtube.com/watch?v=dQw4w9WgXcQ"

# Download to specific directory
yt-dlp-ng "https://www.youtube.com/watch?v=dQw4w9WgXcQ" -o ./downloads

# Download with specific format
yt-dlp-ng "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --format mp4

# Download with custom filename template
yt-dlp-ng "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --template "%(uploader)s - %(title)s.%(ext)s"
```

### Command Line Options

```
USAGE:
    yt-dlp-ng [OPTIONS] <URL>

ARGS:
    <URL>    The URL to download

OPTIONS:
    -o, --output <OUTPUT>        Output directory [default: .]
    -f, --format <FORMAT>        Video format preference [default: best]
    -t, --template <TEMPLATE>    Output filename template [default: %(title)s.%(ext)s]
    -c, --concurrent <CONCURRENT> Number of concurrent downloads [default: 4]
    -v, --verbose                Enable verbose logging
    -h, --help                   Print help information
    -V, --version                Print version information
```

### Filename Templates

- `%(title)s` - Video title
- `%(uploader)s` - Video uploader
- `%(id)s` - Video ID
- `%(ext)s` - File extension
- `%(upload_date)s` - Upload date
- `%(duration)s` - Video duration

## Development Status

**Current Status**: Production-ready for YouTube downloads

**Completed Features**:
- [x] Complete project structure and architecture
- [x] Modern CLI interface with clap
- [x] Core extractor framework with trait system
- [x] Full YouTube extractor with signature decryption
- [x] Download engine with streaming support
- [x] Progress reporting with real-time updates
- [x] Resume capability for interrupted downloads
- [x] Anti-detection measures and retry logic
- [x] Comprehensive test suite (10 tests)
- [x] CI/CD pipeline with GitHub Actions
- [x] Security audits and code quality checks
- [x] Cross-platform builds (Linux, macOS, Windows)

## Architecture

The project is structured as:

- `src/cli/` - Command-line interface
- `src/core/` - Core download and extraction engine
- `src/extractors/` - Site-specific extractors
- `src/config/` - Configuration management
- `src/utils/` - Utility functions

## Contributing

This project is in early development. Contributions are welcome!

## License

MIT License