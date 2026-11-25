![FFmpeg Video & Audio Converter](assets/banner.png)

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Windows-blue?logo=windows" alt="Windows">
  <img src="https://img.shields.io/badge/Engine-FFmpeg-green" alt="FFmpeg">
  <img src="https://img.shields.io/badge/Language-C%2B%2B-orange" alt="C++">
  <img src="https://img.shields.io/badge/Interface-Qt5-41CD52?logo=qt" alt="Qt5">
  <img src="https://img.shields.io/badge/License-MIT-brightgreen" alt="MIT License">
  <img src="https://img.shields.io/badge/Price-Free-success" alt="Free">
</p>

**FFmpeg Video & Audio Converter is a free, open-source Windows desktop application for converting video and audio files without complicated commands.** Powered by FFmpeg and built with a convenient graphical interface, it makes multimedia conversion, video transcoding, audio encoding, and format changes accessible to beginners and experienced users alike.

## ⬇️ Installation

**Download the Windows application**

1. Open the repository's [Releases](https://github.com/kai415-debug/ffmpeg-video-audio-converter/releases) page.
2. Download the latest release archive **`FFmpeg Video & Audio Converter 4.1.2.zip`**.
3. Extract the archive into a folder of your choice.
4. Keep **`FFmpeg Video & Audio Converter 4.1.2.exe`** and `libvlc.dll` together in that folder.
5. Double-click **`FFmpeg Video & Audio Converter 4.1.2.exe`** to launch the application.


## ✨ Features

- 🎬 **Video Format Conversion** — Convert video files between popular multimedia formats using the powerful FFmpeg processing engine.
- 🎵 **Audio File Conversion** — Transform audio files into different formats while configuring encoding options to suit your needs.
- 📦 **Batch Media Processing** — Queue multiple media files and process conversion jobs through a unified desktop workflow.
- ⚡ **FFmpeg-Powered Encoding** — Benefit from FFmpeg's extensive codec and container support for flexible video and audio transcoding.
- 🎛️ **Custom Encoding Presets** — Choose convenient conversion profiles or customize available parameters for greater control over output files.
- 🖥️ **User-Friendly Windows GUI** — Manage media conversion through a graphical interface instead of manually entering terminal commands.
- 🔧 **Advanced Conversion Options** — Adjust supported FFmpeg parameters for workflows that require more precise control over encoding.

## 🖼️ Overview

![Features](assets/features.png)

### Supported Media Workflows

The application brings common multimedia conversion tasks together in one Windows desktop environment.

| Workflow | Purpose |
|---|---|
| Video conversion | Change video formats and containers |
| Audio conversion | Convert audio between supported formats |
| Video transcoding | Re-encode media using FFmpeg |
| Batch conversion | Process multiple files in sequence |
| Preset encoding | Apply predefined conversion settings |
| Custom encoding | Configure available FFmpeg options |

### Built Around FFmpeg

[FFmpeg](https://ffmpeg.org/) provides the underlying multimedia processing capabilities. Its broad ecosystem of codecs, encoders, decoders, and container formats makes it suitable for everyday media conversion as well as more specialized transcoding tasks.

The graphical interface helps make these capabilities easier to access without requiring users to memorize FFmpeg command-line syntax.

## 🚀 Usage

Getting started with desktop media conversion is straightforward:

1. **Launch the application** — Open the FFmpeg Video & Audio Converter on your Windows PC.
2. **Select source media** — Add the video or audio files you want to convert.
3. **Choose an output format** — Select an available format or conversion profile.
4. **Configure encoding settings** — Adjust supported quality, codec, and other conversion parameters as needed.
5. **Start conversion** — Run the conversion job and allow FFmpeg to process your media.
6. **Find your output files** — Open the destination folder and use your converted media.

### 💡 Conversion Tips

- Choose an appropriate output container for the device or application where the media will be used.
- Use a compatible codec when sharing files across different operating systems and media players.
- Keep the original source file when experimenting with custom encoding settings.
- Remember that converting a compressed file into a different format does not necessarily improve its original quality.
- For faster processing, consider the relationship between output quality, resolution, codec, and encoding speed.

## 🪟 Why Choose a Graphical FFmpeg Converter?

FFmpeg is exceptionally capable, but command-line conversion can be intimidating when you only need to change a file format.

This Windows FFmpeg GUI provides a more approachable workflow:

- No need to construct lengthy terminal commands for routine conversions.
- Manage video and audio conversion from a single desktop application.
- Access FFmpeg-powered media processing through graphical controls.
- Use presets for common conversion scenarios.
- Explore additional encoding parameters when standard settings are insufficient.

Whether you are preparing videos for playback, converting audio collections, or handling everyday multimedia files, a dedicated FFmpeg desktop interface can simplify the process.

## 🔒 Security & Privacy

Your media files deserve a transparent and privacy-conscious workflow.

- **No telemetry by design** — The application is intended for local desktop media conversion, not user tracking.
- **No advertising framework** — The application is designed as a straightforward open-source utility.
- **Open-source transparency** — Inspect the source code and review how the application operates.
- **Local media processing** — Conversion is performed through the installed application and its FFmpeg processing components.
- **Dependency awareness** — FFmpeg and other bundled or required components may have their own licensing and security considerations.

### Reporting Security Issues

If you discover a potential security vulnerability, please avoid publishing sensitive exploit details in a public issue.

Use the repository's available private security reporting channel, if enabled, or contact the maintainers through an appropriate private channel.

For ordinary problems, conversion errors, and feature suggestions, open a GitHub issue with:

- Your Windows version.
- The application version.
- The input and intended output formats.
- Relevant error messages.
- Steps to reproduce the problem.

## 🛠️ Technology Stack

| Component | Technology |
|---|---|
| Programming language | C++ |
| Desktop framework | Qt 5 |
| Multimedia engine | FFmpeg |
| Interface technology | Qt / QML |
| Target platform | Microsoft Windows |

## 🤝 Contributing

Contributions are welcome from developers, testers, and multimedia enthusiasts.

Potential areas for improvement include:

- Windows compatibility and application stability.
- Additional conversion presets and supported formats.
- Improvements to the graphical user interface.
- Batch processing and job management.
- FFmpeg integration and error handling.
- Documentation, translations, and usability improvements.

### Contribution Guidelines

1. Fork the repository.
2. Create a dedicated branch for your changes.
3. Keep modifications focused and documented.
4. Test relevant conversion workflows.
5. Submit a pull request describing your improvements.

Please include reproducible details when reporting bugs or proposing changes.

## ❓ Frequently Asked Questions

### Is this a free Windows video converter?

Yes. The application is designed as free, open-source desktop software.

### Does it require FFmpeg?

Yes. FFmpeg provides the multimedia processing engine used for video and audio conversion. The availability of conversion features depends on the FFmpeg build and its supported codecs.

### Can I convert audio files?

Yes. Audio conversion is one of the application's primary workflows, subject to the formats and encoders supported by the configured FFmpeg installation.

### Can I convert multiple files?

The application supports a queue-oriented media conversion workflow for handling multiple files.

### Do I need command-line experience?

No. The graphical interface is intended to make common FFmpeg conversion operations accessible without manually typing commands.

### Does converting a video reduce its quality?

Not necessarily. Remuxing compatible streams can preserve their original quality, while re-encoding may introduce quality loss depending on the selected codec and settings.

### Is FFmpeg the same as a video converter?

FFmpeg is a multimedia framework and command-line toolkit. This application provides a graphical desktop interface for accessing its media conversion capabilities.

## 📚 Useful Resources

- [FFmpeg Official Website](https://ffmpeg.org/)
- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)
- [Qt Documentation](https://doc.qt.io/)
- [GitHub Releases](../../releases)
- [GitHub Issues](../../issues)

## 📄 License

This project is distributed under the **MIT License**. See the `LICENSE` file for the complete license terms. Third-party dependencies, including FFmpeg and Qt, retain their respective licenses.

---

**Convert video and audio files on Windows with the flexibility of FFmpeg and the convenience of a graphical interface.**