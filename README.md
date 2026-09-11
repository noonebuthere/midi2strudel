midi2strudel
======
Convert a midi file to [strudel.cc](https://strudel.cc) code. Made in 9h for
[Aria](https://aria.hackclub.com), a [Hackclub](https://hackclub.com) event.

## Usage
`midi2strudel <file.mid> [opt-level]`
- file.mid is the path to a valid midi file
- opt-level is an integer 0 ~ 1 (default 0), see below

The resulting file will be placed as `midi2strudel.js` next to the
input file.

## Optimisations
A higher opt level reduces the size of the resulting file, also improving
playback performance in Strudel.

### opt-level = 0
No optimisations. Every beat is a single notated note, rest or hold.
The size of beats (4th, 8th, 16th, ...) is determined by the program based
on the input midi. Files upwards of 1000 lines.

### opt-level >= 1
Consecutive rests and holds are collapsed to elongated versions (`-@8`,
for example). Shrinks the file size by an average factor of 15,
compared to opt-level 0.

## Build Instructions
1. Clone the repository
2. Run `cargo build --release` with a fairly recent Rust version
3. The binary can be found in `/target/release/midi2strudel`
