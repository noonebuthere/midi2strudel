use crate::EventType::SetTimeSig;
use std::path::PathBuf;

fn num_to_vlq(mut num: u32) -> Vec<u8> {
    let mut res = Vec::new();
    res.push((num & 0b01111111) as u8);
    num >>= 7;
    while num > 0 {
        res.push((num & 0b01111111) as u8 | 0b10000000);
        num >>= 7;
    }
    res.reverse();
    res
}

#[derive(Debug)]
struct Chunk {
    tp: String,
    len: u32,
    /// Pop to get next byte
    data: Vec<u8>,
}

fn parse_chunks(mut data: Vec<u8>) -> Result<Vec<Chunk>, String> {
    let mut chunks = Vec::new();
    while !data.is_empty() {
        let mut tp = String::with_capacity(4);
        for _ in 0..4 {
            tp.push(data.pop().unwrap() as char);
        }
        let mut len = 0;
        for i in (0..4).rev() {
            len += (data.pop().unwrap() as u32) << (i * 8);
        }
        let mut chunk_data = Vec::with_capacity(len as usize);
        for _ in 0..len {
            chunk_data.push(data.pop().unwrap());
        }
        chunk_data.reverse();
        chunks.push(Chunk {
            tp,
            len,
            data: chunk_data,
        })
    }

    Ok(chunks)
}

#[derive(Debug)]
struct HeaderData {
    format: u16,
    ntrks: u16,
    division: u16,
}

fn process_header_chunk(chunk: &Chunk) -> Result<HeaderData, String> {
    if chunk.tp != "MThd" || chunk.len != 6 {
        return Err("Midi is not valid - first chunk is not a valid header.".to_string());
    }
    let mut d = chunk.data.clone();
    d.reverse();
    Ok(HeaderData {
        format: ((d[0] as u16) << 8) | d[1] as u16,
        ntrks: ((d[2] as u16) << 8) | d[3] as u16,
        division: ((d[4] as u16) << 8) | d[5] as u16,
    })
}

enum ChunkProcessingResult {
    UnknownChunkType,
    ValidTrack(Track),
}

#[derive(Debug)]
struct Track {
    events: Vec<Event>,
    name: String,
}

#[derive(Debug)]
struct Event {
    deltatime: u32,
    tp: EventType,
}

#[derive(Debug)]
enum EventType {
    /// (note)
    MidiOn(u8),
    /// (note)
    MidiOff(u8),
    /// (us_per_quarter_note)
    SetTempo(u32),
    /// (numerator, denominator_as_exp_of_2, clocks_per_metronome_click, notated_32nds_per_quarter)
    SetTimeSig(u8, u8, u8, u8),
    /// (sf, is_minor) - sf is amount of accidentals, neg for flats, pos for sharp
    SetKeySig(i8, u8),
}

fn process_chunk(mut chunk: Chunk) -> ChunkProcessingResult {
    if chunk.tp != "MTrk" {
        return ChunkProcessingResult::UnknownChunkType;
    }

    let mut events = Vec::new();
    let mut track_name = String::new();

    while !chunk.data.is_empty() {
        // parse deltatime
        let mut dt: u32 = 0;
        while let b = chunk.data.pop().unwrap() {
            dt += (b & 0b01111111) as u32;
            // while bit 7 is set
            if b & 0b10000000 == 0x80 {
                dt <<= 7;
            } else {
                // bit 7 is unset
                break;
            }
        }

        let event_id = chunk.data.pop().unwrap();

        if event_id == 0xFF {
            let meta_type = chunk.data.pop().unwrap();

            match meta_type {
                0x00 => {
                    // sequence number
                    let _seq_number = chunk.data.pop().unwrap();
                }
                0x01..0x0F => {
                    // some kind of text event
                    let len = chunk.data.pop().unwrap();
                    for _ in 0..len {
                        let ch = chunk.data.pop().unwrap();
                        track_name.push(ch as char);
                    }
                }
                0x20 => {
                    // midi channel prefix
                    let _01 = chunk.data.pop().unwrap();
                    let _cc = chunk.data.pop().unwrap();
                }
                0x2F => {
                    // end of track
                    let _00 = chunk.data.pop().unwrap();
                }
                0x51 => 'mtch: {
                    // set tempo
                    if chunk.data.pop().unwrap() != 0x03 {
                        eprintln!("Warning: event FF 51 was not followed by 03, ignored");
                        break 'mtch;
                    }
                    let mut tempo: u32 = 0;
                    for _ in 0..3 {
                        tempo <<= 8;
                        tempo += chunk.data.pop().unwrap() as u32;
                    }
                    events.push(Event {
                        deltatime: dt,
                        tp: EventType::SetTempo(tempo),
                    });
                }
                0x54 => {
                    // SMPTE offset
                    let _05 = chunk.data.pop().unwrap();
                    let _hr = chunk.data.pop().unwrap();
                    let _mm = chunk.data.pop().unwrap();
                    let _se = chunk.data.pop().unwrap();
                    let _fr = chunk.data.pop().unwrap();
                    let _ff = chunk.data.pop().unwrap();
                }
                0x58 => 'mtch: {
                    // set time signature
                    if chunk.data.pop().unwrap() != 0x04 {
                        eprintln!("Warning: event FF 58 was not followed by 04, ignored");
                        break 'mtch;
                    }
                    let nn = chunk.data.pop().unwrap();
                    let dd = chunk.data.pop().unwrap();
                    let cc = chunk.data.pop().unwrap();
                    let bb = chunk.data.pop().unwrap();
                    events.push(Event {
                        deltatime: dt,
                        tp: SetTimeSig(nn, dd, cc, bb),
                    });
                }
                0x59 => 'mtch: {
                    // set time signature
                    if chunk.data.pop().unwrap() != 0x02 {
                        eprintln!("Warning: event FF 59 was not followed by 02, ignored");
                        break 'mtch;
                    }
                    let sf = chunk.data.pop().unwrap();
                    let mi = chunk.data.pop().unwrap();
                    events.push(Event {
                        deltatime: dt,
                        tp: EventType::SetKeySig(sf as i8, mi),
                    });
                }
                0x7F => {
                    // sequencer specific meta event
                    let len = chunk.data.pop().unwrap();
                    for _ in 0..len {
                        let _ = chunk.data.pop().unwrap();
                    }
                }
                x => {
                    eprintln!("Warning: invalid event 0xFF {:#x} encountered, ignored", x)
                }
            }
        }

        let upper = (event_id & 0b11110000) >> 4;
        let lower = event_id & 0b1111;
        match (upper, lower) {
            // CHANNEL VOICE
            (0b1000, _) => {
                // Note off
                let note_number = chunk.data.pop().unwrap();
                let _velocity = chunk.data.pop().unwrap();
                events.push(Event {
                    deltatime: dt,
                    tp: EventType::MidiOff(note_number),
                })
            }
            (0b1001, _) => {
                // Note on
                let note_number = chunk.data.pop().unwrap();
                let _velocity = chunk.data.pop().unwrap();
                events.push(Event {
                    deltatime: dt,
                    tp: EventType::MidiOn(note_number),
                })
            }
            (0b1010, _) => {
                // Aftertouch
                let _note_number = chunk.data.pop().unwrap();
                let _velocity = chunk.data.pop().unwrap();
            }
            (0b1011, _) => {
                // CC
                let _controller_number = chunk.data.pop().unwrap();
                let _value = chunk.data.pop().unwrap();
            }
            (0b1100, _) => {
                // Program change
                let _program_number = chunk.data.pop().unwrap();
            }
            (0b1101, _) => {
                // Channel pressure
                let _value = chunk.data.pop().unwrap();
            }
            (0b1110, _) => {
                // Pitch wheel change
                let _least = chunk.data.pop().unwrap();
                let _most = chunk.data.pop().unwrap();
            }
            // SYSTEM COMMON
            (0b1111, lower) => {
                match lower {
                    0b0000 => while chunk.data.pop().unwrap() != 0b11110111 {},
                    0b0001 => {
                        // ndef
                    }
                    0b0010 => {
                        // song position pointer
                        let _ptr = chunk.data.pop().unwrap();
                        let _reg = chunk.data.pop().unwrap();
                    }
                    0b0011 => {
                        // song select
                        let _song = chunk.data.pop().unwrap();
                    }
                    0b0100 => {
                        // ndef
                    }
                    0b0101 => {
                        // ndef
                    }
                    0b0110 => {
                        // tune rq
                    }
                    0b0111 => {
                        // End of exclusive, used above
                    }
                    // SYSTEM REAL TIME MESSAGES
                    0b1000 => {
                        // CLK
                    }
                    0b1001 => {
                        // ndef
                    }
                    0b1010 => {
                        // Start sequence
                    }
                    0b1011 => {
                        // Continue sequence
                    }
                    0b1100 => {
                        // Stop sequence
                    }
                    0b1101 => {
                        // ndef
                    }
                    0b1110 => {
                        // active sensing
                    }
                    0b1111 => {
                        // reset
                    }
                    _ => unreachable!(),
                }
            }
            (u, l) => {
                eprintln!(
                    "Warning: invalid event {:#010b} encountered, ignored",
                    u << 4 | l
                )
            }
        }
    }

    ChunkProcessingResult::ValidTrack(Track {
        events,
        name: track_name,
    })
}

fn parse_midi(mut data: Vec<u8>) -> Result<(HeaderData, Vec<Track>), String> {
    data.reverse();
    let chunks = parse_chunks(data)?;
    if chunks.len() < 2 {
        return Err("Midi is not valid.".to_string());
    }
    let mut chunks = chunks.into_iter();
    let hdr_data = process_header_chunk(&chunks.next().unwrap())?;

    if hdr_data.format == 2 {
        eprintln!("midi2strudel does not support format 2 midi files, sorry!");
        std::process::exit(1);
    }

    let mut tracks = Vec::new();
    for chunk in chunks {
        match process_chunk(chunk) {
            ChunkProcessingResult::UnknownChunkType => {
                eprintln!("Warning: encountered unknown chunk type, ignored.");
            }
            ChunkProcessingResult::ValidTrack(track) => {
                tracks.push(track);
            }
        }
    }
    Ok((hdr_data, tracks))
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.len() != 1 {
        eprintln!("Usage: midi2strudel <file.mid>");
        std::process::exit(1);
    }
    let path = PathBuf::from(args.next().unwrap());
    if !path.exists() | !path.is_file() {
        eprintln!("{} is not a file or does not exist", path.display());
        std::process::exit(1);
    }
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error while reading file: {}", e);
            std::process::exit(1);
        }
    };
    let midi = parse_midi(data);
    println!("{:?}", midi);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vlq_encoding() {
        assert_eq!(num_to_vlq(0), vec![0]);
        assert_eq!(num_to_vlq(0x40), vec![0x40]);
        assert_eq!(num_to_vlq(0x7F), vec![0x7F]);
        assert_eq!(num_to_vlq(0x80), vec![0x81, 0x00]);
        assert_eq!(num_to_vlq(0x2000), vec![0xC0, 0x00]);
        assert_eq!(num_to_vlq(0x3FFF), vec![0xFF, 0x7F]);
        assert_eq!(num_to_vlq(0x4000), vec![0x81, 0x80, 0x00]);
        assert_eq!(num_to_vlq(0x001FFFFF), vec![0xFF, 0xFF, 0x7F]);
        assert_eq!(num_to_vlq(0x08000000), vec![0xC0, 0x80, 0x80, 0x00]);
        assert_eq!(num_to_vlq(0x0FFFFFFF), vec![0xFF, 0xFF, 0xFF, 0x7F]);
    }
}
