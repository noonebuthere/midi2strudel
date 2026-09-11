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
    #[allow(unused)]
    SetTimeSig(u8, u8, u8, u8),
    /// (sf, is_minor) - sf is amount of accidentals, neg for flats, pos for sharp
    #[allow(unused)]
    SetKeySig(i8, u8),
}

fn process_chunk(mut chunk: Chunk) -> ChunkProcessingResult {
    if chunk.tp != "MTrk" {
        return ChunkProcessingResult::UnknownChunkType;
    }

    let mut events = Vec::new();
    let mut track_name = String::new();
    let mut running_status: Option<u8> = None;

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

        let peeked = *chunk.data.last().unwrap();
        let event_id = if peeked & 0x80 != 0 {
            running_status = Some(peeked);
            chunk.data.pop().unwrap()
        } else {
            running_status.expect("Malformed midi file - illegal status")
        };

        if event_id == 0xFF {
            running_status = None;
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
                    let len = chunk.data.pop().unwrap();
                    // TODO: parse vlq
                    for _ in 0..len {
                        let _ = chunk.data.pop().unwrap();
                    }
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
                let velocity = chunk.data.pop().unwrap();
                events.push(Event {
                    deltatime: dt,
                    tp: if velocity == 0 {
                        EventType::MidiOff(note_number)
                    } else {
                        EventType::MidiOn(note_number)
                    },
                });
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

        for (i, _) in lanes.iter().enumerate() {
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

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn parse_midi(
    mut data: Vec<u8>,
    opt_level: u8,
) -> Result<(HeaderData, Vec<QuantizedTrack>), String> {
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

    // BEGIN AI
    
    let mut converted: Vec<(String, Option<u32>, Vec<NoteEvent>)> = Vec::new();
    let mut all_ticks: Vec<u32> = Vec::new();

    for track in tracks {
        let (tempo, note_events) = events_to_notes(track.events);
        for note in &note_events {
            all_ticks.push(note.start);
            all_ticks.push(note.start + note.duration);
        }
        converted.push((track.name, tempo, note_events));
    }

    let division = hdr_data.division.max(1) as u32;
    let tolerance = (division / 64).max(2);
    let subdivisions_per_quarter = find_best_subdivision(division, &all_ticks, tolerance);
    let global_division = (division / subdivisions_per_quarter).max(1);

    // END AI

    let mut laned_tracks = Vec::new();
    for (name, tempo, note_events) in converted {
        let lanes = assign_lanes(note_events);
        let (speed_mod, quantized_lanes) =
            quantize_lanes(lanes, global_division, hdr_data.division as u32, opt_level);
        laned_tracks.push(QuantizedTrack {
            lanes: quantized_lanes,
            name,
            speed_mod,
            tempo,
        });
    }

    Ok((hdr_data, laned_tracks))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum QuantizedNoteType {
    Note(u8),
    Rest,
    Hold,
    /// used in opt=1 to ignore step in codegen
    Skip,
}

#[derive(Clone, Debug)]
struct QuantizedNote {
    tp: QuantizedNoteType,
    len: u32,
}

impl QuantizedNote {
    fn new(tp: QuantizedNoteType) -> Self {
        Self { tp, len: 1 }
    }

    fn repr(&self) -> String {
        let mut repr = match self.tp {
            QuantizedNoteType::Note(n) => n.to_string(),
            QuantizedNoteType::Rest => "-".to_string(),
            QuantizedNoteType::Hold => "_".to_string(),
            QuantizedNoteType::Skip => "".to_string(),
        };

        if self.len > 1 {
            repr.push_str(format!("@{}", self.len).as_str());
        }

        repr
    }
}

fn quantize_lanes(
    lanes: Vec<Vec<NoteEvent>>,
    smallest_division: u32,
    hdr_division: u32,
    opt_level: u8,
) -> (u32, Vec<Vec<QuantizedNote>>) {
    let smallest_division = smallest_division.max(1);
    let speed_mod = hdr_division.div_ceil(smallest_division);
    let mut quantized_lanes = Vec::new();

    let round_div =
        |ticks: u32, div: u32| -> usize { (ticks as f64 / div as f64).round() as usize };

    for lane in lanes {
        let mut max_end_tick = 0;
        for note in &lane {
            max_end_tick = max_end_tick.max(note.start + note.duration);
        }
        let total_steps = round_div(max_end_tick, smallest_division).max(1);
        let mut steps = vec![QuantizedNote::new(QuantizedNoteType::Rest); total_steps];

        for note in lane {
            let start_step = round_div(note.start, smallest_division).min(total_steps - 1);
            let duration_steps = round_div(note.duration, smallest_division).max(1);

            steps[start_step] = QuantizedNote::new(QuantizedNoteType::Note(note.pitch));

            for i in 1..duration_steps {
                let idx = start_step + i;
                if idx < total_steps {
                    steps[idx] = QuantizedNote::new(QuantizedNoteType::Hold);
                }
            }
        }

        if opt_level >= 1 {
            let mut new_steps = Vec::new();
            'outer: for (i, step) in steps.into_iter().enumerate() {
                if i == 0 {
                    new_steps.push(step);
                    continue;
                }

                if let QuantizedNoteType::Note(_) = step.tp {
                    new_steps.push(step);
                    continue;
                }

                for idx in (0..i).rev() {
                    if new_steps[idx].tp == step.tp {
                        new_steps[idx].len += 1;
                        new_steps.push(QuantizedNote::new(QuantizedNoteType::Skip));
                        continue 'outer;
                    }
                    if new_steps[idx].tp == QuantizedNoteType::Skip {
                        continue;
                    }
                    break;
                }
                new_steps.push(step);
            }
            steps = new_steps;
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
    /// in microseconds per quarter note
    tempo: Option<u32>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.len() < 1 || args.len() > 2 {
        eprintln!("Usage: midi2strudel <file.mid> [opt-level]");
        eprintln!("- [opt-level] how strongly to optimise the output: integer 0 ~ 1 (default 0)");
        std::process::exit(1);
    }
    let path = PathBuf::from(args.next().unwrap());
    if !path.exists() | !path.is_file() {
        eprintln!("{} is not a file or does not exist", path.display());
        std::process::exit(1);
    }

    let mut opt_level: u8 = 0;
    if let Some(s) = args.next() {
        if let Ok(n) = s.parse::<u8>() {
            opt_level = n;
        } else {
            eprintln!("opt-level is invalid! must be integer 0 ~ 255");
            std::process::exit(1);
        }
    }

    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error while reading file: {}", e);
            std::process::exit(1);
        }
    };
    let parsed = parse_midi(data, opt_level);
    if let Err(e) = parsed {
        eprintln!("Error while parsing midi: {}", e);
        std::process::exit(1);
    }
    let (header, tracks) = parsed.unwrap();

    let mut code = format!(
        "// generated by midi2strudel\n// track count: {}\n\n",
        header.ntrks
    );
    let mut tempo = 500_000; // midi default tempo 120bpm
    for track in tracks {
        if let Some(t) = track.tempo {
            tempo = t;
        }
        code.push_str(codegen_track(track).as_str());
    }
    code.insert_str(
        0,
        format!(
            "setcpm({})\n\n",
            (1_000_000.0 * 60.0 * 4.0 / tempo as f32).round()
        )
        .as_str(),
    );

    println!("{}", code);
}

fn codegen_track(track: QuantizedTrack) -> String {
    let mut lane_reprs = Vec::new();
    for lane in track.lanes {
        let mut lane_repr = lane
            .iter()
            .fold(String::new(), |acc, x| {
                if acc.is_empty() {
                    x.repr()
                } else if x.repr().is_empty() {
                    acc
                } else {
                    acc + " " + &x.repr()
                }
            });
        lane_repr.insert(0, '<');
        lane_repr.push_str(format!(">*{}", track.speed_mod).as_str());

        let split_amt = 120;
        let mut new_lane_repr = String::new();
        for (i, c) in lane_repr.chars().enumerate() {
            if i != 0 && i % split_amt == 0 {
                new_lane_repr.push('\n');
            }
            new_lane_repr.push(c);
        }

        lane_reprs.push(new_lane_repr);
    }

    let mut code = format!(
        "${}: note(`\n  ",
        track
            .name
            .replace(|c: char| { !c.is_ascii_alphanumeric() }, "")
    );
    code.push_str(lane_reprs.join(",\n  ").as_str());
    code.push_str("\n`).sound(\"<TODO>\")\n\n");
    code
}

#[derive(Debug)]
struct NoteEvent {
    pitch: u8,
    start: u32,
    duration: u32,
}

fn events_to_notes(track: Vec<Event>) -> (Option<u32>, Vec<NoteEvent>) {
    let mut time = 0;
    let mut lane = Vec::new();
    let mut pending_notes: HashMap<u8, Vec<u32>> = HashMap::new();
    let mut tempo = None;
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
            EventType::SetTempo(n) => {
                tempo = Some(n);
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
                if duration == 0 {
                    eprintln!(
                        "Warning: Note {}:{} has zero duration, discarding.",
                        note, start
                    );
                    continue;
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
                let duration = time - t;
                if duration == 0 {
                    eprintln!(
                        "Warning: Malformed midi file (Note {}:{} ON without matching OFF, and implied OFF would be zero-duration), discarding.",
                        k, t
                    );
                    continue;
                }
                eprintln!(
                    "Warning: Malformed midi file (Note {}:{} ON without matching OFF), assuming implied OFF.",
                    k, t
                );
                lane.push(NoteEvent {
                    pitch: k,
                    start: t,
                    duration,
                })
            }
        }
    }

    (tempo, lane)
}
