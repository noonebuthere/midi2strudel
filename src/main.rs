use std::collections::HashMap;
use std::path::PathBuf;

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
        loop {
            let b = chunk.data.pop().unwrap();
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
                        tp: EventType::SetTimeSig(nn, dd, cc, bb),
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


// interval partitioning problem, apparently
// reference: https://www.youtube.com/watch?v=i_G8hZYcKnI
fn assign_lanes(mut notes: Vec<NoteEvent>) -> Vec<Vec<NoteEvent>> {
    notes.sort_by_key(|event| event.start);

    let mut lanes: Vec<Vec<NoteEvent>> = Vec::new();
    let mut lanes_end: Vec<u32> = Vec::new();

    for note in notes {
        let mut best_lane = None;
        let mut best_end = None;

        for (i, lane) in lanes.iter().enumerate() {
            let lane_end = lanes_end[i];
            if lane_end <= note.start && (best_end.is_none() || lane_end > best_end.unwrap()) {
                best_lane = Some(i);
                best_end = Some(lane_end);
            }
        }
        if best_lane == None {
            lanes.push(Vec::new());
            lanes_end.push(0);
            best_lane = Some(lanes.len() - 1);
        }

        lanes_end[best_lane.unwrap()] = note.start + note.duration;
        lanes.get_mut(best_lane.unwrap()).unwrap().push(note);
    }

    lanes
}

fn parse_midi(mut data: Vec<u8>) -> Result<(HeaderData, Vec<QuantizedTrack>), String> {
    data.reverse();
    let chunks = parse_chunks(data)?;
    if chunks.len() < 2 {
        return Err("Midi is not valid - invalid chunk count.".to_string());
    }
    let mut chunks = chunks.into_iter();
    let hdr_data = process_header_chunk(&chunks.next().unwrap())?;

    if hdr_data.format == 2 {
        eprintln!("midi2strudel does not support format 2 midi files, sorry!");
        std::process::exit(1);
    }
    if hdr_data.format > 2 {
        eprintln!("Midi is not valid - invalid format.");
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

    let mut laned_tracks = Vec::new();
    for track in tracks {
        println!("{:?}", track);
        let (smallest_division, note_events) = events_to_notes(track.events);
        let lanes = assign_lanes(note_events);
        let (speed_mod, quantized_lanes) = quantize_lanes(lanes, smallest_division, hdr_data.division as u32);
        laned_tracks.push(QuantizedTrack {
            lanes: quantized_lanes,
            name: track.name,
            speed_mod,
        });
    }

    Ok((hdr_data, laned_tracks))
}

#[derive(Clone, Debug)]
enum QuantizedNote {
    Note(u8),
    Rest,
    Hold
}

fn quantize_lanes(lanes: Vec<Vec<NoteEvent>>, smallest_division: u32, hdr_division: u32) -> (u32, Vec<Vec<QuantizedNote>>) {
    let speed_mod = hdr_division.div_ceil(smallest_division);
    let mut quantized_lanes = Vec::new();
    
    for lane in lanes {
        let mut max_end_tick = 0;
        for note in &lane {
            max_end_tick = max_end_tick.max(note.start + note.duration);
        }
        let total_steps = max_end_tick.div_ceil(smallest_division) as usize;
        let mut steps = vec![QuantizedNote::Rest; total_steps];
        
        for note in lane {
            let start_step = note.start.div_ceil(smallest_division) as usize;
            let duration_steps = note.duration.div_ceil(smallest_division) as usize;
            
            steps[start_step] = QuantizedNote::Note(note.pitch);
            
            for i in 1..duration_steps {
                let idx = start_step + i;
                if idx < total_steps {
                    steps[idx] = QuantizedNote::Hold;
                }
            }
        }
        quantized_lanes.push(steps);
    }
    
    (speed_mod, quantized_lanes)
}

#[derive(Debug)]
struct QuantizedTrack {
    lanes: Vec<Vec<QuantizedNote>>,
    name: String,
    speed_mod: u32,
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
    let parsed = parse_midi(data);
    if let Err(e) = parsed {
        eprintln!("Error while parsing midi: {}", e);
        std::process::exit(1);
    }
    let (header, tracks) = parsed.unwrap();
    println!("HEADER:\n{:#?}\n", header);
    println!("TRACKS:\n{:#?}", tracks);
    for track in tracks {
        codegen_track(track);
    }
}

fn codegen_track(track: QuantizedTrack) -> String {
    let code = String::new();
    
    for lane in track.lanes {
        let lane = String::new();
        
    }
    
    code
}

#[derive(Debug)]
struct RawLane {
    notes: Vec<NoteEvent>,
    name: String,
}

#[derive(Debug)]
struct NoteEvent {
    pitch: u8,
    start: u32,
    duration: u32,
}
fn events_to_notes(track: Vec<Event>) -> (u32, Vec<NoteEvent>) {
    let mut time = 0;
    let mut lowest_duration = u32::MAX;
    let mut lane = Vec::new();
    let mut pending_notes: HashMap<u8, Vec<u32>> = HashMap::new();
    for event in track {
        time += event.deltatime;
        match event.tp {
            EventType::SetTimeSig(_, _, _, _) => {
                eprintln!("Warning: SetTimeSig Event not yet implemented, ignoring.");
                continue;
            }
            EventType::SetKeySig(_, _) => {
                eprintln!("Warning: SetKeySig Event not yet implemented, ignoring.");
                continue;
            }
            EventType::SetTempo(_) => {
                eprintln!("Warning: SetTempo Event not yet implemented, ignoring.");
                continue;
            }
            EventType::MidiOn(note) => {
                let handle = pending_notes.get_mut(&note);
                match handle {
                    Some(v) => v.push(time),
                    None => {
                        let _ = pending_notes.insert(note, vec![time]);
                    }
                }
            }
            EventType::MidiOff(note) => {
                if !pending_notes.contains_key(&note)
                    || pending_notes.get(&note).unwrap().is_empty()
                {
                    eprintln!(
                        "Warning: Malformed midi file (Note OFF without matching ON), ignoring."
                    );
                    continue;
                }
                let start = pending_notes.get_mut(&note).unwrap().pop().unwrap();
                let duration = time - start;
                if duration < lowest_duration {
                    lowest_duration = duration;
                }
                lane.push(NoteEvent {
                    pitch: note,
                    start,
                    duration,
                })
            }
        }
    }

    for (k, v) in pending_notes {
        if !v.is_empty() {
            for t in v {
                eprintln!(
                    "Warning: Malformed midi file (Note {}:{} ON without matching OFF), assuming implied OFF.",
                    k, t
                );
                lane.push(NoteEvent {
                    pitch: k,
                    start: t,
                    duration: time - t,
                })
            }
        }
    }

    (lowest_duration, lane)
}
