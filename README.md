# r_webAudioProv
Web audio provider rewritten in Rust
[Original project](https://github.com/lukasz26671/webAudioProv)
## This is an attempt of learning a new language by making something that I'd actually use

Unsurprisingly it works even better :o

## Requirements
To run this server, you need to place the following binaries in the root folder:
- [yt-dlp.exe](https://github.com/yt-dlp/yt-dlp/releases/latest)
- [ffmpeg.exe & ffprobe.exe](https://www.gyan.dev/ffmpeg/builds/) (Extract from the `bin` folder of the essential build)

## How to run
1. Ensure `yt-dlp.exe` and `ffmpeg.exe` are in the project root.
2. Run `cargo run --release`.
3. Open `http://localhost:8080` in your browser.